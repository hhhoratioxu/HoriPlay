use anyhow::{bail, Context, Result};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;
use webrtc::{
    api::APIBuilder,
    data_channel::{
        data_channel_init::RTCDataChannelInit, data_channel_message::DataChannelMessage,
        data_channel_state::RTCDataChannelState, RTCDataChannel,
    },
    ice_transport::{ice_candidate::RTCIceCandidateInit, ice_server::RTCIceServer},
    peer_connection::{
        configuration::RTCConfiguration, sdp::session_description::RTCSessionDescription,
        RTCPeerConnection,
    },
};

#[derive(Clone)]
pub struct Invite {
    pub room: Uuid,
    secret: [u8; 32],
}
impl Invite {
    pub fn generate() -> Self {
        let mut secret = [0; 32];
        secret[..16].copy_from_slice(Uuid::new_v4().as_bytes());
        secret[16..].copy_from_slice(Uuid::new_v4().as_bytes());
        Self {
            room: Uuid::new_v4(),
            secret,
        }
    }
    pub fn code(&self) -> String {
        format!("{}.{}", self.room.simple(), hex::encode(self.secret))
    }
    pub fn parse(code: &str) -> Result<Self> {
        let (room, key) = code
            .trim()
            .split_once('.')
            .context("invite must be room.secret")?;
        let key = hex::decode(key)?;
        if key.len() != 32 {
            bail!("invalid invite key");
        }
        let mut secret = [0; 32];
        secret.copy_from_slice(&key);
        Ok(Self {
            room: Uuid::parse_str(room)?,
            secret,
        })
    }
    pub fn join_hash(&self) -> String {
        hex::encode(Sha256::digest(self.secret))
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Host,
    Guest,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum Signal {
    Description(Box<RTCSessionDescription>),
    Candidate(RTCIceCandidateInit),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedSignal {
    pub role: Role,
    pub sequence: u64,
    pub payload: String,
    pub mac: String,
}
pub struct SignalAuth {
    invite: Invite,
    role: Role,
    outgoing: u64,
    incoming: u64,
}
impl SignalAuth {
    pub fn new(invite: Invite, role: Role) -> Self {
        Self {
            invite,
            role,
            outgoing: 0,
            incoming: 0,
        }
    }
    fn mac(&self, role: Role, sequence: u64, payload: &str) -> Hmac<Sha256> {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.invite.secret).expect("fixed key");
        mac.update(b"HoriPlay signaling v1\0");
        mac.update(self.invite.room.as_bytes());
        mac.update(&[match role {
            Role::Host => 0,
            Role::Guest => 1,
        }]);
        mac.update(&sequence.to_le_bytes());
        mac.update(payload.as_bytes());
        mac
    }
    pub fn sign(&mut self, signal: &Signal) -> Result<SignedSignal> {
        self.outgoing += 1;
        let payload = serde_json::to_string(signal)?;
        Ok(SignedSignal {
            role: self.role,
            sequence: self.outgoing,
            mac: hex::encode(
                self.mac(self.role, self.outgoing, &payload)
                    .finalize()
                    .into_bytes(),
            ),
            payload,
        })
    }
    pub fn verify(&mut self, signed: SignedSignal) -> Result<Signal> {
        if signed.role == self.role
            || signed.sequence <= self.incoming
            || signed.payload.len() > 131072
        {
            bail!("replayed or invalid signaling");
        }
        self.mac(signed.role, signed.sequence, &signed.payload)
            .verify_slice(&hex::decode(signed.mac)?)
            .context("signaling authentication failed")?;
        let signal = serde_json::from_str(&signed.payload)?;
        self.incoming = signed.sequence;
        Ok(signal)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Create {
        room: Uuid,
        join_hash: String,
    },
    Join {
        room: Uuid,
        join_hash: String,
        name: String,
    },
    Approve,
    Reject,
    Signal {
        signal: SignedSignal,
    },
    Heartbeat,
    Leave,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Created,
    Waiting,
    GuestWaiting { name: String },
    Approved,
    Signal { signal: SignedSignal },
    PeerLeft,
    Error { message: String },
    Heartbeat,
}

#[derive(Debug)]
pub enum PeerEvent {
    Input(Vec<u8>),
    Control(Vec<u8>),
    State(String),
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PathInfo {
    pub connected: bool,
    pub local_candidate: String,
    pub remote_candidate: String,
    pub connection_type: String,
}

pub struct Peer {
    pub pc: Arc<RTCPeerConnection>,
    state: Arc<RwLock<Option<Arc<RTCDataChannel>>>>,
    control: Arc<RwLock<Option<Arc<RTCDataChannel>>>>,
    pending_candidates: tokio::sync::Mutex<Vec<RTCIceCandidateInit>>,
}
#[derive(Debug, Serialize)]
pub struct LoopbackEvidence {
    pub transport: String,
    pub path: PathInfo,
    pub attempted: usize,
    pub completed: usize,
    pub timed_out: usize,
    pub rtt: horiplay_core::Percentiles,
    pub scope: String,
}
/// Real DTLS/SCTP echo test, with synthetic protocol frames. Not a physical-controller benchmark.
pub async fn loopback_benchmark(samples: usize) -> Result<LoopbackEvidence> {
    use horiplay_input_transport::InputPacket;
    use std::time::{Duration, Instant};
    let samples = samples.clamp(1, 5000);
    let (host, mut host_events, mut host_ice) = Peer::new(true, vec![]).await?;
    let (guest, mut guest_events, mut guest_ice) = Peer::new(false, vec![]).await?;
    let result=async {
        let offer=host.offer().await?;let answer=guest.apply(offer).await?.context("missing answer")?;host.apply(answer).await?;
        tokio::time::timeout(Duration::from_secs(20),async{
            while !host.ready().await||!guest.ready().await{tokio::select!{Some(c)=host_ice.recv()=>{guest.apply(c).await?;},Some(c)=guest_ice.recv()=>{host.apply(c).await?;},_=tokio::time::sleep(Duration::from_millis(10))=>{}}}Ok::<_,anyhow::Error>(())
        }).await??;
        // Let the channel-open handshake settle; unreliable state is periodically repeated in production.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut times=Vec::new();let session=Uuid::new_v4();
        for sequence in 1..=samples{
            let packet=InputPacket{session,controller:1,slot:1,sequence:sequence as u64,timestamp_us:0,state:horiplay_core::PadState{buttons:1,axes:[6553,-16384,32767,-32768],triggers:[8192,32767]}}.encode();
            let begin=Instant::now();
            if !guest.send(&packet,false).await?{continue;}
            let echo=tokio::time::timeout(Duration::from_millis(150),async{
                loop{if let Some(PeerEvent::Input(bytes))=host_events.recv().await{if bytes==packet{host.send(&bytes,false).await?;break;}}}
                loop{if let Some(PeerEvent::Input(bytes))=guest_events.recv().await{if bytes==packet{break;}}}Ok::<_,anyhow::Error>(())
            }).await;
            if let Ok(value)=echo{value?;times.push(begin.elapsed().as_secs_f64()*1000.0);}
            tokio::time::sleep(Duration::from_millis(4)).await;
        }
        Ok::<_,anyhow::Error>(LoopbackEvidence{transport:"WebRTC ICE + DTLS + SCTP unordered / maxRetransmits=0".into(),path:host.path().await,attempted:samples,completed:times.len(),timed_out:samples-times.len(),rtt:horiplay_core::percentiles(&times),scope:"Same-machine synthetic 56-byte input frame round trip. Excludes SDL physical sampling, virtual driver, game, WAN and video; timeout count is not measured Internet packet loss.".into()})
    }.await;
    let _ = host.close().await;
    let _ = guest.close().await;
    result
}
impl Peer {
    pub async fn new(
        host: bool,
        servers: Vec<RTCIceServer>,
    ) -> Result<(Self, mpsc::Receiver<PeerEvent>, mpsc::Receiver<Signal>)> {
        Self::with_policy(host, servers, false).await
    }
    pub async fn with_policy(
        host: bool,
        servers: Vec<RTCIceServer>,
        relay_only: bool,
    ) -> Result<(Self, mpsc::Receiver<PeerEvent>, mpsc::Receiver<Signal>)> {
        use webrtc::peer_connection::policy::ice_transport_policy::RTCIceTransportPolicy;
        let pc = Arc::new(
            APIBuilder::new()
                .build()
                .new_peer_connection(RTCConfiguration {
                    ice_servers: servers,
                    ice_transport_policy: if relay_only {
                        RTCIceTransportPolicy::Relay
                    } else {
                        RTCIceTransportPolicy::All
                    },
                    ..Default::default()
                })
                .await?,
        );
        let (events, event_rx) = mpsc::channel(128);
        let (signals, signal_rx) = mpsc::channel(128);
        pc.on_ice_candidate(Box::new(move |candidate| {
            let signals = signals.clone();
            Box::pin(async move {
                if let Some(candidate) = candidate {
                    if let Ok(candidate) = candidate.to_json() {
                        let _ = signals.try_send(Signal::Candidate(candidate));
                    }
                }
            })
        }));
        let events2 = events.clone();
        pc.on_peer_connection_state_change(Box::new(move |state| {
            let events = events2.clone();
            Box::pin(async move {
                let _ = events.try_send(PeerEvent::State(state.to_string()));
            })
        }));
        let state = Arc::new(RwLock::new(None));
        let control = Arc::new(RwLock::new(None));
        if host {
            let fast = pc
                .create_data_channel(
                    "hp-state-v1",
                    Some(RTCDataChannelInit {
                        ordered: Some(false),
                        max_retransmits: Some(0),
                        ..Default::default()
                    }),
                )
                .await?;
            attach(fast.clone(), events.clone(), false);
            *state.write().await = Some(fast);
            let reliable = pc.create_data_channel("hp-control-v1", None).await?;
            attach(reliable.clone(), events.clone(), true);
            *control.write().await = Some(reliable);
        } else {
            let (s, c) = (state.clone(), control.clone());
            pc.on_data_channel(Box::new(move |channel| {
                let (s, c, events) = (s.clone(), c.clone(), events.clone());
                Box::pin(async move {
                    match channel.label() {
                        "hp-state-v1" => {
                            let mut s = s.write().await;
                            if s.is_some() {
                                let _ = channel.close().await;
                                return;
                            }
                            attach(channel.clone(), events, false);
                            *s = Some(channel);
                        }
                        "hp-control-v1" => {
                            let mut c = c.write().await;
                            if c.is_some() {
                                let _ = channel.close().await;
                                return;
                            }
                            attach(channel.clone(), events, true);
                            *c = Some(channel);
                        }
                        _ => {
                            let _ = channel.close().await;
                        }
                    }
                })
            }));
        }
        Ok((
            Self {
                pc,
                state,
                control,
                pending_candidates: tokio::sync::Mutex::new(Vec::new()),
            },
            event_rx,
            signal_rx,
        ))
    }
    pub async fn offer(&self) -> Result<Signal> {
        let offer = self.pc.create_offer(None).await?;
        self.pc.set_local_description(offer.clone()).await?;
        Ok(Signal::Description(Box::new(offer)))
    }
    pub async fn apply(&self, signal: Signal) -> Result<Option<Signal>> {
        match signal {
            Signal::Description(description) => {
                let offer = description.sdp_type
                    == webrtc::peer_connection::sdp::sdp_type::RTCSdpType::Offer;
                self.pc.set_remote_description(*description).await?;
                for candidate in self.pending_candidates.lock().await.drain(..) {
                    self.pc.add_ice_candidate(candidate).await?;
                }
                if offer {
                    let answer = self.pc.create_answer(None).await?;
                    self.pc.set_local_description(answer.clone()).await?;
                    Ok(Some(Signal::Description(Box::new(answer))))
                } else {
                    Ok(None)
                }
            }
            Signal::Candidate(candidate) => {
                if self.pc.remote_description().await.is_none() {
                    let mut pending = self.pending_candidates.lock().await;
                    if pending.len() >= 128 {
                        bail!("too many ICE candidates");
                    }
                    pending.push(candidate);
                } else {
                    self.pc.add_ice_candidate(candidate).await?;
                }
                Ok(None)
            }
        }
    }
    pub async fn ready(&self) -> bool {
        let s = self.state.read().await;
        let c = self.control.read().await;
        s.as_ref()
            .is_some_and(|s| s.ready_state() == RTCDataChannelState::Open)
            && c.as_ref()
                .is_some_and(|c| c.ready_state() == RTCDataChannelState::Open)
    }
    pub async fn send(&self, bytes: &[u8], reliable: bool) -> Result<bool> {
        let holder = if reliable { &self.control } else { &self.state };
        let holder = holder.read().await;
        let channel = holder.as_ref().context("data channel not negotiated")?;
        if channel.ready_state() != RTCDataChannelState::Open {
            return Ok(false);
        }
        // SCTP counts DCEP handshake and unacknowledged bytes as buffered too.
        // A zero-byte threshold can permanently suppress the first snapshot.
        // Bound protocol in-flight data to four input frames; never queue at application level.
        if channel.buffered_amount().await.saturating_add(bytes.len())
            > if reliable { 8192 } else { 224 }
        {
            if reliable {
                bail!("reliable control congested");
            }
            return Ok(false);
        }
        channel.send(&bytes::Bytes::copy_from_slice(bytes)).await?;
        Ok(true)
    }
    pub async fn path(&self) -> PathInfo {
        if let Some(sctp) = self
            .pc
            .sctp()
            .transport()
            .ice_transport()
            .get_selected_candidate_pair()
            .await
        {
            let pair = sctp.to_string();
            let (local, remote) = pair
                .trim_start_matches("(local) ")
                .split_once(" <-> (remote) ")
                .unwrap_or((&pair, ""));
            let (local, remote) = (local.to_owned(), remote.to_owned());
            let kind = if local.contains("relay") || remote.contains("relay") {
                "TURN Relay"
            } else {
                "P2P Direct (inspect candidate addresses for LAN/IPv4/IPv6)"
            };
            PathInfo {
                connected: true,
                local_candidate: local,
                remote_candidate: remote,
                connection_type: kind.into(),
            }
        } else {
            PathInfo::default()
        }
    }
    pub async fn close(&self) -> Result<()> {
        self.pc.close().await?;
        Ok(())
    }
}
fn attach(channel: Arc<RTCDataChannel>, events: mpsc::Sender<PeerEvent>, reliable: bool) {
    let weak = Arc::downgrade(&channel);
    channel.on_message(Box::new(move |message: DataChannelMessage| {
        let events = events.clone();
        let weak = weak.clone();
        Box::pin(async move {
            if message.data.len() > 8192 {
                if let Some(ch) = weak.upgrade() {
                    let _ = ch.close().await;
                }
                return;
            }
            let data = message.data.to_vec();
            let event = if !reliable || data.starts_with(b"HP\x01") {
                PeerEvent::Input(data)
            } else {
                PeerEvent::Control(data)
            };
            if events.try_send(event).is_err() && reliable {
                if let Some(ch) = weak.upgrade() {
                    let _ = ch.close().await;
                }
            }
        })
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn invite_and_signed_fingerprints() {
        let invite = Invite::generate();
        assert_eq!(
            Invite::parse(&invite.code()).unwrap().join_hash(),
            invite.join_hash()
        );
        let mut host = SignalAuth::new(invite.clone(), Role::Host);
        let mut guest = SignalAuth::new(invite, Role::Guest);
        let signal = Signal::Candidate(RTCIceCandidateInit {
            candidate: "candidate:test".into(),
            ..Default::default()
        });
        let signed = host.sign(&signal).unwrap();
        let mut tampered = signed.clone();
        tampered.payload.push(' ');
        assert!(guest.verify(tampered).is_err());
        guest.verify(signed.clone()).unwrap();
        assert!(guest.verify(signed).is_err());
    }
    struct TestAuth;
    impl turn::auth::AuthHandler for TestAuth {
        fn auth_handle(
            &self,
            user: &str,
            realm: &str,
            _src: std::net::SocketAddr,
        ) -> std::result::Result<Vec<u8>, turn::Error> {
            if user != "horiplay-test" {
                return Err(turn::Error::ErrFakeErr);
            }
            Ok(turn::auth::generate_auth_key(
                user,
                realm,
                "local-test-password",
            ))
        }
    }
    #[tokio::test]
    async fn forced_turn_relay_selected_and_transmits() {
        use turn::{
            relay::relay_static::RelayAddressGeneratorStatic,
            server::{
                config::{ConnConfig, ServerConfig},
                Server,
            },
        };
        let socket = Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let address = socket.local_addr().unwrap();
        let server = Server::new(ServerConfig {
            conn_configs: vec![ConnConfig {
                conn: socket,
                relay_addr_generator: Box::new(RelayAddressGeneratorStatic {
                    relay_address: "127.0.0.1".parse().unwrap(),
                    address: "127.0.0.1".into(),
                    net: Arc::new(util::vnet::net::Net::new(None)),
                }),
            }],
            realm: "horiplay-test".into(),
            auth_handler: Arc::new(TestAuth),
            channel_bind_timeout: Duration::from_secs(600),
            alloc_close_notify: None,
        })
        .await
        .unwrap();
        let ice = RTCIceServer {
            urls: vec![format!("turn:{address}?transport=udp")],
            username: "horiplay-test".into(),
            credential: "local-test-password".into(),
        };
        let (a, mut ar, mut ai) = Peer::with_policy(true, vec![ice.clone()], true)
            .await
            .unwrap();
        let (b, _, mut bi) = Peer::with_policy(false, vec![ice], true).await.unwrap();
        let result=tokio::time::timeout(Duration::from_secs(20),async{
            let answer=b.apply(a.offer().await.unwrap()).await.unwrap().unwrap();a.apply(answer).await.unwrap();
            while !a.ready().await||!b.ready().await{tokio::select!{Some(c)=ai.recv()=>{b.apply(c).await.unwrap();},Some(c)=bi.recv()=>{a.apply(c).await.unwrap();},_=tokio::time::sleep(Duration::from_millis(20))=>{}}}
            assert_eq!(a.path().await.connection_type,"TURN Relay");b.send(b"turn-confirmed",true).await.unwrap();
            loop{if let Some(PeerEvent::Control(v))=ar.recv().await{assert_eq!(v,b"turn-confirmed");break;}}
        }).await;
        let _ = a.close().await;
        let _ = b.close().await;
        server.close().await.unwrap();
        result.unwrap();
    }
    #[tokio::test]
    async fn actual_dtls_sctp_dual_channel_loopback() {
        let (a, mut a_rx, mut a_ice) = Peer::new(true, vec![]).await.unwrap();
        let (b, mut b_rx, mut b_ice) = Peer::new(false, vec![]).await.unwrap();
        let offer = a.offer().await.unwrap();
        let answer = b.apply(offer).await.unwrap().unwrap();
        a.apply(answer).await.unwrap();
        let result=tokio::time::timeout(Duration::from_secs(20),async {
            while !a.ready().await||!b.ready().await {tokio::select!{Some(c)=a_ice.recv()=>{println!("host ICE {c:?}");b.apply(c).await.unwrap();},Some(c)=b_ice.recv()=>{println!("guest ICE {c:?}");a.apply(c).await.unwrap();},Some(e)=a_rx.recv()=>println!("host event {e:?}"),Some(e)=b_rx.recv()=>println!("guest event {e:?}"),_=tokio::time::sleep(Duration::from_millis(20))=>{}}}
            println!("Both data channels ready; selected path: {:?}",a.path().await);
            b.send(b"reliable",true).await.unwrap();
            let(mut fast,mut reliable)=(false,false);while !fast||!reliable{tokio::select!{Some(event)=a_rx.recv()=>{println!("received {event:?}");match event{PeerEvent::Input(v)=>{assert_eq!(v,b"HP\x01fast");fast=true;},PeerEvent::Control(v)=>{assert_eq!(v,b"reliable");reliable=true;},_=>{}}},_=tokio::time::sleep(Duration::from_millis(30))=>{println!("send snapshot: {}",b.send(b"HP\x01fast",false).await.unwrap());}}}
            a.send(b"return",true).await.unwrap();loop{if let PeerEvent::Control(v)=b_rx.recv().await.unwrap(){assert_eq!(v,b"return");break;}}
            assert!(a.path().await.connected);
        }).await;
        a.close().await.unwrap();
        b.close().await.unwrap();
        result.unwrap();
    }
}
