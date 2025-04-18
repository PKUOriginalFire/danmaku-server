//! Official QQ bot WebHook

use ed25519_dalek::{ed25519::signature::SignerMut, SecretKey, SigningKey};
use eyre::Result;
use poem::{handler, web::Json, IntoResponse};
use serde::{Deserialize, Serialize};

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
    Dispatch { op: Op<0>, d: serde_json::Value },
    Heartbeat { op: Op<1>, d: i64 },
    Validate { op: Op<13>, d: Validate },
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

#[handler]
#[tracing::instrument(skip_all)]
pub async fn endpoint(Json(payload): Json<Payload>) -> impl IntoResponse {
    let config = crate::config::Config::load();

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
            tracing::debug!("heartbeat: {:?}", d);
            Json(serde_json::json!({ "op": 11, "d": d })).into_response()
        }
        Payload::Dispatch { d, .. } => {
            tracing::debug!("dispatch: {:?}", d);
            Json(serde_json::json!({ "op": 12, "d": 0 })).into_response()
        }
    }
}
