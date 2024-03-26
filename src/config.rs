use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

#[derive(Clone)]
pub struct CombinedConfig {
    global: Arc<GlobalConfig>,
    specific: PodcastConfig,
}

impl CombinedConfig {
    pub fn new(global: Arc<GlobalConfig>, specific: PodcastConfig) -> Self {
        Self { global, specific }
    }

    pub fn max_age(&self) -> Option<u32> {
        self.specific.max_age.or(self.global.max_age)
    }

    pub fn base_path(&self) -> PathBuf {
        PathBuf::from(self.specific.path.as_ref().unwrap_or(&self.global.path))
    }

    pub fn url(&self) -> &str {
        &self.specific.url
    }

    pub fn earliest_date(&self) -> Option<&str> {
        self.specific
            .earliest_date
            .as_ref()
            .or(self.global.earliest_date.as_ref())
            .map(String::as_str)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GlobalConfig {
    max_age: Option<u32>,
    path: String,
    earliest_date: Option<String>,
}

impl GlobalConfig {
    pub fn load() -> Result<Self> {
        let p = dirs::config_dir()
            .ok_or(anyhow::Error::msg("no config dir found"))?
            .join("config.toml");

        if !p.exists() {
            let default = Self::default();
            let s = toml::to_string_pretty(&default)?;
            let mut f = std::fs::File::create(&p)?;
            f.write_all(s.as_bytes())?;
        }

        let str = std::fs::read_to_string(p)?;

        Ok(toml::from_str(&str)?)
    }
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            max_age: Some(90),
            path: dirs::home_dir()
                .expect("home dir not found")
                .join("cringecast")
                .to_string_lossy()
                .to_string(),
            earliest_date: None,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct PodcastConfig {
    url: String,
    max_age: Option<u32>,
    path: Option<String>,
    earliest_date: Option<String>,
}
