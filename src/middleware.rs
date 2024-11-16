use std::sync::Arc;
use std::time::Duration;
use std::vec;

use futures::StreamExt;
use governor::{DefaultKeyedRateLimiter, Quota};
use ring_channel::RingReceiver;
use tokio::sync::broadcast;

use crate::config::Config;
use crate::danmaku::DanmakuPacket;

trait Middleware {
    fn run(&mut self, packet: DanmakuPacket) -> Option<DanmakuPacket>;
}

struct MiddlewareChain {
    middlewares: Vec<Box<dyn Middleware + Send>>,
}

impl MiddlewareChain {
    fn new() -> Self {
        Self {
            middlewares: vec![],
        }
    }

    fn add(&mut self, middleware: impl Middleware + Send + 'static) {
        self.middlewares.push(Box::new(middleware));
    }
}

impl Middleware for MiddlewareChain {
    fn run(&mut self, packet: DanmakuPacket) -> Option<DanmakuPacket> {
        let mut packet = Some(packet);
        for middleware in &mut self.middlewares {
            packet = packet.and_then(|packet| middleware.run(packet));
        }
        packet
    }
}

struct Echo;

impl Middleware for Echo {
    #[tracing::instrument(skip(self))]
    fn run(&mut self, packet: DanmakuPacket) -> Option<DanmakuPacket> {
        tracing::info!("{} -> {}", packet.group, packet.danmaku);
        Some(packet)
    }
}

struct Dedup(DefaultKeyedRateLimiter<(i64, Arc<str>)>);

impl Dedup {
    fn from_config(config: &Config) -> Self {
        Self(DefaultKeyedRateLimiter::keyed(
            Quota::with_period(Duration::from_secs(config.dedup_window)).expect("invalid quota"),
        ))
    }
}

impl Middleware for Dedup {
    #[tracing::instrument(skip(self))]
    fn run(&mut self, packet: DanmakuPacket) -> Option<DanmakuPacket> {
        if self
            .0
            .check_key(&(packet.group, packet.danmaku.text.clone()))
            .is_ok()
        {
            Some(packet)
        } else {
            tracing::info!("drop duplicate: {}", packet.danmaku);
            None
        }
    }
}

#[tracing::instrument(skip(source, sink))]
pub async fn run_middleware(
    mut source: RingReceiver<DanmakuPacket>,
    sink: broadcast::Sender<DanmakuPacket>,
) {
    let config = Config::load();

    let mut chain = MiddlewareChain::new();
    chain.add(Echo);
    chain.add(Dedup::from_config(&config));

    while let Some(packet) = source.next().await {
        if let Some(packet) = chain.run(packet) {
            sink.send(packet).ok();
        }
    }
}
