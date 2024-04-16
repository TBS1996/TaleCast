use crate::patterns::FullPattern;
use crate::patterns::SourceType;
use crate::utils;
use crate::utils::Unix;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Represents a [`PodcastConfig`] value that is either enabled, disabled,
/// or deferring to the global config. Only valid for optional values.
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

impl<'de, T> Deserialize<'de> for ConfigOption<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        let value = Option::<serde_json::Value>::deserialize(deserializer)?;
        match value {
            None => Ok(ConfigOption::UseGlobal),
            Some(serde_json::Value::Bool(false)) => Ok(ConfigOption::Disabled),
            Some(v) => T::deserialize(v.into_deserializer())
                .map(ConfigOption::Enabled)
                .map_err(|_| D::Error::custom("Invalid type for configuration option")),
        }
    }
}

impl<T> Serialize for ConfigOption<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match *self {
            ConfigOption::Enabled(ref value) => value.serialize(serializer),
            ConfigOption::Disabled => serializer.serialize_bool(false),
            ConfigOption::UseGlobal => serializer.serialize_none(),
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

                let Ok(start) = dateparser::parse(&start) else {
                    eprintln!("invalid backlog_start format.");
                    std::process::exit(1);
                };

                DownloadMode::Backlog {
                    start: std::time::Duration::from_secs(start.timestamp() as u64),
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
            .unwrap_or_else(|| global_config.name_pattern.clone());

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
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    id3_tags: HashMap<String, String>,
    download_hook: Option<PathBuf>,
    tracker_path: Option<String>,
}

impl GlobalConfig {
    pub fn load() -> Self {
        let path = Self::default_path();
        Self::load_from_path(&path)
    }

    pub fn load_from_path(path: &Path) -> Self {
        let str = std::fs::read_to_string(&path).unwrap();
        toml::from_str(&str).unwrap()
    }

    pub fn default_path() -> PathBuf {
        let path = crate::utils::config_dir().join("config.toml");

        if !path.exists() {
            let default = Self::default();
            let s = toml::to_string_pretty(&default).unwrap();
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(s.as_bytes()).unwrap();
        }

        path
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PodcastConfigs(pub HashMap<String, PodcastConfig>);

impl PodcastConfigs {
    pub fn load() -> Self {
        let path = Self::path();

        let config_str = std::fs::read_to_string(&path).unwrap();
        let map: HashMap<String, PodcastConfig> = toml::from_str(&config_str).unwrap();

        PodcastConfigs(map)
    }

    pub fn filter(self, filter: Option<regex::Regex>) -> Self {
        let inner = self
            .0
            .into_iter()
            .filter(|(name, _)| match filter {
                Some(ref filter) => filter.is_match(&name),
                None => true,
            })
            .collect();

        Self(inner)
    }

    /// All podcasts matching the regex will not download episodes published earlier than current
    /// time. Podcasts with backlog mode ignored.
    pub fn catch_up(filter: Option<regex::Regex>) {
        let mut podcasts = Self::load().filter(filter);

        for (name, config) in &mut podcasts.0 {
            if config.catch_up() {
                eprintln!("caught up with {}", &name);
            }
        }

        podcasts.save_modified();
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn save_modified(self) {
        let mut all_podcasts = Self::load();
        for (name, config) in self.0 {
            all_podcasts.0.insert(name, config);
        }

        all_podcasts.save_to_file();
    }

    pub fn save_to_file(self) {
        use std::fs::File;

        let str = toml::to_string(&self).unwrap();
        let path = Self::path();

        File::create(&path)
            .unwrap()
            .write_all(str.as_bytes())
            .unwrap();
    }

    pub fn extend(new_podcasts: HashMap<String, PodcastConfig>) {
        let mut podcasts = Self::load();
        for (name, podcast) in new_podcasts {
            if !podcasts.0.contains_key(&name) {
                podcasts.0.insert(name, podcast);
            }
        }

        podcasts.save_to_file();
    }

    pub fn push(name: String, url: String) -> bool {
        let mut podcasts = Self::load();
        if podcasts.0.contains_key(&name) {
            false
        } else {
            let new_podcast = PodcastConfig::new(url);
            podcasts.0.insert(name, new_podcast);
            podcasts.save_to_file();

            true
        }
    }

    pub fn path() -> PathBuf {
        let path = utils::config_dir().join("podcasts.toml");

        if !path.exists() {
            std::fs::File::create(&path).unwrap();
        }

        path
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct PodcastConfig {
    url: String,
    name_pattern: Option<String>,
    id_pattern: Option<String>,
    #[serde(alias = "path")]
    download_path: Option<String>,
    backlog_start: Option<String>,
    backlog_interval: Option<i64>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    id3_tags: HashMap<String, String>,
    max_days: ConfigOption<i64>,
    max_episodes: ConfigOption<i64>,
    earliest_date: ConfigOption<String>,
    download_hook: ConfigOption<PathBuf>,
    tracker_path: ConfigOption<String>,
}

impl PodcastConfig {
    pub fn new(url: String) -> Self {
        Self {
            url,
            name_pattern: Default::default(),
            id_pattern: Default::default(),
            download_path: Default::default(),
            backlog_start: Default::default(),
            backlog_interval: Default::default(),
            id3_tags: Default::default(),
            max_days: Default::default(),
            max_episodes: Default::default(),
            earliest_date: Default::default(),
            download_hook: Default::default(),
            tracker_path: Default::default(),
        }
    }

    pub fn catch_up(&mut self) -> bool {
        use chrono::DateTime;

        let unix = utils::current_unix();
        let current_date = DateTime::from_timestamp(unix, 0)
            .unwrap()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        if self.backlog_start.is_some() || self.backlog_interval.is_some() {
            return false;
        }

        self.earliest_date = ConfigOption::Enabled(current_date.clone());

        true
    }
}
