use std::net::IpAddr;

use envconfig::Envconfig;
use eyre::Result;

#[derive(Envconfig, Debug)]
pub struct Config {
    /// Bind address
    #[envconfig(from = "DANMAKU_LISTEN", default = "0.0.0.0")]
    pub listen: IpAddr,

    /// Port to listen on
    #[envconfig(from = "DANMAKU_PORT", default = "5098")]
    pub port: u16,
}

impl Config {
    /// Load the config from environment variables
    pub fn load() -> Result<Self> {
        let config = Config::init_from_env()?;
        tracing::debug!("loaded config: {:?}", config);
        Ok(config)
    }
}
