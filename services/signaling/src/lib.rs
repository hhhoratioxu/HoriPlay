use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use horiplay_networking::{ClientMessage, ServerMessage};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, Mutex, Semaphore};
use uuid::Uuid;
type Sender = mpsc::Sender<ServerMessage>;
struct Room {
    host_id: Uuid,
    host: Sender,
    guest: Option<(Uuid, Sender)>,
    hash: String,
    created: Instant,
    approved: bool,
}
#[derive(Clone)]
struct Rooms {
    items: Arc<Mutex<HashMap<Uuid, Room>>>,
    connections: Arc<Semaphore>,
}
pub fn router() -> Router {
    Router::new()
        .route("/health", get(|| async { "HoriPlay signaling alpha" }))
        .route("/ws", get(upgrade))
        .with_state(Rooms {
            items: Arc::new(Mutex::new(HashMap::new())),
            connections: Arc::new(Semaphore::new(256)),
        })
}
async fn upgrade(State(rooms): State<Rooms>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.max_message_size(160 * 1024)
        .max_frame_size(160 * 1024)
        .on_upgrade(move |socket| connection(socket, rooms))
}
async fn connection(socket: WebSocket, rooms: Rooms) {
    let Ok(_permit) = rooms.connections.clone().try_acquire_owned() else {
        return;
    };
    let (mut writer, mut reader) = socket.split();
    let (tx, mut rx) = mpsc::channel::<ServerMessage>(128);
    let id = Uuid::new_v4();
    let mut membership: Option<(Uuid, bool)> = None;
    let mut last = Instant::now();
    let mut budget = 150u32;
    let mut refill = Instant::now();
    let mut timer = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            _=timer.tick()=>{if last.elapsed()>Duration::from_secs(25){break;}},
            Some(message)=rx.recv()=>{let Ok(json)=serde_json::to_string(&message)else{break;};if writer.send(Message::Text(json.into())).await.is_err(){break;}},
            message=reader.next()=>{
                let Some(Ok(Message::Text(text)))=message else{break;};
                if refill.elapsed()>Duration::from_secs(10){budget=150;refill=Instant::now();}
                if budget==0{break;}budget-=1;last=Instant::now();
                let Ok(message)=serde_json::from_str::<ClientMessage>(&text)else{break;};
                let mut all=rooms.items.lock().await;
                let result:Result<(),&str>=(||{
                    match message {
                        ClientMessage::Create{room,join_hash}=>{
                            if membership.is_some()||all.len()>=128||all.contains_key(&room){return Err("room unavailable");}
                            if join_hash.len()!=64||!join_hash.bytes().all(|c|c.is_ascii_hexdigit()){return Err("invalid join proof");}
                            all.insert(room,Room{host_id:id,host:tx.clone(),guest:None,hash:join_hash,created:Instant::now(),approved:false});membership=Some((room,true));send(&tx,ServerMessage::Created)?;
                        },
                        ClientMessage::Join{room,join_hash,name}=>{
                            if membership.is_some()||name.is_empty()||name.len()>64||name.chars().any(char::is_control){return Err("invalid join");}
                            let room_data=all.get_mut(&room).ok_or("room unavailable")?;
                            if room_data.created.elapsed()>Duration::from_secs(600)||room_data.hash!=join_hash||room_data.guest.is_some(){return Err("room unavailable or invite expired");}
                            send(&room_data.host,ServerMessage::GuestWaiting{name})?;room_data.guest=Some((id,tx.clone()));membership=Some((room,false));send(&tx,ServerMessage::Waiting)?;
                        },
                        ClientMessage::Approve=>{
                            let (room,true)=membership.ok_or("not in room")? else{return Err("host only");};
                            let r=all.get_mut(&room).ok_or("room closed")?;let (_,guest)=r.guest.as_ref().ok_or("no guest waiting")?;
                            send(guest,ServerMessage::Approved)?;send(&r.host,ServerMessage::Approved)?;r.approved=true;
                        },
                        ClientMessage::Reject=>{
                            let (room,true)=membership.ok_or("not in room")? else{return Err("host only");};
                            let r=all.get_mut(&room).ok_or("room closed")?;if let Some((_,guest))=r.guest.take(){let _=send(&guest,ServerMessage::PeerLeft);}r.approved=false;
                        },
                        ClientMessage::Signal{signal}=>{
                            let (room,host)=membership.ok_or("not in room")?;let r=all.get(&room).ok_or("room closed")?;
                            if !r.approved{return Err("host authorization required");}
                            let role=if host{horiplay_networking::Role::Host}else{horiplay_networking::Role::Guest};
                            if signal.role!=role {return Err("signal role mismatch");}
                            if host {let (_,guest)=r.guest.as_ref().ok_or("guest left")?;send(guest,ServerMessage::Signal{signal})?;}else{
                                if r.guest.as_ref().map(|(guest,_)|*guest)!=Some(id){return Err("guest removed");}
                                send(&r.host,ServerMessage::Signal{signal})?;
                            }
                        },
                        ClientMessage::Heartbeat=>{send(&tx,ServerMessage::Heartbeat)?;},
                        ClientMessage::Leave=>return Err("session closed"),
                    }Ok(())
                })();
                if let Err(message)=result{let _=tx.try_send(ServerMessage::Error{message:message.into()});drop(all);let _=writer.send(Message::Text(serde_json::to_string(&ServerMessage::Error{message:message.into()}).unwrap().into())).await;break;}
            }
        }
    }
    if let Some((room, host)) = membership {
        let mut all = rooms.items.lock().await;
        if host {
            if all.get(&room).is_some_and(|r| r.host_id == id) {
                if let Some(r) = all.remove(&room) {
                    if let Some((_, guest)) = r.guest {
                        let _ = send(&guest, ServerMessage::PeerLeft);
                    }
                }
            }
        } else if let Some(r) = all.get_mut(&room) {
            if r.guest.as_ref().is_some_and(|(guest, _)| *guest == id) {
                r.guest = None;
                r.approved = false;
                let _ = send(&r.host, ServerMessage::PeerLeft);
            }
        }
    }
}
fn send(tx: &Sender, message: ServerMessage) -> Result<(), &'static str> {
    tx.try_send(message)
        .map_err(|_| "peer is too slow or disconnected")
}
