use crate::display::DownloadBar;
use crate::episode;
use crate::patterns::Evaluate;
use crate::patterns::FullPattern;
use crate::podcast::Podcast;
use crate::podcast::RawPodcast;
use crate::utils;
use crate::utils::Unix;
use futures::future;
use indicatif::MultiProgress;
use regex::Regex;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
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

/// Data needed to evaluate a [`FullPattern`].
#[derive(Clone, Copy)]
pub struct EvalData<'a> {
    pub pod_name: &'a str,
    pub podcast: &'a RawPodcast,
    pub episode: &'a episode::Attributes,
}

impl<'a> EvalData<'a> {
    pub fn new(
        pod_name: &'a str,
        podcast: &'a RawPodcast,
        episode: &'a episode::Attributes,
    ) -> Self {
        Self {
            pod_name,
            podcast,
            episode,
        }
    }
}

/// Full configuration for a specific podcast-episode.
///
/// Combines settings from [`GlobalConfig`] and [`PodcastConfig`].
/// Must be computed for every episode because config might contain patterns unique to episode.
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub url: String,
    pub name_pattern: String,
    pub id_pattern: String,
    pub download_path: PathBuf,
    pub partial_path: Option<PathBuf>,
    pub tracker_path: PathBuf,
    pub symlink: Option<PathBuf>,
    pub id3_tags: HashMap<String, String>,
    pub download_hook: Option<PathBuf>,
}

