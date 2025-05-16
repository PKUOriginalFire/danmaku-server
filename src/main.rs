use std::sync::Arc;

use eyre::Result;
use futures::FutureExt;
use poem::listener::TcpListener;
use poem::middleware::{NormalizePath, TrailingSlash};
use poem::web::Html;
use poem::{get, handler, post, EndpointExt, IntoResponse, Route, Server};
use ring_channel::ring_channel;
use tokio::sync::broadcast;
use tracing_subscriber::EnvFilter;

use crate::danmaku::DanmakuPacket;
use crate::middleware::run_middleware;

mod config;
mod danmaku;
mod middleware;
mod onebot;
mod webhook;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = config::Config::load();

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
    // upstream -|ring_channel|-> middlewares -|broadcast|-> downstream
    let (source, middle) = ring_channel::<DanmakuPacket>(32.try_into().unwrap());
    let sink = broadcast::channel::<DanmakuPacket>(32).0;
    tokio::spawn(run_middleware(middle, sink.clone()));

    // public server
    let app = Route::new()
        .at("/:id", get(index))
        .at("/client/:id", get(client))
        // .at("/webhook", post(webhook::webhook.data(source.clone())))
        .at(
            "/danmaku/:id",
            get(danmaku::client
                .data(source.clone())
                .data(Arc::new(sink.subscribe()))),
        )
        .with(NormalizePath::new(TrailingSlash::Trim));

    tracing::info!(
        "public service listening on {}:{}",
        config.listen,
        config.port
    );
    let public = Server::new(TcpListener::bind((config.listen, config.port))).run(app);

    // private server
    let app = Route::new()
        .at("/onebot", get(onebot::onebot.data(source.clone())))
        .at("/webhook", post(webhook::webhook.data(source.clone())))
        .at("/danmaku", get(danmaku::upstream.data(source.clone())))
        .with(NormalizePath::new(TrailingSlash::Trim));

    tracing::info!(
        "private listening on {}:{}",
        config.listen,
        config.private_port
    );
    let private = Server::new(TcpListener::bind((config.listen, config.private_port))).run(app);

    tokio::try_join!(public, private)?;
    Ok(())
}

#[handler]
fn index() -> impl IntoResponse {
    Html(include_str!("../frontend/dist/index.html"))
}

#[handler]
fn client() -> impl IntoResponse {
    Html(include_str!("../frontend/dist/client.html"))
}
