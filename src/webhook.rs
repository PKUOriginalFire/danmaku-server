//! Official QQ bot WebHook

use ed25519_dalek::{ed25519::signature::SignerMut, SecretKey, SigningKey};
use eyre::Result;
use poem::{
    handler,
    web::{Data, Json},
    IntoResponse,
};
use ring_channel::RingSender;
use serde::{Deserialize, Serialize};

use crate::{
    config::Config,
    danmaku::{Danmaku, DanmakuPacket},
};

/// Integer tag support from https://github.com/serde-rs/serde/issues/745#issuecomment-1450072069
#[derive(Debug)]
pub struct Op<const V: u8>;

impl<'de, const V: u8> Deserialize<'de> for Op<V> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if u8::deserialize(deserializer)? == V {
            Ok(Op::<V>)
        } else {
            Err(serde::de::Error::custom("invalid op"))
        }
    }
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
#[serde(untagged)]
enum Payload {
    Dispatch {
        id: String,
        op: Op<0>,
        d: serde_json::Value,
    },
    Heartbeat {
        op: Op<1>,
        d: i64,
    },
    Validate {
        op: Op<13>,
        d: Validate,
    },
}

#[derive(Deserialize, Debug)]
struct Validate {
    plain_token: String,
    event_ts: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Sign {
    plain_token: String,
    signature: String,
}

#[derive(Deserialize, Debug)]
struct Message {
    content: Option<String>,
    channel_id: String,
    author: User,
}

#[derive(Deserialize, Debug)]
struct User {
    username: String,
}

#[handler]
#[tracing::instrument(skip_all)]
pub async fn webhook(
    Json(payload): Json<Payload>,
    Data(sink): Data<&RingSender<DanmakuPacket>>,
) -> impl IntoResponse {
    let config = Config::load();

    tracing::debug!("payload: {:?}", payload);

    match payload {
        Payload::Validate { d, .. } => {
            // Populate secret length to 32 bytes
            let seed = config
                .bot_secret
                .as_bytes()
                .iter()
                .cycle()
                .take(ed25519_dalek::SECRET_KEY_LENGTH)
                .copied()
                .collect::<Vec<u8>>();
            let seed = SecretKey::try_from(seed).unwrap();

            let mut signer = SigningKey::from(seed);

            let mut msg = d.event_ts.clone();
            msg.push_str(&d.plain_token);

            let signature = signer.sign(msg.as_bytes());
            let signature = hex::encode(signature.to_bytes());

            Json(Sign {
                plain_token: d.plain_token.clone(),
                signature,
            })
            .into_response()
        }
        Payload::Heartbeat { d, .. } => {
            Json(serde_json::json!({ "op": 11, "d": d })).into_response()
        }
        Payload::Dispatch { id, d, .. } => {
            // TODO: check signature by header (https://bot.q.qq.com/wiki/develop/api-v2/dev-prepare/interface-framework/sign.html)
            if id.starts_with("MESSAGE_CREATE") {
                match receive_message(&d, &config) {
                    Ok(Some(packet)) => {
                        sink.send(packet).expect("all middleware tasks are gone");
                    }
                    Ok(None) => {}
                    Err(e) => tracing::error!("failed to handle message: {}", e),
                }
            }
            Json(serde_json::json!({ "op": 12, "d": 0 })).into_response()
        }
    }
}

#[tracing::instrument]
fn receive_message(data: &serde_json::Value, config: &Config) -> Result<Option<DanmakuPacket>> {
    let msg: Message = serde_json::from_value(data.clone())?;

    thread_local! {
        // clean @user mention
        static RE: regex::Regex = regex::Regex::new(r"^<@!?[0-9]+>").unwrap();
    }

    if let Some(message) = msg.content {
        let message = RE.with(|re| re.replace_all(&message, ""));
        let message = message.trim();
        if message.chars().count() > config.max_length {
            return Ok(None);
        }
        let sender = Some(msg.author.username.into());
        tracing::debug!("{:?} -> {}", sender, message);

        let danmaku = Danmaku {
            text: message.into(),
            color: None, // TODO: customize
            size: None,
            sender,
        };
        return Ok(Some(DanmakuPacket {
            group: msg.channel_id.parse()?,
            danmaku,
        }));
    }
    Ok(None)
}
