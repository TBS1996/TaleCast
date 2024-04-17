use crate::config::DownloadMode;
use crate::config::{Config, GlobalConfig, PodcastConfigs};
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
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use quickxml_to_serde::{xml_string_to_json, Config as XmlConfig};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Write as IOWrite;
use std::path::Path;
use std::path::PathBuf;

fn xml_to_value(xml: &str) -> Value {
    let xml = utils::remove_xml_namespaces(&xml, NAMESPACE_ALTER);
    let conf = XmlConfig::new_with_defaults();
    xml_string_to_json(xml, &conf).unwrap()
}

pub struct Podcasts(Vec<Podcast>);

impl Podcasts {
    pub async fn new(global_config: GlobalConfig, configs: PodcastConfigs) -> Self {
        if configs.is_empty() {
            eprintln!("No podcasts configured!");
            eprintln!("Add podcasts with \"{} --add 'url' 'name'\" or by manually configuring the {:?} file.", crate::APPNAME, &PodcastConfigs::path());
            std::process::exit(1);
        }

        let mut podcasts = vec![];
        eprintln!("fetching podcasts...");
        for (name, config) in configs.0 {
            let config = Config::new(&global_config, config);
            let podcast = Podcast::new(name, config);
            podcasts.push(podcast);
        }

        let mut podcasts = futures::future::join_all(podcasts).await;

        podcasts.sort_by_key(|pod| pod.name.clone());
        Self(podcasts)
    }

    pub fn set_progress_bars(mut self, multi_progress: Option<&MultiProgress>) -> Self {
        let Some(mp) = multi_progress else {
            return self;
        };

        for podcast in &mut self.0 {
            let progress_bar = mp.add(ProgressBar::new_spinner());
            podcast.progress_bar = Some(progress_bar);
        }

        self
    }

    pub fn longest_name(&self) -> usize {
        match self
            .0
            .iter()
            .map(|podcast| podcast.name.chars().count())
            .max()
        {
            Some(len) => len,
            None => {
                eprintln!("no podcasts configured");
                std::process::exit(1);
            }
        }
    }

    pub async fn sync(self) -> Vec<PathBuf> {
        let longest_name = self.longest_name();
        eprintln!("syncing {} podcasts", &self.0.len());

        let futures = self
            .0
            .into_iter()
            .map(|podcast| tokio::task::spawn(async move { podcast.sync(longest_name).await }))
            .collect::<Vec<_>>();

        let episodes: Vec<PathBuf> = future::join_all(futures)
            .await
            .into_iter()
            .filter_map(Result::ok)
            .flatten()
            .collect();

        episodes
    }

    pub fn into_outlines(self) -> Vec<opml::Outline> {
        self.0
            .into_iter()
            .map(|pod| opml::Outline {
                text: pod.name().to_owned(),
                r#type: Some("rss".to_string()),
                xml_url: Some(pod.config().url.clone()),
                title: Some(pod.name().to_owned()),
                ..opml::Outline::default()
            })
            .collect()
    }
}

