use crate::patterns::FullPattern;
use crate::patterns::SourceType;
use crate::utils;
use crate::utils::Unix;
use regex::Regex;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;
use std::time;

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

/// Full configuration for a specific podcast.
///
/// Combines settings from [`GlobalConfig`] and [`PodcastConfig`].
#[derive(Debug, Clone)]
pub struct Config {
    pub url: String,
    pub name_pattern: FullPattern,
    pub id_pattern: FullPattern,
    pub download_path: FullPattern,
    pub tracker_path: FullPattern,
    pub id3_tags: HashMap<String, String>,
    pub download_hook: Option<PathBuf>,
    pub style: IndicatifSettings,
    pub mode: DownloadMode,
    pub symlink: Option<FullPattern>,
}

impl Config {
    pub fn new(global_config: &GlobalConfig, podcast_config: PodcastConfig) -> Self {
        let mode = match (
            podcast_config.backlog_start,
            podcast_config.backlog_interval,
        ) {
            (None, None) => DownloadMode::Standard {
                max_time: podcast_config
                    .max_days
                    .into_val(global_config.max_days.as_ref())
                    .map(|days| Unix::from_secs(days as u64 * 86400)),
                max_episodes: podcast_config
                    .max_episodes
                    .into_val(global_config.max_episodes.as_ref()),
                earliest_date: {
                    podcast_config
                        .earliest_date
                        .into_val(global_config.earliest_date.as_ref())
                        .map(|date| utils::date_str_to_unix(&date))
                },
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
                    eprintln!("'max_episodes' not compatible with backlog mode.");
                    eprintln!("If you want to limit the amount of episodes to download, consider changing the 'backlog_start' setting.");
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
                    interval: Unix::from_secs(interval as u64 * 86400),
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

        let download_path_str = podcast_config
            .download_path
            .unwrap_or_else(|| global_config.download_path.clone());

        let download_path = FullPattern::from_str(&download_path_str, vec![SourceType::Podcast]);

        let tracker_path = match podcast_config
            .tracker_path
            .into_val(global_config.tracker_path.as_ref())
        {
            Some(tracker_path) => FullPattern::from_str(&tracker_path, vec![SourceType::Podcast]),
            None => {
                if download_path_str.ends_with('/') {
                    let p = download_path_str + ".downloaded";
                    FullPattern::from_str(&p, vec![SourceType::Podcast])
                } else {
                    let p = download_path_str + "/.downloaded";
                    FullPattern::from_str(&p, vec![SourceType::Podcast])
                }
            }
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
        let style = global_config.style.clone();

        let symlink = podcast_config
            .symlink
            .or(global_config.symlink.clone())
            .map(|str| {
                FullPattern::from_str(
                    &str,
                    vec![SourceType::Id3, SourceType::Podcast, SourceType::Episode],
                )
            });

        Self {
            url,
            name_pattern,
            id_pattern,
            mode,
            id3_tags,
            download_hook,
            download_path,
            tracker_path,
            style,
            symlink,
        }
    }
}

fn default_user_agent() -> String {
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/58.0.3029.110 Safari/537.36".to_string()
}

#[derive(Serialize, Default, Deserialize, Debug, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct SearchSettings {
    max_results: Option<usize>,
    line_width: Option<usize>,
}

impl SearchSettings {
    fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Serialize, Default, Deserialize, Debug, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct IndicatifSettings {
    pub enabled: Option<bool>,
    pub download_bar: Option<String>,
    pub completed: Option<String>,
    pub hooks: Option<String>,
    pub spinner_speed: Option<u64>,
    pub title_length: Option<usize>,
}

impl IndicatifSettings {
    fn is_default(&self) -> bool {
        self == &Self::default()
    }

    fn default_download_template() -> String {
        "{spinner:.green}  {msg} {bar:15.cyan/blue} {bytes}/{total_bytes}".to_string()
    }

    fn default_complete_template() -> String {
        "✅ {msg}".to_owned()
    }

    fn default_hooks() -> String {
        "{spinner:.green} finishing up download hooks...".to_string()
    }

    pub fn download_template(&self) -> String {
        self.download_bar
            .clone()
            .unwrap_or_else(IndicatifSettings::default_download_template)
    }

    pub fn completion_template(&self) -> String {
        self.completed
            .clone()
            .unwrap_or_else(IndicatifSettings::default_complete_template)
    }

    pub fn hook_template(&self) -> String {
        self.hooks
            .clone()
            .unwrap_or_else(IndicatifSettings::default_hooks)
    }

    pub fn spinner_speed(&self) -> time::Duration {
        let millis = self.spinner_speed.unwrap_or(100);
        time::Duration::from_millis(millis)
    }

    pub fn title_length(&self) -> usize {
        self.title_length.unwrap_or(30)
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
    #[serde(default, skip_serializing_if = "IndicatifSettings::is_default")]
    style: IndicatifSettings,
    user_agent: Option<String>,
    #[serde(default, skip_serializing_if = "SearchSettings::is_default")]
    search: SearchSettings,
    symlink: Option<String>,
}

impl GlobalConfig {
    /// Loads the global config from the default path.
    ///
    /// If config is not present it'll create a default one.
    /// It'll always save after loading in case it's missing a required field
    /// which has a default. Reason is I want the user to see all the required
    /// fields in the `config.toml` file instead of just silently using the default
    /// value. This also makes the user aware of any new required fields after updating.
    /// Note that this means any comments will unfortunately be removed.
    pub fn load() -> Self {
        let path = Self::default_path();

        let config: Self = fs::read_to_string(&path)
            .ok()
            .and_then(|str| toml::from_str(&str).ok())
            .unwrap_or_default();

        config.save();
        config
    }

    /// Serializes the config to the default path.
    pub fn save(&self) {
        let path = Self::default_path();
        let str = toml::to_string(self).unwrap();
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(str.as_bytes()).unwrap();
    }

    /// For using a global config from a path specified as a commandline argument.
    /// Main difference from the normal loading is that it won't create a default one if it's missing.
    pub fn load_from_path(path: &Path) -> Self {
        if !path.exists() {
            eprintln!("no config located at {:?}", path);
            process::exit(1);
        };

        let str = match fs::read_to_string(&path) {
            Ok(str) => str,
            Err(e) => {
                eprintln!("unable to read given config file:{:?}\n{:?}", path, e);
                process::exit(1);
            }
        };

        match toml::from_str(&str) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("unable to parse given config file: {:?}\n{:?}", path, e);
                process::exit(1);
            }
        }
    }

    pub fn user_agent(&self) -> String {
        self.user_agent.clone().unwrap_or_else(default_user_agent)
    }

    pub fn default_path() -> PathBuf {
        utils::config_dir().join("config.toml")
    }

    pub fn max_search_results(&self) -> usize {
        self.search.max_results.unwrap_or(9)
    }

    pub fn max_line_width(&self) -> usize {
        self.search.line_width.unwrap_or(79)
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
            style: Default::default(),
            search: Default::default(),
            symlink: None,
            user_agent: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum DownloadMode {
    Standard {
        max_time: Option<Unix>,
        earliest_date: Option<Unix>,
        max_episodes: Option<i64>,
    },
    Backlog {
        start: Unix,
        interval: Unix,
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

    pub fn filter(self, filter: Option<Regex>) -> Self {
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

    /// All podcasts matching the regex will only download upcoming episodes.
    /// time. Podcasts with backlog mode ignored.
    pub fn catch_up(filter: Option<Regex>) {
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

    /// Appends the `podcast.toml` file with the given podcast.
    ///
    /// If a podcast with the same name already exist,
    /// it does nothing and will return false. Otherwise true.
    pub fn push(name: String, podcast: PodcastConfig) -> bool {
        let mut podcasts = Self::load();
        if podcasts.0.contains_key(&name) {
            false
        } else {
            podcasts.0.insert(name, podcast);
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

    pub fn into_outlines(self) -> Vec<opml::Outline> {
        self.0
            .into_iter()
            .map(|(name, pod)| opml::Outline {
                text: name.clone(),
                r#type: Some("rss".to_string()),
                xml_url: Some(pod.url.clone()),
                title: Some(name),
                ..opml::Outline::default()
            })
            .collect()
    }
}

impl From<PodcastConfigs> for opml::OPML {
    fn from(podcasts: PodcastConfigs) -> opml::OPML {
        use opml::{Body, Head, OPML};

        let mut opml = OPML {
            head: Some(Head {
                title: Some("TaleCast Podcast Feeds".to_string()),
                date_created: Some(chrono::Utc::now().to_rfc2822()),
                ..Head::default()
            }),
            ..Default::default()
        };

        let outlines = podcasts.into_outlines();

        opml.body = Body { outlines };

        opml
    }
}

impl IntoIterator for PodcastConfigs {
    type Item = (String, PodcastConfig);
    type IntoIter = std::collections::hash_map::IntoIter<String, PodcastConfig>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a PodcastConfigs {
    type Item = (&'a String, &'a PodcastConfig);
    type IntoIter = std::collections::hash_map::Iter<'a, String, PodcastConfig>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// The configuration of a specific podcast.
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
    symlink: Option<String>,
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
            symlink: Default::default(),
        }
    }

    pub fn path() -> PathBuf {
        let path = utils::config_dir().join("podcasts.toml");

        if !path.exists() {
            fs::File::create(&path).unwrap();
        }

        path
    }

    /// Changes the `earliest_date` setting to the current time.
    ///
    /// This means only episodes published after this function was called will be downloaded.
    /// Remember to save after calling this function.
    pub fn catch_up(&mut self) -> bool {
        use chrono::DateTime;

        let unix = utils::current_unix();
        let current_date = DateTime::from_timestamp(unix.as_secs() as i64, 0)
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