impl Config {
    pub fn new(
        global_config: &GlobalConfig,
        podcast_config: &PodcastConfig,
        data: EvalData<'_>,
    ) -> Self {
        let podcast_config = podcast_config.to_owned();
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

        let download_path = FullPattern::direct_eval_dir(&download_path_str, data);

        let tracker_path = match podcast_config
            .tracker_path
            .into_val(global_config.tracker_path.as_ref())
        {
            Some(tracker_path) => tracker_path,
            None => {
                if download_path_str.ends_with('/') {
                    download_path_str + ".downloaded"
                } else {
                    download_path_str + "/.downloaded"
                }
            }
        };

        let tracker_path = FullPattern::direct_eval_file(&tracker_path, data);

        let name_pattern = FullPattern::from_str(
            &podcast_config
                .name_pattern
                .unwrap_or_else(|| global_config.name_pattern.clone()),
        )
        .evaluate(data);

        let id_pattern = podcast_config
            .id_pattern
            .unwrap_or_else(|| global_config.id_pattern.clone());

        let id_pattern = FullPattern::from_str(&id_pattern).evaluate(data);

        let symlink = podcast_config
            .symlink
            .or(global_config.symlink.clone())
            .map(|str| FullPattern::direct_eval_dir(str.as_ref(), data));

        let partial_path = podcast_config
            .partial_path
            .or(global_config.partial_path.clone())
            .map(|str| FullPattern::direct_eval_dir(str.as_ref(), data));

        Config {
            url: podcast_config.url.clone(),
            name_pattern,
            id_pattern,
            download_path,
            partial_path,
            tracker_path,
            symlink,
            id3_tags: id3_tags.clone(),
            download_hook: download_hook.clone(),
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
    pattern: Option<String>,
}

impl SearchSettings {
    fn is_default(&self) -> bool {
        self == &Self::default()
    }

    pub fn pattern(&self) -> String {
        self.pattern.clone().unwrap_or_else(Self::default_pattern)
    }

    fn default_pattern() -> String {
        "{collectionName} - {artistName}".to_string()
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

    fn default_error_template() -> String {
        "❌ {msg}".to_owned()
    }

    fn default_hooks() -> String {
        "{spinner:.green} finishing up download hooks...".to_string()
    }

    fn default_podcast_fetch_template() -> String {
        "{spinner:.green}  {msg}fetching podcast...".to_string()
    }

    pub fn podcast_fetch_template() -> String {
        Self::default_podcast_fetch_template()
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

    pub fn error_template(&self) -> String {
        Self::default_error_template()
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

    pub fn enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct GlobalConfig {
    #[serde(default = "default_download_path", alias = "path")]
    download_path: String,
    partial_path: Option<String>,
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
    style: Arc<IndicatifSettings>,
    user_agent: Option<String>,
    #[serde(default, skip_serializing_if = "SearchSettings::is_default")]
    search: SearchSettings,
    symlink: Option<String>,
    #[serde(default, skip_serializing_if = "LogConfig::is_default")]
    log: Arc<LogConfig>,
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
        if !path.exists() {
            let config = Self::default();
            config.save();
            return config;
        }

        let str = match fs::read_to_string(&path) {
            Ok(str) => str,
            Err(e) => {
                eprintln!("unable to read config file: {:?}", e);
                process::exit(1);
            }
        };

        let config: Self = match toml::from_str(&str) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("unable to parse config file: {:?}", e);
                process::exit(1);
            }
        };

        config.save();
        config
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

    pub fn style(&self) -> Arc<IndicatifSettings> {
        Arc::clone(&self.style)
    }

    pub fn log(&self) -> Arc<LogConfig> {
        Arc::clone(&self.log)
    }

    /// Serializes the config to the default path.
    pub fn save(&self) {
        let path = Self::default_path();
        let str = toml::to_string(self).unwrap();
        let mut f = std::fs::File::create(&path).expect("unable to create config file");
        f.write_all(str.as_bytes()).unwrap();
    }

    pub fn user_agent(&self) -> String {
        self.user_agent.clone().unwrap_or_else(default_user_agent)
    }

    pub fn search_settings(&self) -> &SearchSettings {
        &self.search
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
            log: Default::default(),
            symlink: None,
            user_agent: None,
            partial_path: None,
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
        max_episodes: Option<i64>,
    },
}

impl DownloadMode {
    pub fn new(global_config: &GlobalConfig, podcast_config: &PodcastConfig) -> Self {
        match (
            podcast_config.backlog_start.clone(),
            podcast_config.backlog_interval.clone(),
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
                        .clone()
                        .into_val(global_config.earliest_date.as_ref())
                        .map(|date| {
                            utils::date_str_to_unix(&date)
                                .expect("failed to parse earliest_date string")
                        })
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
                    max_episodes: podcast_config
                    .max_episodes
                    .into_val(global_config.max_episodes.as_ref()),
                }
            }
        }
    }
}

impl Default for DownloadMode {
    fn default() -> Self {
        Self::Standard {
            max_time: None,
            earliest_date: None,
            max_episodes: None,
        }
    }
}

fn init_reqwest_client(config: &GlobalConfig) -> Arc<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(&config.user_agent())
        .build()
        .map(Arc::new)
        .expect("error: failed to instantiate reqwest client")
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PodcastConfigs(HashMap<String, PodcastConfig>);

impl PodcastConfigs {
    pub async fn sync(self, global_config: GlobalConfig, log_file: &Path) -> Vec<PathBuf> {
        eprintln!("syncing {} podcasts", self.len());
        log::info!("syncing podcasts..");

        let mp = MultiProgress::new();
        let global_config = Arc::new(global_config);
        let client = init_reqwest_client(&global_config);

        let Some(longest_name) = self.longest_name() else {
            return vec![];
        };

        let error_occured = Arc::new(AtomicBool::new(false));

        let futures = self
            .into_inner()
            .into_iter()
            .map(|(name, config)| {
                let client = Arc::clone(&client);
                let settings = global_config.style();
                let mut ui = DownloadBar::new(name.clone(), settings, &mp, longest_name);
                let global_config = Arc::clone(&global_config);
                let val = error_occured.clone();

                tokio::task::spawn(async move {
                    match Podcast::new(name, config, &global_config, client, &ui).await {
                        Ok(podcast) => podcast.sync(&mut ui).await,
                        Err(e) => {
                            ui.error(&e);
                            val.store(true, Ordering::SeqCst);
                            vec![]
                        }
                    }
                })
            })
            .collect::<Vec<_>>();

        let paths: Vec<PathBuf> = future::join_all(futures)
            .await
            .into_iter()
            .filter_map(Result::ok)
            .flatten()
            .collect();

        if let Some(p) = global_config.log().path() {
            if true || error_occured.load(Ordering::SeqCst) {
                utils::create_dir(p);
                let log_name = log_file.file_name().unwrap();
                let new_path = p.join(log_name);
                fs::rename(log_file, new_path).unwrap();
            }
        }

        paths
    }

    pub fn load() -> Self {
        let Ok(config_str) = fs::read_to_string(&Self::path()) else {
            eprintln!("error: failed to read podcasts.toml file");
            process::exit(1);
        };

        match toml::from_str(&config_str) {
            Ok(s) => Self(s),
            Err(e) => {
                eprintln!("failed to deserialize podcasts.toml file\n{:?}", e);
                process::exit(1);
            }
        }
    }

    fn into_inner(self) -> HashMap<String, PodcastConfig> {
        self.0
    }

    pub fn filter(mut self, filter: Option<Regex>) -> Self {
        self.0.retain(|name, _| match filter {
            Some(ref filter) => filter.is_match(&name),
            None => true,
        });

        self
    }

    pub fn assert_not_empty(self) -> Self {
        if self.is_empty() {
            eprintln!("No podcasts configured!");
            eprintln!("You can add podcasts with the following methods:\n");
            eprintln!("* \"{} --search <name of podcast>\"", crate::APPNAME);
            eprintln!(
                "* \"{} --add <feed url>  <name of podcast>\"",
                crate::APPNAME
            );
            eprintln!(
                "*  Manually configuring the {:?} file.",
                &PodcastConfigs::path()
            );
            process::exit(1);
        }

        self
    }

    pub fn longest_name(&self) -> Option<usize> {
        self.0.iter().map(|(name, _)| name.chars().count()).max()
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

        let str = toml::to_string(&self).expect("failed to serialize podcastconfigs");
        let path = Self::path();

        if let Err(e) = File::create(&path).map(|mut file| file.write_all(str.as_bytes())) {
            eprintln!("failed to save podcast configs to file: {:?}", e);
            process::exit(1);
        };
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
            std::fs::File::create(&path).expect("failed to create podcasts.toml file");
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
    pub url: String,
    name_pattern: Option<String>,
    id_pattern: Option<String>,
    #[serde(alias = "path")]
    download_path: Option<String>,
    partial_path: Option<String>,
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
            partial_path: Default::default(),
        }
    }

    /// Changes the `earliest_date` setting to the current time.
    ///
    /// This means only episodes published after this function was called will be downloaded.
    /// Remember to save after calling this function.
    pub fn catch_up(&mut self) -> bool {
        use chrono::DateTime;

        let unix = utils::current_unix();
        let current_date = DateTime::from_timestamp(unix.as_secs() as i64, 0)
            .expect("failed to convert unix to datetime")
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        if self.backlog_start.is_some() || self.backlog_interval.is_some() {
            return false;
        }

        self.earliest_date = ConfigOption::Enabled(current_date.clone());

        true
    }
}

#[derive(Serialize, Default, Deserialize, Debug, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct LogConfig {
    path: Option<PathBuf>,
    level: Option<log::LevelFilter>,
    third_party: Option<bool>,
}

impl LogConfig {
    pub fn level(&self) -> log::LevelFilter {
        self.level.unwrap_or(log::LevelFilter::Trace)
    }
    pub fn third_party(&self) -> bool {
        self.third_party.unwrap_or(true)
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn is_default(&self) -> bool {
        self == &Self::default()
    }
}
