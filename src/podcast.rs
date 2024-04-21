use crate::config::DownloadMode;
use crate::config::{Config, GlobalConfig, PodcastConfigs};
use crate::display::DownloadBar;
use crate::download_tracker::DownloadedEpisodes;
use crate::episode::DownloadedEpisode;
use crate::episode::Episode;
use crate::patterns::Evaluate;
use crate::tags;
use crate::utils;
use futures::future;
use futures_util::StreamExt;
use indicatif::MultiProgress;
use io::Seek;
use quickxml_to_serde::{xml_string_to_json, Config as XmlConfig};
use serde_json::Map;
use serde_json::Value;
use std::fs;
use std::io;
use std::io::Write as IOWrite;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;

/// Converts the podcast's xml string to a [`serde_json::Value`].
///
/// The library will merge different namespaces together, which is why we manually change
/// the itunes namespace, and then after converting it, we change it back. Preserving itunes:XXX as
/// separate keys.
fn xml_to_value(xml: &str) -> Value {
    let placeholder = "__placeholder__";
    let replacement = format!("itunes{}", placeholder);
    let xml = xml.replace("itunes:", &replacement);
    let conf = XmlConfig::new_with_defaults();
    let val = xml_string_to_json(xml.to_string(), &conf)
        .unwrap()
        .get("rss")
        .unwrap()
        .get("channel")
        .unwrap()
        .clone();

    // Create a new map to store the transformed keys at the top level
    let mut new_map: Map<String, Value> = Map::new();

    if let Some(obj) = val.as_object() {
        for (key, value) in obj {
            let new_key = key.replace(&replacement, "itunes:");
            new_map.insert(new_key, value.clone());
        }
    }

    let items = val
        .as_object()
        .unwrap()
        .get("item")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            let mut new_item_map: Map<String, Value> = Map::new();
            for (key, val) in item.as_object().unwrap().iter() {
                let new_key = key.replace(&replacement, "itunes:");
                new_item_map.insert(new_key, val.clone());
            }
            Value::Object(new_item_map)
        })
        .collect::<Vec<Value>>();

    new_map.insert("items".to_string(), Value::Array(items));

    Value::Object(new_map)
}

pub struct Podcasts {
    mp: MultiProgress,
    podcasts: Vec<PodcastEntry>,
    client: Arc<reqwest::Client>,
    global_config: GlobalConfig,
}

impl Podcasts {
    pub fn new(global_config: GlobalConfig) -> Self {
        let mp = MultiProgress::new();

        let client = reqwest::Client::builder()
            .user_agent(&global_config.user_agent())
            .build()
            .map(Arc::new)
            .unwrap();

        let podcasts = vec![];

        Self {
            mp,
            client,
            podcasts,
            global_config,
        }
    }
    pub async fn add(mut self, configs: PodcastConfigs) -> Self {
        let mut podcasts = vec![];

        for (name, config) in configs {
            let config = Config::new(&self.global_config, config);
            let podcast = PodcastEntry::new(name, config);
            podcasts.push(podcast);
        }

        self.podcasts.extend(podcasts);
        self.podcasts.sort_by_key(|pod| pod.name.clone());

        self
    }

    fn longest_name(&self) -> Option<usize> {
        self.podcasts
            .iter()
            .map(|pod| pod.name.chars().count())
            .max()
    }

    pub async fn sync(self) -> Vec<PathBuf> {
        eprintln!("syncing {} podcasts", &self.podcasts.len());

        let Some(longest_name) = self.longest_name() else {
            return vec![];
        };

        let futures = self
            .podcasts
            .into_iter()
            .map(|podcast| {
                let client = Arc::clone(&self.client);
                let ui = DownloadBar::new(
                    podcast.name.clone(),
                    self.global_config.style(),
                    &self.mp,
                    longest_name,
                );

                tokio::task::spawn(async move {
                    podcast.fetch(&client, &ui).await.sync(&client, &ui).await
                })
            })
            .collect::<Vec<_>>();

        future::join_all(futures)
            .await
            .into_iter()
            .filter_map(Result::ok)
            .flatten()
            .collect()
    }
}

#[derive(Debug)]
pub struct PodcastEntry {
    name: String,
    config: Config,
}

impl PodcastEntry {
    pub fn new(name: String, config: Config) -> Self {
        Self { name, config }
    }

