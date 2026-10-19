use anyhow::{bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use horiplay_core::{InputSettings, Owner, PadInfo, Percentiles, TimingWindow, Watchdog};
use horiplay_gamepad::{SdlGamepads, SdlRuntime};
use horiplay_input_transport::{InputPacket, Receiver};
use horiplay_networking::{
    ClientMessage, Invite, PathInfo, Peer, PeerEvent, Role, ServerMessage, SignalAuth,
};
use horiplay_virtual_gamepad::{BackendStatus, Rumble, Viiper, VirtualGamepadBackend};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use webrtc::ice_transport::ice_server::RTCIceServer;
mod input;
pub mod performance;
use input::{GuestInput, InputFeed, Sample};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub signaling: String,
    pub ice_urls: Vec<String>,
    pub turn_username: String,
    pub turn_password: String,
    pub relay_only: bool,
    pub viiper_address: SocketAddr,
    pub display_name: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            signaling: "ws://127.0.0.1:8787/ws".into(),
            ice_urls: vec!["stun:stun.l.google.com:19302".into()],
            turn_username: String::new(),
            turn_password: String::new(),
            relay_only: false,
            viiper_address: "127.0.0.1:3242".parse().unwrap(),
            display_name: "HoriPlay Guest".into(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteController {
    pub id: u32,
    pub name: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub phase: String,
    pub error: Option<String>,
    pub input_error: Option<String>,
    pub invite: Option<String>,
    pub devices: Vec<PadInfo>,
    pub selected: Option<u32>,
    pub guest_name: Option<String>,
    pub peer_approved: bool,
    pub remote: Option<RemoteController>,
    pub backend: Option<BackendStatus>,
    pub path: PathInfo,
    pub rtt: Percentiles,
    pub sample: Percentiles,
    pub serialization: Percentiles,
    pub input_wait: Percentiles,
    pub sampler_work: Percentiles,
    pub dispatch: Percentiles,
    pub sent: u64,
    pub received: u64,
    pub rejected: u64,
    pub granted_slot: Option<u8>,
    pub settings: InputSettings,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    AcceptPeer,
    RejectPeer,
    Grant { controller: u32, slot: u8 },
    Revoke,
    Stop,
}
enum InputAction {
    Rumble(u32, Rumble),
}
pub struct Engine {
    snapshot: Arc<Mutex<Snapshot>>,
    actions: tokio::sync::Mutex<Option<mpsc::Sender<Action>>>,
    input: std::sync::mpsc::SyncSender<InputAction>,
    feed: Arc<InputFeed>,
    stopped: Arc<AtomicBool>,
    worker: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
    _sdl: Option<SdlRuntime>,
}
impl Engine {
    /// Construct on the application main thread for SDL initialization.
    pub fn new() -> Self {
        Self::new_inner(true)
    }
    fn new_inner(enable_input: bool) -> Self {
        let snapshot = Arc::new(Mutex::new(Snapshot {
            phase: "Idle".into(),
            ..Default::default()
        }));
        let (tx, rx) = std::sync::mpsc::sync_channel(16);
        let feed = Arc::new(InputFeed::default());
        let stopped = Arc::new(AtomicBool::new(false));
        let (state, stop) = (snapshot.clone(), stopped.clone());
        // Engine is constructed in Tauri setup / the CLI's main-thread entry point.
        let runtime = if enable_input {
            match SdlGamepads::default_path().and_then(|path| SdlRuntime::initialize(&path)) {
                Ok(runtime) => Some(runtime),
                Err(e) => {
                    snapshot.lock().unwrap().input_error = Some(format!("{e:#}"));
                    None
                }
            }
        } else {
            None
        };
        let worker = runtime.as_ref().map(|runtime| {
            let factory = runtime.clone();
            let feed = feed.clone();
            std::thread::spawn(move || {
                let result = (|| -> Result<()> {
                    let mut pads = factory.open();
                    let mut timings = TimingWindow::<512>::default();
                    let mut work = TimingWindow::<512>::default();
                    let mut next_ui = Instant::now();
                    let mut next_metrics = Instant::now();
                    let mut next_poll = Instant::now();
                    let mut selected_name: Option<(u32, Arc<str>)> = None;
                    while !stop.load(Ordering::Relaxed) {
                        let start = Instant::now();
                        while let Ok(InputAction::Rumble(id, rumble)) = rx.try_recv() {
                            let _ = pads.rumble(id, rumble.low, rumble.high, rumble.duration_ms);
                        }
                        let devices = pads.poll_cached()?;
                        let captured = Instant::now();
                        timings.record(start.elapsed().as_secs_f64() * 1000.0);
                        let (selected, settings) = {
                            let mut s = state.lock().unwrap();
                            if s.selected
                                .is_some_and(|id| !devices.iter().any(|p| p.id == id))
                            {
                                s.selected = None;
                            }
                            if s.selected.is_none() {
                                s.selected = devices.first().map(|p| p.id);
                            }
                            let topology_changed = s.devices.len() != devices.len()
                                || s.devices.iter().zip(devices).any(|(a, b)| a.id != b.id);
                            if captured >= next_ui || topology_changed {
                                if s.devices.len() == devices.len() {
                                    s.devices.clone_from_slice(devices);
                                } else {
                                    s.devices = devices.to_vec();
                                }
                                next_ui = captured + Duration::from_millis(33);
                            }
                            (s.selected, s.settings.clone())
                        };
                        let sample = selected
                            .and_then(|id| devices.iter().find(|p| p.id == id))
                            .map(|pad| {
                                if selected_name.as_ref().map(|(id, _)| *id) != Some(pad.id) {
                                    selected_name = Some((pad.id, Arc::from(pad.name.as_str())));
                                }
                                Sample {
                                    id: pad.id,
                                    name: selected_name.as_ref().unwrap().1.clone(),
                                    state: settings.apply(pad.state),
                                    captured,
                                }
                            });
                        feed.publish(sample);
                        work.record(start.elapsed().as_secs_f64() * 1000.0);
                        if captured >= next_metrics {
                            // Sorting is infrequent and never holds the shared UI/input lock.
                            let (sample, sampler_work) = (timings.summary(), work.summary());
                            let mut s = state.lock().unwrap();
                            s.sample = sample;
                            s.sampler_work = sampler_work;
                            next_metrics = captured + Duration::from_millis(100);
                        }
                        // No high-rate empty polling or busy spinning. Discovery is ~100 ms
                        // plus the current sampler period and OS scheduling jitter.
                        let period = Duration::from_millis(if devices.is_empty() { 16 } else { 4 });
                        next_poll += period;
                        if next_poll <= Instant::now() {
                            next_poll = Instant::now() + period;
                        }
                        std::thread::sleep(next_poll.saturating_duration_since(Instant::now()));
                    }
                    Ok(())
                })();
                if let Err(e) = result {
                    feed.publish(None);
                    let mut s = state.lock().unwrap();
                    s.devices.clear();
                    s.input_error = Some(format!("{e:#}"));
                }
            })
        });
        Self {
            snapshot,
            actions: tokio::sync::Mutex::new(None),
            input: tx,
            feed,
            stopped,
            worker: std::sync::Mutex::new(worker),
            _sdl: runtime,
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap().clone()
    }
    pub fn configure_input(&self, selected: Option<u32>, settings: InputSettings) -> Result<()> {
        if !settings.valid() {
            bail!("invalid dead zone, curve, sensitivity or mapping");
        }
        let mut s = self.snapshot.lock().unwrap();
        if selected.is_some_and(|id| !s.devices.iter().any(|p| p.id == id)) {
            bail!("controller disconnected");
        }
        s.selected = selected;
        s.settings = settings;
        Ok(())
    }
    pub fn rumble(&self, id: u32) -> Result<()> {
        let s = self.snapshot();
        if !s.devices.iter().any(|p| p.id == id && p.rumble) {
            bail!("controller does not report rumble capability");
        }
        self.input.try_send(InputAction::Rumble(
            id,
            Rumble {
                low: 24000,
                high: 18000,
                duration_ms: 250,
            },
        ))?;
        Ok(())
    }
    pub async fn start(&self, config: Config, code: Option<String>) -> Result<()> {
        validate_config(&config)?;
        let mut actions = self.actions.lock().await;
        if actions.as_ref().is_some_and(|tx| !tx.is_closed()) {
            bail!("a session is already running");
        }
        let host = code.is_none();
        let invite = if let Some(code) = code {
            Invite::parse(&code)?
        } else {
            Invite::generate()
        };
        let (tx, rx) = mpsc::channel(16);
        *actions = Some(tx);
        let (snapshot, input, feed) =
            (self.snapshot.clone(), self.input.clone(), self.feed.clone());
        feed.arm(false);
        {
            let mut s = snapshot.lock().unwrap();
            s.phase = "Connecting signaling".into();
            s.error = None;
            s.backend = None;
            s.remote = None;
            s.guest_name = None;
            s.peer_approved = false;
            s.granted_slot = None;
            s.path = PathInfo::default();
            s.invite = if host { Some(invite.code()) } else { None };
            s.sent = 0;
            s.received = 0;
            s.rejected = 0;
            s.rtt = Default::default();
            s.serialization = Default::default();
            s.dispatch = Default::default();
            s.input_wait = Default::default();
        }
        tokio::spawn(async move {
            let result = run(
                config,
                invite,
                host,
                snapshot.clone(),
                input,
                feed.clone(),
                rx,
            )
            .await;
            feed.arm(false);
            let mut s = snapshot.lock().unwrap();
            s.phase = "Stopped".into();
            s.peer_approved = false;
            s.granted_slot = None;
            s.backend = None;
            s.path.connected = false;
            s.invite = None;
            if let Err(e) = result {
                s.error = Some(format!("{e:#}"));
            }
        });
        Ok(())
    }
    pub async fn action(&self, action: Action) -> Result<()> {
        let actions = self.actions.lock().await;
        let tx = actions.as_ref().context("no active session")?;
        tx.try_send(action).context("session busy or stopped")?;
        Ok(())
    }
}
impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
        if let Ok(actions) = self.actions.try_lock() {
            if let Some(tx) = actions.as_ref() {
                let _ = tx.try_send(Action::Stop);
            }
        }
    }
}
fn validate_config(config: &Config) -> Result<()> {
    let url = url::Url::parse(&config.signaling)?;
    let local = matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    );
    if url.scheme() != "wss" && !(url.scheme() == "ws" && local) {
        bail!("Internet signaling requires wss://; ws:// is allowed only on loopback");
    }
    if config.display_name.is_empty()
        || config.display_name.len() > 64
        || config.display_name.chars().any(char::is_control)
    {
        bail!("display name must contain 1..64 non-control bytes");
    }
    if !config.viiper_address.ip().is_loopback() {
        bail!("VIIPER API must be loopback");
    }
    if config.relay_only
        && !config
            .ice_urls
            .iter()
            .any(|url| url.starts_with("turn:") || url.starts_with("turns:"))
    {
        bail!("relay-only mode requires a TURN server");
    }
    Ok(())
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Control {
    Connect { id: u32, name: String },
    Disconnect { id: u32 },
    Grant { id: u32, slot: u8 },
    Revoke,
    Rumble { id: u32, value: Rumble },
    Ping { nonce: u64 },
    Pong { nonce: u64 },
}
async fn control(peer: &Peer, message: &Control) -> Result<()> {
    if !peer.send(&serde_json::to_vec(message)?, true).await? {
        bail!("reliable controller channel is not open");
    }
    Ok(())
}
async fn run(
    config: Config,
    invite: Invite,
    host: bool,
    snapshot: Arc<Mutex<Snapshot>>,
    input: std::sync::mpsc::SyncSender<InputAction>,
    feed: Arc<InputFeed>,
    mut actions: mpsc::Receiver<Action>,
) -> Result<()> {
    let (mut ws, _) = tokio_tungstenite::connect_async(&config.signaling)
        .await
        .context("signaling connection")?;
    let hello = if host {
        ClientMessage::Create {
            room: invite.room,
            join_hash: invite.join_hash(),
        }
    } else {
        ClientMessage::Join {
            room: invite.room,
            join_hash: invite.join_hash(),
            name: config.display_name.clone(),
        }
    };
    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        serde_json::to_string(&hello)?.into(),
    ))
    .await?;
    let servers = config
        .ice_urls
        .iter()
        .map(|url| RTCIceServer {
            urls: vec![url.clone()],
            username: config.turn_username.clone(),
            credential: config.turn_password.clone(),
        })
        .collect();
    let (peer, mut events, mut ice) = Peer::with_policy(host, servers, config.relay_only).await?;
    let mut auth = SignalAuth::new(invite.clone(), if host { Role::Host } else { Role::Guest });
    let mut receiver = Receiver::new(invite.room, invite.room);
    let mut backend: Option<Viiper> = None;
    let mut watchdog = Watchdog::new(Duration::from_millis(250));
    let mut grant: Option<(u32, u8)> = None;
    let mut tick = tokio::time::interval(Duration::from_millis(if host { 8 } else { 50 }));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut maintenance = tokio::time::interval(Duration::from_secs(1));
    let mut heart = tokio::time::interval(Duration::from_secs(10));
    let mut guest_input = GuestInput::default();
    let start = Instant::now();
    let (mut rtts, mut dispatch) = (
        TimingWindow::<1024>::default(),
        TimingWindow::<1024>::default(),
    );
    let mut outstanding_ping = None;
    let mut approved = false;
    let mut last_rumble = None::<Rumble>;
    let mut last_rumble_at = Instant::now() - Duration::from_secs(1);
    let result=async {
        loop {tokio::select! {
            Some(action)=actions.recv()=>match action {
                Action::Stop=>break,
                Action::AcceptPeer if host&&!snapshot.lock().unwrap().peer_approved=>{snapshot.lock().unwrap().peer_approved=true;ws.send(tokio_tungstenite::tungstenite::Message::Text(serde_json::to_string(&ClientMessage::Approve)?.into())).await?;},
                Action::RejectPeer if host=>{ws.send(tokio_tungstenite::tungstenite::Message::Text(serde_json::to_string(&ClientMessage::Reject)?.into())).await?;if approved{break;}},
                Action::Grant{controller,slot} if host=>{
                    let s=snapshot.lock().unwrap().clone();
                    if !approved||!peer.ready().await||s.remote.as_ref().map(|r|r.id)!=Some(controller){bail!("controller is not connected or peer not authorized");}
                    if backend.is_some(){bail!("revoke the current controller before assigning another slot");}
                    match Viiper::connect(config.viiper_address,slot) {
                        Ok(device)=>{
                            // Feedback and freshness belong to this permission grant only.
                            last_rumble=None;watchdog=Watchdog::new(Duration::from_millis(250));
                            backend=Some(device);receiver.slots.assign(slot,Owner{peer:invite.room,controller}).map_err(anyhow::Error::msg)?;grant=Some((controller,slot));control(&peer,&Control::Grant{id:controller,slot}).await?;let mut s=snapshot.lock().unwrap();s.granted_slot=Some(slot);s.error=None;
                        },
                        Err(e)=>{snapshot.lock().unwrap().error=Some(format!("{e:#}"));}
                    }
                },
                Action::Revoke if host=>{
                    if let Some((id,_))=grant.take(){receiver.revoke(id);}
if let Some(mut device)=backend.take(){device.neutralize()?;}
                    control(&peer,&Control::Revoke).await?;let mut s=snapshot.lock().unwrap();s.granted_slot=None;s.backend=None;s.phase="Controller permission revoked".into();
                },
                _=>{}
            },
            message=ws.next()=>{
                let Some(message)=message else{bail!("signaling disconnected");};
                let message=message?;
                if let tokio_tungstenite::tungstenite::Message::Text(text)=message {
                    if text.len()>160*1024{bail!("oversize signaling");}
                    match serde_json::from_str::<ServerMessage>(&text)? {
                        ServerMessage::Created=>snapshot.lock().unwrap().phase="Room ready · invite valid for 10 minutes".into(),
                        ServerMessage::Waiting=>snapshot.lock().unwrap().phase="Waiting for host approval".into(),
                        ServerMessage::GuestWaiting{name}=>{let mut s=snapshot.lock().unwrap();s.guest_name=Some(name);s.phase="Guest waiting · approval required".into();},
                        ServerMessage::Approved=>{if approved{continue;}approved=true;{let mut s=snapshot.lock().unwrap();s.peer_approved=true;s.phase="Negotiating encrypted P2P".into();}if host{let signal=auth.sign(&peer.offer().await?)?;ws.send(tokio_tungstenite::tungstenite::Message::Text(serde_json::to_string(&ClientMessage::Signal{signal})?.into())).await?;}},
                        ServerMessage::Signal{signal}=>{if !approved{bail!("unsolicited signaling");}
if let Some(answer)=peer.apply(auth.verify(signal)?).await?{let signal=auth.sign(&answer)?;ws.send(tokio_tungstenite::tungstenite::Message::Text(serde_json::to_string(&ClientMessage::Signal{signal})?.into())).await?;}},
                        ServerMessage::PeerLeft=>break,ServerMessage::Error{message}=>bail!("{message}"),ServerMessage::Heartbeat=>{},
                    }
                }else if message.is_close(){break;}
            },
            Some(candidate)=ice.recv()=>{if approved{let signal=auth.sign(&candidate)?;ws.send(tokio_tungstenite::tungstenite::Message::Text(serde_json::to_string(&ClientMessage::Signal{signal})?.into())).await?;}},
            Some(event)=events.recv()=>match event {
                PeerEvent::State(state)=>{snapshot.lock().unwrap().phase=format!("WebRTC {state}");if state=="disconnected"||state=="failed"||state=="closed"{break;}},
                PeerEvent::Input(data) if host=>{
                    let decoded=InputPacket::decode(&data);let accepted=decoded.and_then(|p|receiver.accept(p));
                    match accepted {
                        Ok(state)=>{if let Some(device)=backend.as_mut(){let begin=Instant::now();device.update(state)?;dispatch.record(begin.elapsed().as_secs_f64()*1000.0);watchdog.accepted(Instant::now());snapshot.lock().unwrap().received+=1;}},
                        Err(_)=>{snapshot.lock().unwrap().rejected+=1;}
                    }
                },
                PeerEvent::Control(data)=>{
                    match serde_json::from_slice::<Control>(&data)? {
                        Control::Connect{id,name} if host=>{
                            if name.len()>256{bail!("invalid controller name");}
                            let existing=snapshot.lock().unwrap().remote.as_ref().map(|r|r.id);
                            if existing!=Some(id) {if let Some((old,_))=grant.take(){receiver.revoke(old);}
if let Some(mut device)=backend.take(){device.neutralize()?;}let mut s=snapshot.lock().unwrap();s.granted_slot=None;s.backend=None;}
                            snapshot.lock().unwrap().remote=Some(RemoteController{id,name});
                        },
                        Control::Disconnect{id} if host=>{
                            if grant.is_some_and(|(current,_)|current==id){receiver.revoke(id);grant=None;if let Some(mut device)=backend.take(){device.neutralize()?;}}
                            let mut s=snapshot.lock().unwrap();s.remote=None;s.backend=None;s.granted_slot=None;
                        },
                        Control::Grant{id,slot} if !host&&slot<4=>{
                            if guest_input.announced!=Some(id){bail!("host granted an unknown controller");}grant=Some((id,slot));snapshot.lock().unwrap().granted_slot=Some(slot);guest_input.new_grant();feed.arm(true);
                        },
                        Control::Revoke if !host=>{feed.arm(false);if let Some((id,_))=grant.take(){let _=input.try_send(InputAction::Rumble(id,Rumble{low:0,high:0,duration_ms:0}));}snapshot.lock().unwrap().granted_slot=None;},
                        Control::Rumble{id,value} if !host&&grant.is_some_and(|(current,_)|current==id)=>{
                            let capable=snapshot.lock().unwrap().devices.iter().any(|p|p.id==id&&p.rumble);
                            if capable{let _=input.try_send(InputAction::Rumble(id,Rumble{duration_ms:value.duration_ms.min(250),..value}));}
                        },
                        Control::Ping{nonce}=>{control(&peer,&Control::Pong{nonce}).await?;},
                        Control::Pong{nonce}=>{if let Some((expected,sent))=outstanding_ping{if nonce==expected{rtts.record(Instant::now().duration_since(sent).as_secs_f64()*1000.0);outstanding_ping=None;}}},
                        _=>{}
                    }
                },
                _=>{}
            },
            _=feed.changed.notified(), if !host=>{
                let sent=guest_input.flush(&peer,&feed,invite.room,&mut grant).await?;
                let mut s=snapshot.lock().unwrap();s.sent+=sent;s.granted_slot=grant.map(|(_,slot)|slot);
            },
            _=tick.tick()=>{
                if host {
                    if watchdog.expired(Instant::now()){if let Some(device)=backend.as_mut(){device.neutralize()?;}last_rumble=Some(Rumble{low:0,high:0,duration_ms:0});last_rumble_at=Instant::now()-Duration::from_secs(1);snapshot.lock().unwrap().phase="Input timed out · remote controller neutralized".into();}
                    if let Some(device)=backend.as_mut(){
                        if let Some(rumble)=device.feedback()?{last_rumble=Some(rumble);last_rumble_at=Instant::now()-Duration::from_secs(1);}
                        if let(Some((id,_)),Some(rumble))=(grant,last_rumble){if last_rumble_at.elapsed()>=Duration::from_millis(100){control(&peer,&Control::Rumble{id,value:rumble}).await?;last_rumble_at=Instant::now();if rumble.low==0&&rumble.high==0{last_rumble=None;}}}
                        snapshot.lock().unwrap().backend=Some(device.status());
                    }
                }else{
                    let sent=guest_input.flush(&peer,&feed,invite.room,&mut grant).await?;
                    let mut s=snapshot.lock().unwrap();s.sent+=sent;s.granted_slot=grant.map(|(_,slot)|slot);
                }
            },
            _=maintenance.tick()=>{
                let path=peer.path().await;
                if peer.ready().await{let nonce=start.elapsed().as_micros() as u64;if outstanding_ping.is_none(){outstanding_ping=Some((nonce,Instant::now()));control(&peer,&Control::Ping{nonce}).await?;}}
                let (rtt,serialization,dispatch,input_wait)=(rtts.summary(),guest_input.serialization.summary(),dispatch.summary(),guest_input.wait.summary());
                let mut s=snapshot.lock().unwrap();s.path=path;s.rtt=rtt;s.serialization=serialization;s.dispatch=dispatch;s.input_wait=input_wait;
            },
            _=heart.tick()=>{ws.send(tokio_tungstenite::tungstenite::Message::Text(serde_json::to_string(&ClientMessage::Heartbeat)?.into())).await?;},
        }}Ok::<_,anyhow::Error>(())
    }.await;
    // Cleanup runs on every normal/error exit. OS removal is in backend Drop.
    if let Some(mut device) = backend.take() {
        let _ = device.neutralize();
        drop(device);
    }
    if let Some((id, _)) = grant {
        let _ = input.try_send(InputAction::Rumble(
            id,
            Rumble {
                low: 0,
                high: 0,
                duration_ms: 0,
            },
        ));
    }
    let _ = ws.close(None).await;
    let _ = peer.close().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_cleartext_remote_signaling() {
        let mut c = Config {
            signaling: "ws://192.0.2.1:8787/ws".into(),
            ..Default::default()
        };
        assert!(validate_config(&c).is_err());
        c.signaling = "wss://example.com/ws".into();
        validate_config(&c).unwrap();
    }
    #[tokio::test]
    async fn real_signaling_requires_approval_before_p2p() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, horiplay_signaling::router())
                .await
                .unwrap();
        });
        // Real network test; no native initialization on the Rust test thread.
        let host = Engine::new_inner(false);
        let guest = Engine::new_inner(false);
        let config = Config {
            signaling: format!("ws://{address}/ws"),
            ice_urls: vec![],
            ..Default::default()
        };
        host.start(config.clone(), None).await.unwrap();
        let code = host.snapshot().invite.unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        guest.start(config, Some(code)).await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while host.snapshot().guest_name.is_none() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert!(!guest.snapshot().path.connected);
        assert!(host.snapshot().backend.is_none());
        host.action(Action::AcceptPeer).await.unwrap();
        tokio::time::timeout(Duration::from_secs(20), async {
            while !guest.snapshot().path.connected {
                assert!(
                    guest.snapshot().error.is_none(),
                    "{:?}",
                    guest.snapshot().error
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
        assert!(host.snapshot().backend.is_none());
        host.action(Action::Stop).await.unwrap();
        guest.action(Action::Stop).await.unwrap();
        server.abort();
    }
}
