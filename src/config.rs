use crate::Unix;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

/// Represents a [`PodcastConfig`] value that is either enabled, disabled, or we defer to the
/// global config.
#[derive(Clone, Copy, Debug, Default)]
pub enum ConfigOption<T> {
    /// Defer to the value in the global config.
    #[default]
    UseGlobal,
    /// Use this value for configuration.
    Enabled(T),
    /// Don't use any values.
    Disabled,
}

#[derive(Clone, Debug)]
pub struct CombinedConfig {
    global: Arc<GlobalConfig>,
    specific: PodcastConfig,
}

impl CombinedConfig {
    pub fn new(global: Arc<GlobalConfig>, specific: PodcastConfig) -> Self {
        Self { global, specific }
    }

    pub fn max_days(&self) -> Option<i64> {
        let DownloadMode::Standard { max_days, .. } = self.specific.mode else {
            return None;
        };

        match max_days {
            ConfigOption::Disabled => None,
            ConfigOption::UseGlobal => self.global.max_days,
            ConfigOption::Enabled(age) => Some(age),
        }
    }

    pub fn max_episodes(&self) -> Option<i64> {
        let DownloadMode::Standard { max_episodes, .. } = self.specific.mode else {
            return None;
        };

        match max_episodes {
            ConfigOption::Disabled => None,
            ConfigOption::UseGlobal => self.global.max_episodes,
            ConfigOption::Enabled(age) => Some(age),
        }
    }

    pub fn base_path(&self) -> &Path {
        self.specific.path.as_ref().unwrap_or(&self.global.path)
    }

    pub fn url(&self) -> &str {
        &self.specific.url
    }

    pub fn mode(&self) -> DownloadMode {
        self.specific.mode
    }

    pub fn earliest_date(&self) -> Option<&str> {
        match &self.specific.earliest_date {
            ConfigOption::Disabled => None,
            ConfigOption::UseGlobal => self.global.earliest_date.as_ref().map(|x| x.as_str()),
            ConfigOption::Enabled(date) => Some(&date),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct GlobalConfig {
    max_days: Option<i64>,
    max_episodes: Option<i64>,
    path: PathBuf,
    earliest_date: Option<String>,
}

impl GlobalConfig {
    pub fn load() -> Result<Self> {
        let config_dir = dirs::config_dir()
            .ok_or(anyhow::Error::msg("no config dir found"))?
            .join("cringecast");

        std::fs::create_dir_all(&config_dir).unwrap();

        let p = config_dir.join("config.toml");

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
            max_days: Some(120),
            max_episodes: Some(10),
            path: dirs::home_dir()
                .expect("home dir not found")
                .join("cringecast"),
            earliest_date: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum DownloadMode {
    Standard {
        max_days: ConfigOption<i64>,
        max_episodes: ConfigOption<i64>,
    },
    Backlog {
        start: Unix,
        interval: i64,
    },
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
struct RawPodcastConfig {
    url: String,
    path: Option<PathBuf>,
    #[serde(default, deserialize_with = "deserialize_config_option_int")]
    max_days: ConfigOption<i64>,
    #[serde(default, deserialize_with = "deserialize_config_option_int")]
    max_episodes: ConfigOption<i64>,
    #[serde(default, deserialize_with = "deserialize_config_option_string")]
    earliest_date: ConfigOption<String>,
    backlog_start: Option<String>,
    backlog_interval: Option<i64>,
}

impl From<RawPodcastConfig> for PodcastConfig {
    fn from(config: RawPodcastConfig) -> Self {
        let mode = match (config.backlog_start, config.backlog_interval) {
            (None, None) => DownloadMode::Standard {
                max_days: config.max_days,
                max_episodes: config.max_episodes,
            },
            (Some(_), None) => {
                println!("missing backlog_interval");
                std::process::exit(1);
            }
            (None, Some(_)) => {
                println!("missing backlog_start");
                std::process::exit(1);
            }
            (Some(start), Some(interval)) => {
                let start = chrono::NaiveDate::parse_from_str(&start, "%Y-%m-%d")
                    .expect("invalid backlog_start format. Use YYYY-MM-DD")
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc()
                    .timestamp();
                DownloadMode::Backlog { start, interval }
            }
        };

        Self {
            url: config.url,
            path: config.path,
            earliest_date: config.earliest_date,
            mode,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(from = "RawPodcastConfig")]
pub struct PodcastConfig {
    url: String,
    path: Option<PathBuf>,
    earliest_date: ConfigOption<String>,
    mode: DownloadMode,
}

fn deserialize_config_option_int<'de, D>(deserializer: D) -> Result<ConfigOption<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde_json::Value;

    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        Some(Value::Number(n)) if n.is_i64() => Ok(ConfigOption::Enabled(n.as_i64().unwrap())),
        Some(Value::Bool(false)) => Ok(ConfigOption::Disabled),
        _ => Err(serde::de::Error::custom(
            "Invalid type for configuration option",
        )),
    }
}

fn deserialize_config_option_string<'de, D>(
    deserializer: D,
) -> Result<ConfigOption<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde_json::Value;

    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        Some(Value::String(s)) => Ok(ConfigOption::Enabled(s)),
        Some(Value::Bool(false)) => Ok(ConfigOption::Disabled),
        _ => Err(serde::de::Error::custom(
            "Invalid type for configuration option",
        )),
    }
}
