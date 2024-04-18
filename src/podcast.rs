use crate::config::DownloadMode;
use crate::config::{Config, GlobalConfig, PodcastConfigs};
use crate::display::DownloadBar;
use crate::episode::DownloadedEpisode;
use crate::episode::Episode;
use crate::patterns::DataSources;
use crate::patterns::Evaluate;
use crate::utils;
use crate::utils::Unix;
use crate::utils::NAMESPACE_ALTER;
use futures::future;
use futures_util::StreamExt;
use indicatif::MultiProgress;
use io::Seek;
use quickxml_to_serde::{xml_string_to_json, Config as XmlConfig};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::io::Write as IOWrite;
use std::path::Path;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;

fn xml_to_value(xml: &str) -> Value {
    let xml = utils::remove_xml_namespaces(&xml, NAMESPACE_ALTER);
    let conf = XmlConfig::new_with_defaults();
    xml_string_to_json(xml, &conf).unwrap()
}

pub struct Podcasts {
    _mp: MultiProgress,
    podcasts: Vec<Podcast>,
}

impl Podcasts {
    pub async fn new(global_config: GlobalConfig, configs: PodcastConfigs) -> Self {
        if configs.is_empty() {
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
            std::process::exit(1);
        }

        let mp = MultiProgress::new();

        let client = reqwest::Client::builder()
            .user_agent(&global_config.user_agent())
            .build()
            .map(Arc::new)
            .unwrap();

        let longest_name = configs.longest_name();

        let mut podcasts = vec![];
        eprintln!("fetching podcasts...");
        for (name, config) in configs.0 {
            let config = Config::new(&global_config, config);
            let progress_bar =
                DownloadBar::new(name.clone(), global_config.style(), &mp, longest_name);
            let client = Arc::clone(&client);
            let podcast = Podcast::new(name, config, client, progress_bar);
            podcasts.push(podcast);
        }

        let mut podcasts = futures::future::join_all(podcasts).await;

        podcasts.sort_by_key(|pod| pod.name.clone());

        Self { podcasts, _mp: mp }
    }

