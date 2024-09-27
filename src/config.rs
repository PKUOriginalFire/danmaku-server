use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;
use std::{borrow::Cow, net::IpAddr};

use confique::Config as _;
use eyre::{Context, Result};
use parking_lot::Mutex;

#[derive(confique::Config, Debug)]
pub struct Config {
    /// Bind address
    #[config(default = "0.0.0.0")]
    pub listen: IpAddr,

    /// Port to listen on
    #[config(default = 5098)]
    pub port: u16,

    /// Group IDs to watch
    #[config(default = [])]
    pub groups: Vec<i64>,
}

impl Config {
    const DEFAULT_PATH: &'static str = "/etc/danmaku-server/config.toml";

    pub fn path() -> Cow<'static, str> {
        std::env::var("DANMAKU_SERVER_CONFIG").map_or(Self::DEFAULT_PATH.into(), Cow::Owned)
    }

    /// Get the config (cached)
    pub fn get() -> Result<Arc<Self>> {
        static CONFIG: Mutex<Option<(Arc<Config>, SystemTime)>> = Mutex::new(None);

        let path = Self::path();
        let path = Path::new(path.as_ref());

        // If the config file has not been modified, return the cached config
        let mut config = CONFIG.lock();
        if let Some((config, last_modified)) = config.as_ref() {
            let modified = path
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::now());
            if modified == *last_modified {
                return Ok(config.clone());
            }
        }

        // Otherwise, load the new config and update the cache
        let new_config = Arc::new(Self::load()?);
        let modified = path
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::now());
        *config = Some((new_config.clone(), modified));
        Ok(new_config)
    }

    /// Load the config from disk and environment variables
    fn load() -> Result<Self> {
        let path = Self::path();
        let path = Path::new(path.as_ref());
        if !path.exists() {
            let template = confique::toml::template::<Config>(Default::default());
            tracing::info!("creating default config at {}", path.display());
            std::fs::create_dir_all(path.parent().unwrap())?;
            std::fs::write(path, template)?;
        }

        path.metadata()?.modified()?;

        let config = Config::builder()
            .env()
            .file("/etc/danmaku-server/config.toml")
            .load()
            .wrap_err("failed to load config")?;
        tracing::debug!("loaded config: {:?}", config);
        Ok(config)
    }
}
