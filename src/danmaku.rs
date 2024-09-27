use std::fmt::Display;

use futures::StreamExt;
use futures_util::SinkExt;
use poem::web::websocket::{Message, WebSocket};
use poem::web::{Data, RemoteAddr};
use poem::{handler, IntoResponse};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Danmaku {
    pub text: String,
    pub color: Option<String>,
    pub size: Option<i32>,
    pub sender: Option<String>,
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
    Data(channel): Data<&broadcast::Sender<Danmaku>>,
) -> impl IntoResponse {
    let peer = peer.clone();
    tracing::info!("connection from {}", peer);
    let channel = channel.clone();
    let mut recv = channel.subscribe();
    ws.on_upgrade(|mut socket| async move {
        loop {
            tokio::select! {
                Ok(danmaku) = recv.recv() => {
                    if let Ok(danmaku) = serde_json::to_string(&danmaku) {
                        let _ = socket.send(Message::Text(danmaku)).await;
                    }
                }
                Some(Ok(msg)) = socket.next() => {
                    tracing::debug!("got message: {:?}", msg);
                    match msg {
                        Message::Text(msg) => {
                            if let Ok(danmaku) = serde_json::from_str::<Danmaku>(&msg) {
                                channel.send(danmaku).expect("failed to send message");
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
