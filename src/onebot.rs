use std::borrow::Cow;

use eyre::Result;
use futures_util::StreamExt;
use poem::web::websocket::{Message as WebSocketMessage, WebSocket};
use poem::web::{Data, RemoteAddr};
use poem::{handler, IntoResponse};
use ring_channel::RingSender;
use serde::Deserialize;
use smol_str::ToSmolStr;

use crate::config::Config;
use crate::danmaku::{Danmaku, DanmakuPacket};
use crate::onebot::cqcode::cq_to_text;

mod cqcode;

#[derive(Deserialize, Debug, Default)]
#[serde(default)]
#[serde(rename_all = "snake_case")]
pub struct MessageEvent<'a> {
    pub post_type: &'a str,
    pub time: i64,
    pub self_id: i64,
    pub group_id: Option<i64>,
    pub sender: Option<Sender<'a>>,
    pub message: Option<Message<'a>>,
}

#[derive(Deserialize, Debug, Default)]
#[serde(default)]
#[serde(rename_all = "snake_case")]
pub struct Sender<'a> {
    pub user_id: i64,
    pub nickname: Option<&'a str>,
    pub card: Option<&'a str>,
}

impl<'a> Sender<'a> {
    pub fn name(&self) -> Cow<'a, str> {
        if let Some(card) = self.card.filter(|s| !s.is_empty()) {
            card.into()
        } else if let Some(nickname) = self.nickname.filter(|s| !s.is_empty()) {
            nickname.into()
        } else {
            self.user_id.to_string().into()
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
#[serde(rename_all = "snake_case")]
pub enum Message<'a> {
    Text(&'a str),
    Segments(Vec<MessageSegment<'a>>),
}

impl<'a> Message<'a> {
    pub fn segments(&'a self) -> Vec<Cow<'a, str>> {
        match self {
            Message::Text(text) => cq_to_text(text),
            Message::Segments(segments) => {
                segments.iter().map(|segment| segment.to_text()).collect()
            }
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "snake_case")]
pub enum MessageSegment<'a> {
    Text {
        text: &'a str,
    },
    At {
        qq: i64,
        name: Option<&'a str>,
    },
    #[serde(other)]
    Unknown,
}

impl<'a> MessageSegment<'a> {
    pub fn to_text(&self) -> Cow<'a, str> {
        match self {
            &MessageSegment::Text { text } => text.into(),
            MessageSegment::At { qq, name } => {
                if let Some(name) = name.filter(|s| !s.is_empty()) {
                    format!("@{}", name).into()
                } else {
                    format!("@{}", qq).into()
                }
            }
            MessageSegment::Unknown => "".into(),
        }
    }
}

#[handler]
#[tracing::instrument(skip_all)]
pub async fn onebot(
    ws: WebSocket,
    peer: &RemoteAddr,
    Data(sink): Data<&RingSender<DanmakuPacket>>,
) -> impl IntoResponse {
    tracing::info!("connection from {}", peer);
    let sink = sink.clone();
    let config = Config::load();
    ws.on_upgrade(|mut socket| async move {
        while let Some(msg) = socket.next().await {
            let Ok(msg) = msg else { return };
            tracing::debug!("got message: {:?}", msg);

            if let WebSocketMessage::Text(msg) = msg {
                match handle_message_event(msg, &config).await {
                    Ok(Some(packet)) => {
                        sink.send(packet).expect("all middleware tasks are gone");
                    }
                    Ok(None) => {}
                    Err(e) => tracing::error!("failed to handle message: {}", e),
                }
            }
        }
    })
}

#[tracing::instrument]
async fn handle_message_event(message: String, config: &Config) -> Result<Option<DanmakuPacket>> {
    let event: MessageEvent = serde_json::from_str(&message)?;
    if event.post_type != "message" {
        return Ok(None);
    }
    if let Some(group) = event.group_id {
        if let Some(message) = event.message {
            let message = message.segments().join("");
            let message = message.trim();
            if message.chars().count() > config.max_length {
                return Ok(None);
            }
            let sender = event.sender.map(|sender| sender.name().into());
            tracing::debug!("{:?} -> {}", sender, message);

            let danmaku = Danmaku {
                text: message.into(),
                color: None, // TODO: customize
                size: None,
                sender,
            };
            let group = group.to_smolstr();
            let packet = DanmakuPacket { group, danmaku };
            return Ok(Some(packet));
        }
    }

    Ok(None)
}
