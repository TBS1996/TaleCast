use crate::config::DownloadMode;
use crate::config::{Config, GlobalConfig, PodcastConfigs};
use crate::display::DownloadBar;
use crate::episode::DownloadedEpisode;
use crate::episode::Episode;
use crate::tags;
use crate::utils;
use futures::future;
use indicatif::MultiProgress;
use quickxml_to_serde::{xml_string_to_json, Config as XmlConfig};
use serde_json::Map;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

/// Converts the podcast's xml string to a [`serde_json::Value`].
///
/// The library will merge different namespaces together, which is why we manually change
/// the itunes namespace, and then after converting it, we change it back. Preserving itunes:XXX as
/// separate keys.
fn xml_to_value(xml: &str) -> Option<(Value, Vec<Map<String, Value>>)> {
    let placeholder = "__placeholder__";
    let replacement = format!("itunes{}", placeholder);
    let xml = xml.replace("itunes:", &replacement);
    let conf = XmlConfig::new_with_defaults();
    let mut val = std::mem::take(
        xml_string_to_json(xml.to_string(), &conf)
            .ok()?
            .get_mut("rss")?
            .get_mut("channel")?,
    );

    // Create a new map to store the transformed keys at the top level
    let mut new_map: Map<String, Value> = Map::new();

    if let Some(obj) = val.as_object() {
        for (key, value) in obj {
            let new_key = key.replace(&replacement, "itunes:");
            new_map.insert(new_key, value.clone());
        }
    }

    let items = std::mem::take(val.as_object_mut()?.get_mut("item")?.as_array_mut()?);

    let items = items
        .iter()
        .map(|item| {
            let mut new_item_map: Map<String, Value> = Map::new();
            for (key, val) in item.as_object().unwrap().iter() {
                let new_key = key.replace(&replacement, "itunes:");
                new_item_map.insert(new_key, val.clone());
            }
            new_item_map
        })
        .collect::<Vec<Map<String, Value>>>();

    Some((Value::Object(new_map), items))
}

pub struct Podcasts {
    mp: MultiProgress,
    podcasts: Vec<PodcastEntry>,
    client: Arc<reqwest::Client>,
    global_config: Arc<GlobalConfig>,
}

impl Podcasts {
    pub fn new(global_config: GlobalConfig) -> Self {
        let mp = MultiProgress::new();
        let global_config = Arc::new(global_config);

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
                let config = Arc::clone(&self.global_config);

                tokio::task::spawn(async move {
                    let podcast = match podcast.fetch(client, &ui, &config).await {
                        Ok(s) => s,
                        Err(e) => {
                            ui.error(&e);
                            return vec![];
                        }
                    };

                    podcast.sync(&ui).await
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
    config: PodcastConfig,
}

use crate::config::PodcastConfig;

impl PodcastEntry {
    pub fn new(name: String, config: PodcastConfig) -> Self {
        Self { name, config }
    }

    pub async fn fetch(
        self,
        client: Arc<reqwest::Client>,
        ui: &DownloadBar,
        global_config: &GlobalConfig,
    ) -> Result<Podcast, String> {
        ui.fetching();
        let Some(xml_string) = utils::download_text(&client, &self.config.url, ui).await else {
            return Err("failed to download xml-file".to_string());
        };

        let Some((channel, items)) = xml_to_value(&xml_string) else {
            return Err("failed to parse xml".to_string());
        };

        let mut podcast = Podcast {
            name: self.name,
            xml: channel,
            episodes: vec![],
            client,
            mode: DownloadMode::new(global_config, &self.config),
        };

        let mut episodes = vec![];

        for item in items {
            if let Some(mut ep) = Episode::new(item) {
                let config = Config::new(global_config, &self.config, &podcast, &ep);
                ep.config = config;
                episodes.push(ep);
            }
        }

        episodes.sort_by_key(|episode| episode.published);

        let mut index = 0;
        for episode in &mut episodes {
            episode.index = index;
            index += 1;
        }

        podcast.episodes = episodes;

        Ok(podcast)
    }
}

#[derive(Debug)]
pub struct Podcast {
    name: String, // The configured name in `podcasts.toml`.
    xml: serde_json::Value,
    episodes: Vec<Episode>,
    client: Arc<reqwest::Client>,
    mode: DownloadMode,
}

impl Podcast {
    pub fn name(&self) -> &str {
        &self.name
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

    pub async fn download_episode<'a>(
        &'a self,
        episode: &'a Episode,
        ui: &DownloadBar,
    ) -> Result<DownloadedEpisode<'a>, String> {
        let mut episode = episode.download(&self.client, ui).await;
        self.set_mp3_tags(&mut episode).await?;
        episode.process().await?;
        episode.run_download_hook();
        episode.mark_downloaded();
        Ok(episode)
    }

    pub async fn sync(self, ui: &DownloadBar) -> Vec<PathBuf> {
        ui.init();

        let episodes = self.pending_episodes();
        let mut downloaded = vec![];

        for (index, episode) in episodes.iter().enumerate() {
            ui.begin_download(&episode, index, episodes.len());
            match self.download_episode(episode, ui).await {
                Ok(downloaded_episode) => downloaded.push(downloaded_episode),
                Err(e) => {
                    ui.error(&e);
                    break;
                }
            };
        }

        let mut paths = vec![];

        ui.hook_status();
        for episode in &mut downloaded {
            episode.await_handle().await;
            paths.push(episode.path().to_path_buf());
        }

        ui.complete();
        paths
    }

    pub fn get_text_attribute(&self, key: &str) -> Option<&str> {
        utils::val_to_str(self.xml.get(key)?)
    }

    fn pending_episodes(&self) -> Vec<&Episode> {
        let qty = self.episodes.len();

        let mut pending: Vec<&Episode> = self
            .episodes
            .iter()
            .filter(|episode| episode.should_download(&self.mode, qty))
            .collect();

        // In backlog mode it makes more sense to download earliest episode first.
        // in standard mode, the most recent episodes are more relevant.
        match self.mode {
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

    async fn set_mp3_tags(&self, episode: &mut DownloadedEpisode<'_>) -> Result<(), String> {
        if episode.path().extension().is_some_and(|ext| ext == "mp3") {
            tags::set_mp3_tags(&self, episode).await;
        };

        Ok(())
    }
}
