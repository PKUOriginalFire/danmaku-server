use std::fmt::Display;
use std::sync::Arc;

use futures::StreamExt;
use futures_util::SinkExt;
use poem::web::websocket::{Message, WebSocket};
use poem::web::{Data, Path, RemoteAddr};
use poem::{handler, IntoResponse};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Danmaku {
    pub text: Arc<str>,
    pub color: Option<Arc<str>>,
    pub size: Option<f64>,
    pub sender: Option<Arc<str>>,
}

#[derive(Clone, Debug)]
pub struct DanmakuPacket {
    pub group: i64,
    pub danmaku: Danmaku,
}

impl Display for Danmaku {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(sender) = &self.sender {
            write!(f, "{}: ", sender)?;
        }
        write!(f, "{}", self.text)?;
        if self.color.is_some() || self.size.is_some() {
            write!(f, " [")?;
            if let Some(color) = &self.color {
                write!(f, "color={};", color)?;
            }
            if let Some(size) = self.size {
                write!(f, "size={};", size)?;
            }
            write!(f, "]")?;
        }
        Ok(())
    }
}

#[handler]
#[tracing::instrument(skip_all)]
pub async fn endpoint(
    ws: WebSocket,
    peer: &RemoteAddr,
    Path(group): Path<i64>,
    Data(channel): Data<&broadcast::Sender<DanmakuPacket>>,
) -> impl IntoResponse {
    let peer = peer.clone();
    tracing::info!("connection from {} to group {}", peer, group);
    let channel = channel.clone();
    let mut recv = channel.subscribe();
    ws.on_upgrade(move |mut socket| async move {
        loop {
            tokio::select! {
                Ok(packet) = recv.recv() => {
                    if packet.group != group { continue; }
                    if let Ok(danmaku) = serde_json::to_string(&packet.danmaku) {
                        let _ = socket.send(Message::Text(danmaku)).await;
                    }
                }
                Some(Ok(msg)) = socket.next() => {
                    tracing::debug!("got message: {:?}", msg);
                    match msg {
                        Message::Text(msg) => {
                            if let Ok(danmaku) = serde_json::from_str::<Danmaku>(&msg) {
                                let packet = DanmakuPacket { group, danmaku };
                                channel.send(packet).expect("failed to send message");
                            }
                        },
                        Message::Ping(payload) => {
                            let _ = socket.send(Message::Pong(payload)).await;
                        },
                        Message::Close(close) => {
                            tracing::info!("connection from {} closed: {:?}", peer, close);
                            return;
                        },
                        _ => {}
                    }
                }
                else => { return }
            }
        }
    })
}
