use std::fmt::Display;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use futures_util::SinkExt;
use poem::web::websocket::{Message, WebSocket};
use poem::web::{Data, Path, RemoteAddr};
use poem::{handler, IntoResponse};
use ring_channel::RingSender;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;

use crate::config::Config;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Danmaku {
    pub text: Arc<str>,
    pub color: Option<Arc<str>>,
    pub size: Option<f64>,
    pub sender: Option<Arc<str>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DanmakuPacket {
    pub group: SmolStr,
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
#[tracing::instrument(skip(ws))]
pub async fn client(
    ws: WebSocket,
    RemoteAddr(peer): &RemoteAddr,
    Path(group): Path<SmolStr>,
    Data(source): Data<&Arc<broadcast::Receiver<DanmakuPacket>>>,
) -> impl IntoResponse {
    let peer = peer.clone();
    tracing::info!("connection from {} to group {}", peer, &group);

    let mut source = source.resubscribe();
    ws.on_upgrade(move |mut socket| async move {
        let mut ping = tokio::time::interval(Duration::from_secs(30));
        loop {
            tokio::select! {
                // From upstream
                packet = source.recv() => {
                    match packet {
                        Ok(packet) => {
                            if packet.group != group { continue; }
                            if let Ok(danmaku) = serde_json::to_string(&packet.danmaku) {
                                let _ = socket.send(Message::Text(danmaku)).await;
                                tracing::debug!("{} -> {}", packet.group, packet.danmaku);
                            }
                        }
                        Err(RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }

                // From client
                Some(Ok(msg)) = socket.next() => {
                    tracing::debug!("got message: {:?}", msg);
                    match msg {
                        Message::Ping(payload) => {
                            let _ = socket.send(Message::Pong(payload)).await;
                            tracing::debug!("pong");
                        }
                        Message::Close(close) => {
                            tracing::info!("connection from {} closed: {:?}", peer, close);
                            break;
                        }
                        _ => {}
                    }
                }

                // Ping
                _ = ping.tick() => {
                    let _ = socket.send(Message::Ping(vec![])).await;
                    tracing::debug!("ping");
                }

                // On error
                else => { break }
            }
        }
        if let Err(e) = socket.close().await {
            tracing::error!("failed to close connection: {}", e);
        }
    })
}

#[handler]
#[tracing::instrument(skip(ws, sink))]
pub async fn upstream(
    ws: WebSocket,
    RemoteAddr(peer): &RemoteAddr,
    Data(sink): Data<&RingSender<DanmakuPacket>>,
) -> impl IntoResponse {
    let peer = peer.clone();
    tracing::info!("connection from {}", peer);

    let sink = sink.clone();

    let config = Config::load();
    ws.on_upgrade(move |mut socket| async move {
        let mut ping = tokio::time::interval(Duration::from_secs(30));
        loop {
            tokio::select! {
                // From client
                Some(Ok(msg)) = socket.next() => {
                    tracing::debug!("got message: {:?}", msg);
                    match msg {
                        Message::Text(msg) => {
                            if let Ok(packet) = serde_json::from_str::<DanmakuPacket>(&msg) {
                                if packet.danmaku.text.chars().count() > config.max_length { continue; }
                                sink.send(packet).expect("all middleware tasks are gone");
                            }
                        }
                        Message::Ping(payload) => {
                            let _ = socket.send(Message::Pong(payload)).await;
                            tracing::debug!("pong");
                        }
                        Message::Close(close) => {
                            tracing::info!("connection from {} closed: {:?}", peer, close);
                            break;
                        }
                        _ => {}
                    }
                }

                // Ping
                _ = ping.tick() => {
                    let _ = socket.send(Message::Ping(vec![])).await;
                    tracing::debug!("ping");
                }

                // On error
                else => { break }
            }
        }
        if let Err(e) = socket.close().await {
            tracing::error!("failed to close connection: {}", e);
        }
    })
}
