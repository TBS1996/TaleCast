use crate::patterns::FullPattern;
use crate::patterns::SourceType;
use crate::utils::Unix;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Represents a [`PodcastConfig`] value that is either enabled, disabled, or we defer to the
/// global config. Only valid for optional values.
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

impl<T: Clone> ConfigOption<T> {
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled(_))
    }

    pub fn into_val(self, global_value: Option<&T>) -> Option<T> {
        match self {
            Self::Disabled => None,
            Self::Enabled(t) => Some(t),
            Self::UseGlobal => global_value.cloned(),
        }
    }
}

fn default_name_pattern() -> String {
    "{pubdate::%Y-%m-%d} {rss::episode::title}".to_string()
}

fn default_download_path() -> String {
    "{home}/talecast/{podname}".to_string()
}

fn default_id_pattern() -> String {
    "{guid}".to_string()
}

/// Configuration for a specific podcast.
#[derive(Debug, Clone)]
pub struct Config {
    pub url: String,
    pub name_pattern: FullPattern,
    pub id_pattern: FullPattern,
    pub download_path: FullPattern,
    pub tracker_path: FullPattern,
    pub id3_tags: HashMap<String, String>,
    pub download_hook: Option<PathBuf>,
    pub mode: DownloadMode,
}

impl Config {
    pub fn new(global_config: &GlobalConfig, podcast_config: PodcastConfig) -> Self {
        let mode = match (
            podcast_config.backlog_start,
            podcast_config.backlog_interval,
        ) {
            (None, None) => DownloadMode::Standard {
                max_days: podcast_config
                    .max_days
                    .into_val(global_config.max_days.as_ref()),
                max_episodes: podcast_config
                    .max_episodes
                    .into_val(global_config.max_episodes.as_ref()),
                earliest_date: podcast_config
                    .earliest_date
                    .into_val(global_config.earliest_date.as_ref()),
            },
            (Some(_), None) => {
                eprintln!("missing backlog_interval");
                std::process::exit(1);
            }
            (None, Some(_)) => {
                eprintln!("missing backlog_start");
                std::process::exit(1);
            }
            (Some(start), Some(interval)) => {
                if podcast_config.max_days.is_enabled() {
                    eprintln!("'max_days' not compatible with backlog mode.");
                    std::process::exit(1);
                }

                if podcast_config.max_episodes.is_enabled() {
                    eprintln!("'max_episodes' not compatible with backlog mode. Consider moving the start_date variable.");
                    std::process::exit(1);
                }

                if podcast_config.earliest_date.is_enabled() {
                    eprintln!("'earliest_date' not compatible with backlog mode.");
                    std::process::exit(1);
                }

                let Ok(start) = chrono::NaiveDate::parse_from_str(&start, "%Y-%m-%d") else {
                    eprintln!("invalid backlog_start format. Use YYYY-MM-DD");
                    std::process::exit(1);
                };

                let start = start.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();

                DownloadMode::Backlog {
                    start: std::time::Duration::from_secs(start as u64),
                    interval,
                }
            }
        };

        let id3_tags = {
            let mut map = HashMap::with_capacity(
                global_config.id3_tags.len() + podcast_config.id3_tags.len(),
            );

            for (key, val) in global_config.id3_tags.iter() {
                map.insert(key.clone(), val.clone());
            }

            for (key, val) in podcast_config.id3_tags.iter() {
                map.insert(key.clone(), val.clone());
            }
            map
        };

        let download_hook = podcast_config
            .download_hook
            .into_val(global_config.download_hook.as_ref());

        let download_path = podcast_config
            .download_path
            .unwrap_or_else(|| global_config.download_path.clone());

        let download_path = FullPattern::from_str(&download_path, vec![SourceType::Podcast]);

        let tracker_path = match podcast_config
            .tracker_path
            .into_val(global_config.tracker_path.as_ref())
        {
            Some(tracker_path) => FullPattern::from_str(&tracker_path, vec![SourceType::Podcast]),
            None => download_path.clone().append_text(".downloaded".to_owned()),
        };

        let name_pattern = podcast_config
            .name_pattern
            .into_val(Some(&global_config.name_pattern))
            .unwrap();

        let name_pattern = FullPattern::from_str(&name_pattern, SourceType::all());

        let id_pattern = podcast_config
            .id_pattern
            .unwrap_or_else(|| global_config.id_pattern.clone());

        let id_pattern = FullPattern::from_str(
            &id_pattern,
            vec![SourceType::Id3, SourceType::Podcast, SourceType::Episode],
        );

        let url = podcast_config.url;

        Self {
            url,
            name_pattern,
            id_pattern,
            mode,
            id3_tags,
            download_hook,
            download_path,
            tracker_path,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct GlobalConfig {
    #[serde(default = "default_download_path", alias = "path")]
    download_path: String,
    #[serde(default = "default_name_pattern")]
    name_pattern: String,
    #[serde(default = "default_id_pattern")]
    id_pattern: String,
    max_days: Option<i64>,
    max_episodes: Option<i64>,
    earliest_date: Option<String>,
    #[serde(default)]
    id3_tags: HashMap<String, String>,
    download_hook: Option<PathBuf>,
    tracker_path: Option<String>,
}

impl GlobalConfig {
    pub fn load(p: &Path) -> Self {
        if !p.exists() {
            let default = Self::default();
            let s = toml::to_string_pretty(&default).unwrap();
            let mut f = std::fs::File::create(&p).unwrap();
            f.write_all(s.as_bytes()).unwrap();
        }

        let str = std::fs::read_to_string(p).unwrap();
        toml::from_str(&str).unwrap()
    }
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            name_pattern: default_name_pattern(),
            download_path: default_download_path(),
            id_pattern: default_id_pattern(),
            max_days: None,
            max_episodes: Some(10),
            earliest_date: None,
            id3_tags: Default::default(),
            download_hook: None,
            tracker_path: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum DownloadMode {
    Standard {
        max_days: Option<i64>,
        earliest_date: Option<String>,
        max_episodes: Option<i64>,
    },
    Backlog {
        start: Unix,
        interval: i64,
    },
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct PodcastConfig {
    url: String,
    #[serde(alias = "path")]
    download_path: Option<String>,
    #[serde(default, deserialize_with = "deserialize_config_option_int")]
    max_days: ConfigOption<i64>,
    #[serde(default, deserialize_with = "deserialize_config_option_int")]
    max_episodes: ConfigOption<i64>,
    #[serde(default, deserialize_with = "deserialize_config_option_string")]
    earliest_date: ConfigOption<String>,
    #[serde(default, deserialize_with = "deserialize_config_option_pathbuf")]
    download_hook: ConfigOption<PathBuf>,
    #[serde(default, deserialize_with = "deserialize_config_option_string")]
    tracker_path: ConfigOption<String>,
    backlog_start: Option<String>,
    backlog_interval: Option<i64>,
    #[serde(default)]
    id3_tags: HashMap<String, String>,
    #[serde(default, deserialize_with = "deserialize_config_option_string")]
    name_pattern: ConfigOption<String>,
    id_pattern: Option<String>,
}

impl PodcastConfig {
    pub fn load_all() -> HashMap<String, Self> {
        let path = crate::utils::podcasts_toml();
        if !path.exists() {
            std::fs::File::create(&path).unwrap();
        }

        let config_str = std::fs::read_to_string(&path).unwrap();
        let map: HashMap<String, Self> = toml::from_str(&config_str).unwrap();
        if map.is_empty() {
            eprintln!("No podcasts configured!");
            eprintln!("Add podcasts with \"{} --add 'url' 'name'\" or by manually configuring the {:?} file.", crate::APPNAME, &path);
            std::process::exit(1);
        }
        map
    }
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

fn deserialize_config_option_pathbuf<'de, D>(
    deserializer: D,
) -> Result<ConfigOption<PathBuf>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde_json::Value;

    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        Some(Value::String(s)) => Ok(ConfigOption::Enabled(PathBuf::from(&s))),
        Some(Value::Bool(false)) => Ok(ConfigOption::Disabled),
        _ => Err(serde::de::Error::custom(
            "Invalid type for configuration option",
        )),
    }
}