    pub async fn fetch(self, client: &reqwest::Client, ui: &DownloadBar) -> Podcast {
        ui.fetching();
        let xml_string = utils::download_text(&client, &self.config.url, ui).await;
        let mut xml = xml_to_value(&xml_string);
        let mut items = std::mem::take(xml.get_mut("item").unwrap().as_array_mut().unwrap());

        let mut episodes = vec![];

        for item in items.iter_mut() {
            let item = std::mem::take(item.as_object_mut().unwrap());
            if let Some(ep) = Episode::new(item) {
                episodes.push(ep);
            }
        }

        episodes.sort_by_key(|episode| episode.published);

        let mut index = 0;
        for episode in &mut episodes {
            episode.index = index;
            index += 1;
        }

        Podcast {
            name: self.name,
            xml,
            config: self.config,
            episodes,
        }
    }
}

#[derive(Debug)]
pub struct Podcast {
    name: String, // The configured name in `podcasts.toml`.
    xml: serde_json::Value,
    config: Config,
    episodes: Vec<Episode>,
}

impl Podcast {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    fn get_str<'a>(&'a self, key: &str) -> Option<&'a str> {
        let inner = self.xml.get(key)?;
        utils::val_to_str(inner)
    }

    pub fn title(&self) -> &str {
        self.get_str("title").unwrap()
    }

    pub fn author(&self) -> Option<&str> {
        let key = "itunes:author";
        self.get_str(&key)
    }

    pub fn categories(&self) -> Vec<&str> {
        let key = "itunes:category";
        match self.xml.get(&key).and_then(|x| x.as_array()) {
            Some(v) => v.iter().filter_map(utils::val_to_str).collect(),
            None => vec![],
        }
    }

    pub fn copyright(&self) -> Option<&str> {
        let inner = self.xml.get("copyright")?;
        utils::val_to_str(&inner)
    }

    pub fn language(&self) -> Option<&str> {
        self.get_str("language")
    }

    pub fn image(&self) -> Option<&str> {
        let inner = self.xml.get("image")?;
        utils::val_to_url(inner)
    }

    pub async fn sync(self, client: &reqwest::Client, ui: &DownloadBar) -> Vec<PathBuf> {
        ui.init();

        let episodes = self.pending_episodes();
        let episode_qty = episodes.len();

        let mut downloaded = vec![];
        let mut hook_handles = vec![];

        for (index, episode) in episodes.into_iter().enumerate() {
            ui.begin_download(&episode, index, episode_qty);
            let mut downloaded_episode = self.download_episode(&client, &ui, episode).await;
            self.process_episode(&mut downloaded_episode).await;
            hook_handles.extend(self.run_download_hook(&downloaded_episode));
            self.mark_downloaded(&downloaded_episode);
            downloaded.push(downloaded_episode);
        }

        if !hook_handles.is_empty() {
            ui.hook_status();
            futures::future::join_all(hook_handles).await;
        }

        ui.complete();
        downloaded
            .into_iter()
            .map(|episode| episode.path().to_owned())
            .collect()
    }

    fn download_path(&self, episode: &Episode) -> PathBuf {
        let evaluated = self.config.download_path.evaluate(self, episode);
        let path = PathBuf::from(evaluated);
        utils::create_dir(&path);
        path.join(episode.partial_name())
    }

    fn partial_path(&self, episode: &Episode) -> PathBuf {
        match self.config().partial_path.as_ref() {
            Some(p) => {
                let p = p.evaluate(self, episode);
                let path = PathBuf::from(p);
                utils::create_dir(&path);
                path.join(episode.partial_name())
            }
            None => self.download_path(episode),
        }
    }

    pub fn get_text_attribute(&self, key: &str) -> Option<&str> {
        let rss = self.xml.get("rss").unwrap();
        let channel = rss.get("channel").unwrap();
        channel.get(key)?.as_str()
    }

    fn is_episode_downloaded(&self, episode: &Episode) -> bool {
        let id = self.get_id(episode);
        let path = self.tracker_path(episode);
        let downloaded = DownloadedEpisodes::load(&path);
        downloaded.contains_episode(&id)
    }

    fn should_download(&self, episode: &Episode) -> bool {
        if self.is_episode_downloaded(episode) {
            return false;
        };

        match &self.config.mode {
            DownloadMode::Backlog { start, interval } => {
                let time_passed = utils::current_unix() - *start;
                let intervals_passed = time_passed.as_secs() / interval.as_secs();
                intervals_passed >= episode.index as u64
            }

            DownloadMode::Standard {
                max_time,
                max_episodes,
                earliest_date,
            } => {
                let max_time_exceeded = max_time.map_or(false, |max_time| {
                    (utils::current_unix() - episode.published) > max_time
                });

                let max_episodes_exceeded = max_episodes.map_or(false, |max_episodes| {
                    (self.episodes.len() - max_episodes as usize) > episode.index
                });

                let episode_too_old = earliest_date.map_or(false, |date| date > episode.published);

                !max_time_exceeded && !max_episodes_exceeded && !episode_too_old
            }
        }
    }

    fn mark_downloaded(&self, episode: &DownloadedEpisode) {
        let id = self.get_id(episode.inner());
        let path = self.tracker_path(episode.inner());
        DownloadedEpisodes::append(&path, &id, &episode);
    }

    fn get_id(&self, episode: &Episode) -> String {
        self.config
            .id_pattern
            .evaluate(self, episode)
            .replace(" ", "_")
    }

    fn tracker_path(&self, episode: &Episode) -> PathBuf {
        let path = self.config().tracker_path.evaluate(self, episode);
        PathBuf::from(&path)
    }

    fn pending_episodes(&self) -> Vec<&Episode> {
        let mut pending: Vec<&Episode> = self
            .episodes
            .iter()
            .filter(|episode| self.should_download(episode))
            .collect();

        // In backlog mode it makes more sense to download earliest episode first.
        // in standard mode, the most recent episodes are more relevant.
        match self.config.mode {
            DownloadMode::Backlog { .. } => {
                pending.sort_by_key(|ep| ep.index);
            }
            DownloadMode::Standard { .. } => {
                pending.sort_by_key(|ep| ep.index);
                pending.reverse();
            }
        }

        pending
    }

    pub async fn download_episode<'a>(
        &self,
        client: &reqwest::Client,
        ui: &DownloadBar,
        episode: &'a Episode,
    ) -> DownloadedEpisode<'a> {
        let partial_path = self.partial_path(&episode);

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&partial_path)
            .unwrap();

        let mut downloaded = file.seek(io::SeekFrom::End(0)).unwrap();

        let response = client
            .get(&episode.url)
            .header(reqwest::header::RANGE, format!("bytes={}-", downloaded))
            .send()
            .await;

        let response = utils::handle_response(response);

        let total_size = response.content_length().unwrap_or(0);
        let extension = utils::get_extension_from_response(&response, &episode);

        ui.init_download_bar(downloaded, total_size);

        let mut stream = response.bytes_stream();

        while let Some(item) = stream.next().await {
            let chunk = item.unwrap();
            file.write_all(&chunk).unwrap();
            downloaded = std::cmp::min(downloaded + (chunk.len() as u64), total_size);
            ui.set_progress(downloaded);
        }

        let path = {
            let mut path = self.download_path(&episode).clone();
            path.set_extension(extension);
            path
        };

        std::fs::rename(partial_path, &path).unwrap();

        DownloadedEpisode::new(episode, path)
    }

    async fn process_episode(&self, episode: &mut DownloadedEpisode<'_>) {
        if episode.path().extension().unwrap() == "mp3" {
            tags::set_mp3_tags(&self, episode, &self.config.id3_tags).await;
        };

        let file_name = self.config().name_pattern.evaluate(self, episode.inner());
        let symlink_path = self
            .config()
            .symlink
            .clone()
            .map(|path| path.evaluate(self, episode.inner()))
            .map(PathBuf::from);

        episode.rename(file_name);

        if let Some(symlink_path) = symlink_path {
            let new_path = symlink_path.join(episode.file_name());
            if episode.path() == new_path {
                eprintln!("error: symlink points to itself: {:?}", new_path);
                process::exit(1);
            }
            let _ = std::fs::create_dir_all(&symlink_path);
            if !symlink_path.is_dir() {
                eprintln!(
                    "error: symlink path is not a directory: {:?}",
                    &symlink_path
                );
                process::exit(1);
            }

            std::os::unix::fs::symlink(episode.path(), new_path).unwrap();
        }
    }

    fn run_download_hook(
        &self,
        episode: &DownloadedEpisode,
    ) -> Option<tokio::task::JoinHandle<()>> {
        let script_path = self.config.download_hook.clone()?;
        let path = episode.path().to_owned();

        let handle = tokio::task::spawn_blocking(move || {
            std::process::Command::new(script_path)
                .arg(path)
                .output()
                .unwrap();
        });

        Some(handle)
    }
}
