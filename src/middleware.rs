use futures::StreamExt;
use ring_channel::RingReceiver;
use tokio::sync::broadcast;

use crate::danmaku::DanmakuPacket;

#[tracing::instrument(skip(source, sink))]
pub async fn run_middleware(
    mut source: RingReceiver<DanmakuPacket>,
    sink: broadcast::Sender<DanmakuPacket>,
) {
    while let Some(packet) = source.next().await {
        if let Some(packet) = Some(packet).and_then(echo) {
            sink.send(packet).ok();
        }
    }
}

#[tracing::instrument]
fn echo(packet: DanmakuPacket) -> Option<DanmakuPacket> {
    tracing::info!("{} -> {}", packet.group, packet.danmaku);
    Some(packet)
}