    pub async fn sync(self) -> Vec<PathBuf> {
        eprintln!("syncing {} podcasts", &self.podcasts.len());

        let futures = self
            .podcasts
            .into_iter()
            .map(|podcast| tokio::task::spawn(async move { podcast.sync().await }))
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
pub struct Podcast {
    name: String, // The configured name in `podcasts.toml`.
    channel: rss::Channel,
    xml: serde_json::Value,
    config: Config,
    ui: DownloadBar,
    client: Arc<reqwest::Client>,
}

impl Podcast {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    fn download_folder(&self) -> PathBuf {
        let data_sources = DataSources::default().set_podcast(self);
        let evaluated = self.config.download_path.evaluate(data_sources);
        let path = PathBuf::from(evaluated);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    async fn new(
        name: String,
        config: Config,
        client: Arc<reqwest::Client>,
        progress_bar: DownloadBar,
    ) -> Self {
        let xml_string = utils::download_text(&client, &config.url).await;
        let channel = rss::Channel::read_from(xml_string.as_bytes()).unwrap();
        let xml = xml_to_value(&xml_string);

        Self {
            name,
            channel,
            xml,
            ui: progress_bar,
            config,
            client,
        }
    }

    fn episodes(&self) -> Vec<Episode<'_>> {
        let mut vec = vec![];

        let mut map = HashMap::<&str, &serde_json::Map<String, serde_json::Value>>::new();

        let rss = self.xml.get("rss").unwrap();
        let channel = rss.get("channel").unwrap();
        let raw_items = channel
            .get("item")
            .expect("items not found")
            .as_array()
            .unwrap();

        for item in raw_items {
            let item = item.as_object().unwrap();
            let guid = utils::get_guid(item);
            map.insert(guid, item);
        }

        for item in self.channel.items() {
            let Some(guid) = item.guid() else { continue };
            let obj = map.get(guid.value()).unwrap();

            // in case the episodes are not chronological we put all indices as zero and then
            // sort by published date and set index.
            if let Some(episode) = Episode::new(&item, 0, obj) {
                vec.push(episode);
            }
        }

        vec.sort_by_key(|episode| episode.published);

        let mut index = 0;
        for episode in &mut vec {
            episode.index = index;
            index += 1;
        }

        vec
    }

    pub fn get_text_attribute(&self, key: &str) -> Option<&str> {
        let rss = self.xml.get("rss").unwrap();
        let channel = rss.get("channel").unwrap();
        channel.get(key)?.as_str()
    }

    fn should_download(
        &self,
        episode: &Episode,
        latest_episode: usize,
        downloaded: &DownloadedEpisodes,
    ) -> bool {
        let id = self.get_id(episode);

        if downloaded.0.contains_key(&id) {
            return false;
        }

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
                    (latest_episode - max_episodes as usize) > episode.index
                });

                let episode_too_old = earliest_date.map_or(false, |date| date > episode.published);

                !max_time_exceeded && !max_episodes_exceeded && !episode_too_old
            }
        }
    }

    fn mark_downloaded(&self, episode: &DownloadedEpisode) {
        let id = self.get_id(episode.inner());
        let path = self.tracker_path();
        DownloadedEpisodes::append(&path, &id, &episode);
    }

    fn get_id(&self, episode: &Episode) -> String {
        let data_sources = DataSources::default()
            .set_podcast(self)
            .set_episode(episode);

        self.config
            .id_pattern
            .evaluate(data_sources)
            .replace(" ", "_")
    }

    fn tracker_path(&self) -> PathBuf {
        let source = DataSources::default().set_podcast(self);
        let path = self.config().tracker_path.evaluate(source);
        PathBuf::from(&path)
    }

    fn pending_episodes(&self) -> Vec<Episode<'_>> {
        let mut episodes = self.episodes();
        let episode_qty = episodes.len();
        let path = self.tracker_path();
        let downloaded = DownloadedEpisodes::load(&path);

        episodes = episodes
            .into_iter()
            .filter(|episode| self.should_download(episode, episode_qty, &downloaded))
            .collect();

        // In backlog mode it makes more sense to download earliest episode first.
        // in standard mode, the most recent episodes are more relevant.
        match self.config.mode {
            DownloadMode::Backlog { .. } => {
                episodes.sort_by_key(|ep| ep.index);
            }
            DownloadMode::Standard { .. } => {
                episodes.sort_by_key(|ep| ep.index);
                episodes.reverse();
            }
        }

        episodes
    }

    pub async fn download_episode<'a>(&self, episode: Episode<'a>) -> DownloadedEpisode<'a> {
        let partial_path = self.download_folder().join(episode.partial_name());

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&partial_path)
            .unwrap();

        let mut downloaded = file.seek(io::SeekFrom::End(0)).unwrap();

        let response = self
            .client
            .get(episode.url)
            .header(reqwest::header::RANGE, format!("bytes={}-", downloaded))
            .send()
            .await;

        let response = utils::handle_response(response);

        let total_size = response.content_length().unwrap_or(0);
        let extension = utils::get_extension_from_response(&response, &episode.url);

        self.ui.init_download_bar(downloaded, total_size);

        let mut stream = response.bytes_stream();

        while let Some(item) = stream.next().await {
            let chunk = item.unwrap();
            file.write_all(&chunk).unwrap();
            downloaded = std::cmp::min(downloaded + (chunk.len() as u64), total_size);
            self.ui.set_progress(downloaded);
        }

        let path = {
            let mut path = partial_path.clone();
            path.set_extension(extension);
            path
        };

        std::fs::rename(partial_path, &path).unwrap();

        DownloadedEpisode::new(episode, path)
    }

    async fn process_episode(&self, episode: &mut DownloadedEpisode<'_>) {
        let mp3_tags = if episode.path().extension().unwrap() == "mp3" {
            crate::tags::set_mp3_tags(&self.channel, episode, &self.config.id3_tags).await
        } else {
            id3::Tag::default()
        };

        let datasources = DataSources::default()
            .set_id3(&mp3_tags)
            .set_episode(episode.inner())
            .set_podcast(self);

        let file_name = self.config().name_pattern.evaluate(datasources);
        let symlink_path = self
            .config()
            .symlink
            .clone()
            .map(|path| path.evaluate(datasources))
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

    pub async fn sync(&self) -> Vec<PathBuf> {
        self.ui.init();

        let episodes = self.pending_episodes();
        let episode_qty = episodes.len();

        let mut downloaded = vec![];
        let mut hook_handles = vec![];

        for (index, episode) in episodes.into_iter().enumerate() {
            self.ui.begin_download(&episode, index, episode_qty);
            let mut downloaded_episode = self.download_episode(episode).await;
            self.process_episode(&mut downloaded_episode).await;
            hook_handles.extend(self.run_download_hook(&downloaded_episode));
            self.mark_downloaded(&downloaded_episode);
            downloaded.push(downloaded_episode);
        }

        if !hook_handles.is_empty() {
            self.ui.hook_status();
            futures::future::join_all(hook_handles).await;
        }

        self.ui.complete();
        downloaded
            .into_iter()
            .map(|episode| episode.path().to_owned())
            .collect()
    }
}

/// Keeps track of which episodes have already been downloaded.
#[derive(Debug, Default)]
struct DownloadedEpisodes(HashMap<String, Unix>);

impl DownloadedEpisodes {
    fn load(path: &Path) -> Self {
        let s = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Self::default();
            }
            e @ Err(_) => e.unwrap(),
        };

        let mut hashmap: HashMap<String, Unix> = HashMap::new();

        for line in s.trim().lines() {
            let mut parts = line.split_whitespace();
            if let (Some(id), Some(timestamp_str)) = (parts.next(), parts.next()) {
                let id = id.to_string();
                let timestamp = timestamp_str
                    .parse::<i64>()
                    .expect("Timestamp should be a valid i64");
                let timestamp = std::time::Duration::from_secs(timestamp as u64);

                hashmap.insert(id, timestamp);
            }
        }

        Self(hashmap)
    }

    fn append(path: &Path, id: &str, episode: &DownloadedEpisode) {
        use std::io::Write;

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .unwrap();

        writeln!(
            file,
            "{} {} \"{}\"",
            id,
            utils::current_unix().as_secs(),
            episode.as_ref().title
        )
        .unwrap();
    }
}
