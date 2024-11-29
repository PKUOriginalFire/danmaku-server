use std::sync::Arc;
use std::time::Duration;
use std::vec;

use futures::StreamExt;
use governor::{DefaultKeyedRateLimiter, Quota};
use regex::RegexSet;
use ring_channel::RingReceiver;
use tokio::sync::broadcast;

use crate::config::Config;
use crate::danmaku::DanmakuPacket;

/// Danmaku Middleware
trait Middleware {
    fn run(&mut self, packet: DanmakuPacket) -> Option<DanmakuPacket>;
}

/// Middleware Chain
struct MiddlewareChain {
    middlewares: Vec<Box<dyn Middleware + Send>>,
}

impl MiddlewareChain {
    fn new() -> Self {
        Self {
            middlewares: vec![],
        }
    }

    fn add(&mut self, middleware: Option<impl Middleware + Send + 'static>) {
        if let Some(middleware) = middleware {
            self.middlewares.push(Box::new(middleware));
        }
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

/// Display incoming danmaku to log
struct Echo;

impl Middleware for Echo {
    #[tracing::instrument(skip(self, packet))]
    fn run(&mut self, packet: DanmakuPacket) -> Option<DanmakuPacket> {
        tracing::info!("{} -> {}", packet.group, packet.danmaku);
        Some(packet)
    }
}

/// Deduplicate danmaku coming within a time window
struct Dedup(DefaultKeyedRateLimiter<(i64, Arc<str>)>);

impl Dedup {
    fn from_config(config: &Config) -> Option<Self> {
        if config.dedup_window <= 0 {
            return None;
        }
        let duration = Duration::from_secs(config.dedup_window as u64);
        Some(Self(DefaultKeyedRateLimiter::keyed(
            Quota::with_period(duration).expect("invalid quota"),
        )))
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

/// Filter danmaku by regex
struct RegexFilter(RegexSet);

impl RegexFilter {
    fn new() -> Self {
        const BLACKLIST: &str = include_str!("../assets/blacklist.txt");
        Self(RegexSet::new(BLACKLIST.trim().lines()).expect("invalid regex"))
    }
}

impl Middleware for RegexFilter {
    #[tracing::instrument(skip(self))]
    fn run(&mut self, packet: DanmakuPacket) -> Option<DanmakuPacket> {
        if self.0.is_match(&packet.danmaku.text) {
            tracing::info!("drop blacklisted: {}", packet.danmaku);
            return None;
        }

        Some(packet)
    }
}

#[tracing::instrument(skip(source, sink))]
pub async fn run_middleware(
    mut source: RingReceiver<DanmakuPacket>,
    sink: broadcast::Sender<DanmakuPacket>,
) {
    let config = Config::load();

    let mut chain = MiddlewareChain::new();
    chain.add(Some(Echo));
    chain.add(Dedup::from_config(&config));
    chain.add(Some(RegexFilter::new()));

    while let Some(packet) = source.next().await {
        if let Some(packet) = chain.run(packet) {
            sink.send(packet).ok();
        }
    }
}
