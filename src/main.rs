use eyre::Result;
use futures::FutureExt;
use poem::listener::TcpListener;
use poem::middleware::{NormalizePath, TrailingSlash};
use poem::web::Html;
use poem::{get, handler, EndpointExt, IntoResponse, Route, Server};
use tokio::sync::broadcast;
use tracing_subscriber::EnvFilter;

use crate::danmaku::DanmakuPacket;

mod config;
mod danmaku;
mod onebot;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = config::Config::load()?;

    // gracefully shutdown on ctrl-c or SIGTERM
    tokio::spawn(async move {
        let signal = tokio::signal::ctrl_c();

        #[cfg(unix)]
        let signal = futures::future::select(signal.boxed(), {
            use tokio::signal::unix::{signal, SignalKind};
            let mut signal = signal(SignalKind::terminate()).unwrap();
            let signal = async move { signal.recv().await }.boxed();
            signal
        });

        signal.await;
        tracing::info!("shutting down");
        std::process::exit(0);
    });

    // server
    let channel = broadcast::channel::<DanmakuPacket>(32).0;
    let app = Route::new()
        .at("/", get(index))
        .at("/onebot", get(onebot::endpoint.data(channel.clone())))
        .at("/danmaku/:id", get(danmaku::endpoint.data(channel.clone())))
        .with(NormalizePath::new(TrailingSlash::Trim));
    tokio::spawn(echo(channel));

    tracing::info!("listening on {}:{}", config.listen, config.port);
    Server::new(TcpListener::bind((config.listen, config.port)))
        .run(app)
        .await?;
    Ok(())
}

#[handler]
fn index() -> impl IntoResponse {
    Html(include_str!("index.html"))
}

#[tracing::instrument]
async fn echo(channel: broadcast::Sender<DanmakuPacket>) {
    let mut channel = channel.subscribe();
    while let Ok(packet) = channel.recv().await {
        tracing::info!("{} -> {}", packet.group, packet.danmaku);
    }
}