impl From<Podcasts> for opml::OPML {
    fn from(podcasts: Podcasts) -> opml::OPML {
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

#[derive(Debug)]
pub struct Podcast {
    name: String, // The configured name in `podcasts.toml`.
    channel: rss::Channel,
    xml: serde_json::Value,
    config: Config,
    progress_bar: Option<ProgressBar>,
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

    async fn new(name: String, config: Config) -> Self {
        let xml_string = utils::download_text(&config.url).await;
        let channel = rss::Channel::read_from(xml_string.as_bytes()).unwrap();
        let xml = xml_to_value(&xml_string);

        Self {
            name,
            channel,
            xml,
            config,
            progress_bar: None,
        }
    }

    pub fn get_text_attribute(&self, key: &str) -> Option<&str> {
        let rss = self.xml.get("rss").unwrap();
        let channel = rss.get("channel").unwrap();
        channel.get(key).unwrap().as_str()
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
                let days_passed = (utils::current_unix() - start.as_secs() as i64) / 86400;
                let current_backlog_index = days_passed / interval;

                current_backlog_index >= episode.index as i64
            }

            DownloadMode::Standard {
                max_days,
                max_episodes,
                earliest_date,
            } => {
                let max_days_exceeded = || {
                    max_days.is_some_and(|max_days| {
                        (utils::current_unix() - episode.published) > max_days as i64 * 86400
                    })
                };

                let max_episodes_exceeded = || {
                    max_episodes.is_some_and(|max_episodes| {
                        (latest_episode - max_episodes as usize) > episode.index
                    })
                };

                let episode_too_old = || {
                    earliest_date.as_ref().is_some_and(|date| {
                        dateparser::parse(date).unwrap().timestamp() > episode.published
                    })
                };

                !max_days_exceeded() && !max_episodes_exceeded() && !episode_too_old()
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
        // in standard mode, the most recent episodes are seen as more relevant.
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

    fn show_download_bar(&self) {
        if let Some(pb) = &self.progress_bar {
            let template = self.config().download_template();
            pb.set_style(ProgressStyle::default_bar().template(&template).unwrap());
            pb.enable_steady_tick(self.config().spinner_speed());
        }
    }

    fn show_download_info(
        &self,
        episode: &Episode,
        index: usize,
        longest_podcast_name: usize,
        episode_qty: usize,
    ) {
        if let Some(pb) = &self.progress_bar {
            let fitted_episode_title = {
                let title_length = self.config().title_length();
                let padded = &format!("{:<width$}", episode.title, width = title_length);
                utils::truncate_string(padded, title_length, true)
            };

            let msg = format!(
                "{:<podcast_width$} {}/{} {} ",
                &self.name,
                index + 1,
                episode_qty,
                &fitted_episode_title,
                podcast_width = longest_podcast_name + 3
            );

            pb.set_message(msg);
            pb.set_position(0);
        }
    }

    fn set_template(&self, style: &str) {
        if let Some(pb) = &self.progress_bar {
            pb.set_style(ProgressStyle::default_bar().template(style).unwrap());
        }
    }

    pub async fn download_episode<'a>(&self, episode: Episode<'a>) -> DownloadedEpisode<'a> {
        let partial_path = {
            let file_name = sanitize_filename::sanitize(&episode.guid);
            let file_name = format!("{}.partial", file_name);
            self.download_folder().join(file_name)
        };

        let mut downloaded: u64 = 0;

        let mut file = if partial_path.exists() {
            use std::io::Seek;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .open(&partial_path)
                .unwrap();
            downloaded = file.seek(std::io::SeekFrom::End(0)).unwrap();
            file
        } else {
            std::fs::File::create(&partial_path).unwrap()
        };

        let mut req_builder = reqwest::Client::builder()
            .user_agent(&self.config.user_agent)
            .redirect(reqwest::redirect::Policy::default())
            .build()
            .unwrap()
            .get(episode.url);

        if downloaded > 0 {
            let range_header_value = format!("bytes={}-", downloaded);
            req_builder = req_builder.header(reqwest::header::RANGE, range_header_value);
        }

        let response = req_builder.send().await.unwrap();
        let total_size = response.content_length().unwrap_or(0);

        let ext = match PathBuf::from(episode.url)
            .extension()
            .and_then(|ext| ext.to_str().map(String::from))
        {
            Some(ext) => ext.to_string(),
            None => {
                let content_type = response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|ct| ct.to_str().ok())
                    .unwrap_or("application/octet-stream");

                let extensions = mime_guess::get_mime_extensions_str(&content_type).unwrap();

                match extensions.contains(&"mp3") {
                    true => "mp3".to_owned(),
                    false => extensions
                        .first()
                        .expect("extension not found.")
                        .to_string(),
                }
            }
        };

        let ext = ext
            .split_once("?")
            .map(|(l, _)| l.to_string())
            .unwrap_or(ext);

        if let Some(pb) = &self.progress_bar {
            pb.set_length(total_size);
            pb.set_position(downloaded);
        }

        let mut stream = response.bytes_stream();

        while let Some(item) = stream.next().await {
            let chunk = item.unwrap();
            file.write_all(&chunk).unwrap();
            downloaded = std::cmp::min(downloaded + (chunk.len() as u64), total_size);

            if let Some(pb) = &self.progress_bar {
                pb.set_position(downloaded);
            }
        }

        let path = {
            let mut path = partial_path.clone();
            path.set_extension(ext);
            path
        };

        std::fs::rename(partial_path, &path).unwrap();

        DownloadedEpisode::new(episode, path)
    }

    async fn normalize_episode(&self, episode: &mut DownloadedEpisode<'_>) {
        let mp3_tags = if episode.path().extension().unwrap() == "mp3" {
            crate::tags::set_mp3_tags(&self.channel, episode, &self.config.id3_tags).await
        } else {
            id3::Tag::default()
        };

        let datasource = DataSources::default()
            .set_id3(&mp3_tags)
            .set_episode(episode.inner())
            .set_podcast(self);

        let file_name = self.config().name_pattern.evaluate(datasource);

        episode.rename(file_name);
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

    fn mark_complete(&self) {
        if let Some(pb) = &self.progress_bar {
            let template = self.config().completion_template();
            self.set_template(&template);
            pb.finish_with_message(self.name.clone());
        }
    }

    pub async fn sync(&self, longest_podcast_name: usize) -> Vec<PathBuf> {
        self.show_download_bar();

        let episodes = self.pending_episodes();
        let episode_qty = episodes.len();

        let mut downloaded = vec![];
        let mut hook_handles = vec![];

        for (index, episode) in episodes.into_iter().enumerate() {
            self.show_download_info(&episode, index, longest_podcast_name, episode_qty);
            let mut downloaded_episode = self.download_episode(episode).await;
            self.normalize_episode(&mut downloaded_episode).await;
            hook_handles.extend(self.run_download_hook(&downloaded_episode));
            self.mark_downloaded(&downloaded_episode);
            downloaded.push(downloaded_episode);
        }

        if !hook_handles.is_empty() {
            let template = self.config().hook_template();
            self.set_template(&template);
            futures::future::join_all(hook_handles).await;
        }

        self.mark_complete();
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
            utils::current_unix(),
            episode.as_ref().title
        )
        .unwrap();
    }
}
