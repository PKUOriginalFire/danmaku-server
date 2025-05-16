use std::net::IpAddr;
use std::sync::{Arc, OnceLock};

use envconfig::Envconfig;

#[derive(Envconfig, Debug)]
pub struct Config {
    /// Bind address
    #[envconfig(from = "DANMAKU_LISTEN", default = "0.0.0.0")]
    pub listen: IpAddr,

    /// Port to listen on
    #[envconfig(from = "DANMAKU_PORT", default = "5098")]
    pub port: u16,

    /// Port to listen on
    #[envconfig(from = "DANMAKU_PRIVATE_PORT", default = "5099")]
    pub private_port: u16,

    // Danmaku max length
    #[envconfig(from = "DANMAKU_MAX_LENGTH", default = "50")]
    pub max_length: usize,

    /// Danmaku deduplication window (in seconds)
    #[envconfig(from = "DANMAKU_DEDUP_WINDOW", default = "-1")]
    pub dedup_window: i32,

    /// Official QQBot Secret
    #[envconfig(from = "DANMAKU_BOT_SECRET", default = "0")]
    pub bot_secret: String,
}

impl Config {
    /// Load the config from environment variables
    pub fn load() -> Arc<Self> {
        static CONFIG: OnceLock<Arc<Config>> = OnceLock::new();
        let config = CONFIG.get_or_init(|| {
            let config = Config::init_from_env().expect("failed to load config");
            tracing::debug!("loaded config: {:?}", config);
            Arc::new(config)
        });
        config.clone()
    }
}
