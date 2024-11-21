use crate::config::DownloadMode;
use crate::config::EvalData;
use crate::config::PodcastConfig;
use crate::config::{Config, GlobalConfig};
use crate::display::DownloadBar;
use crate::episode;
use crate::episode::Episode;
use crate::episode::RawEpisode;
use crate::tags;
use crate::utils;
use quickxml_to_serde::{xml_string_to_json, Config as XmlConfig};
use serde_json::Map;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

fn get_inner_channel(xml: String) -> Option<serde_json::Value> {
    let conf = XmlConfig::new_with_defaults();
    std::mem::take(
        xml_string_to_json(xml, &conf)
            .ok()?
            .get_mut("rss")?
            .get_mut("channel")?,
    )
    .into()
}

/// Converts the podcast's xml string to serde values of the channel and the episodes.
///
/// The library will merge different namespaces together, which is why we manually change
/// the itunes namespace, and then after converting it, we change it back. Preserving itunes:XXX as
/// separate keys.
fn xml_to_value(xml: &str, ui: &DownloadBar) -> Option<(RawPodcast, Vec<RawEpisode>)> {
    ui.log_info("converting xml to serde values");
    let placeholder = "__placeholder__";
    let replacement = format!("itunes{}", placeholder);
    let xml = xml.replace("itunes:", &replacement);
    let mut val = match get_inner_channel(xml) {
        Some(val) => val,
        None => {
            ui.log_error("failed to find rss/channel xml tags");
            return None;
        }
    };

    // Create a new map to store the transformed keys at the top level
    let mut new_map: Map<String, Value> = Map::new();

    if let Some(obj) = val.as_object() {
        for (key, value) in obj {
            let new_key = key.replace(&replacement, "itunes:");
            new_map.insert(new_key, value.clone());
        }
    }

    let podcast = RawPodcast::new(new_map);

    let items = std::mem::take(val.as_object_mut()?.get_mut("item")?.as_array_mut()?);

    let episodes = items
        .iter()
        .map(|item| {
            let mut new_item_map: Map<String, Value> = Map::new();
            for (key, val) in item.as_object().expect("unexpected serde type").iter() {
                let new_key = key.replace(&replacement, "itunes:");
                new_item_map.insert(new_key, val.clone());
            }
            RawEpisode::new(new_item_map)
        })
        .collect::<Vec<RawEpisode>>();

    Some((podcast, episodes))
}

#[derive(Debug)]
pub struct RawPodcast(Map<String, serde_json::Value>);

impl RawPodcast {
    pub fn new(raw: serde_json::Map<String, serde_json::Value>) -> Self {
        Self(raw)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        utils::val_to_str(self.0.get(key)?)
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
        match self.0.get(key).and_then(|x| x.as_array()) {
            Some(v) => v.iter().filter_map(utils::val_to_str).collect(),
            None => vec![],
        }
    }

    pub fn copyright(&self) -> Option<&str> {
        let inner = self.0.get("copyright")?;
        utils::val_to_str(&inner)
    }

    pub fn language(&self) -> Option<&str> {
        self.get_str("language")
    }

    pub fn image(&self) -> Option<&str> {
        let inner = self.0.get("image")?;
        utils::val_to_url(inner)
    }
}

#[derive(Debug)]
pub struct Podcast {
    episodes: Vec<Episode>,
    client: Arc<reqwest::Client>,
    mode: DownloadMode,
}

impl Podcast {
    pub async fn new(
        name: String,
        config: PodcastConfig,
        global_config: &GlobalConfig,
        client: Arc<reqwest::Client>,
        ui: &DownloadBar,
    ) -> Result<Podcast, String> {
        ui.fetching();
        ui.log_info("downloading podcast info...");
        let Some(xml_string) = utils::download_text(&client, &config.url, ui).await else {
            return Err("failed to download xml-file".into());
        };

        let Some((raw_podcast, raw_episodes)) = xml_to_value(&xml_string, ui) else {
            return Err("failed to parse xml".into());
        };

        let episode_attrs = {
            let mut attrs = vec![];

            for episode in raw_episodes {
                ui.log_trace("parsing attributes from raw episode");
                match episode::Attributes::new(episode) {
                    Ok(attr) => attrs.push(attr),
                    Err(e) => {
                        ui.log_debug(e);
                    }
                }
            }

            attrs.sort_by_key(|attr| attr.published());
            attrs
        };

        let mut episodes = vec![];
        for (index, attr) in episode_attrs.into_iter().enumerate() {
            let tags = tags::extract_tags_from_raw(&raw_podcast, &attr, ui).await;
            let config = {
                let data = EvalData::new(&name, &raw_podcast, &attr);
                Config::new(global_config, &config, data)
            };

            let url = attr
                .image()
                .ok()
                .or(raw_podcast.image())
                .map(ToString::to_string);

            let episode = Episode::new(attr, index, config, tags, url);
            episodes.push(episode);
        }

        let mode = DownloadMode::new(global_config, &config);

        Ok(Podcast {
            episodes,
            client,
            mode,
        })
    }

    pub async fn sync(self, ui: &mut DownloadBar) -> Vec<PathBuf> {
        ui.init();
        ui.log_info("syncing...");

        let episodes = self.pending_episodes();
        let mut downloaded = vec![];

        for (index, episode) in episodes.iter().enumerate() {
            ui.begin_download(&episode, index, episodes.len());

            match episode.download(&self.client, ui).await {
                Ok(downloaded_episode) => downloaded.push(downloaded_episode),
                Err(e) => {
                    ui.error(&e);
                    break;
                }
            };
        }

        let mut paths = vec![];

        ui.hook_status();
        for mut episode in downloaded {
            episode.await_handle(ui).await;
            paths.push(episode.into_path());
        }

        ui.complete();
        paths
    }

    fn pending_episodes(&self) -> Vec<&Episode> {
        let qty = self.episodes.len();

        let mut pending: Vec<&Episode> = self
            .episodes
            .iter()
            .filter(|episode| episode.within_age_limits(&self.mode, qty))
            .collect();
        
        // In backlog mode it makes more sense to download earliest episode first.
        // in standard mode, the most recent episodes are more relevant.
        match self.mode {
            
            DownloadMode::Backlog { start: _, interval: _,max_episodes} => {
                pending.sort_by_key(|ep| ep.index);
                if max_episodes.is_some() { 
                    pending.truncate(max_episodes.unwrap() as usize);
                }
            }

            DownloadMode::Standard { max_time: _, earliest_date: _, max_episodes } => {
                pending.sort_by_key(|ep| ep.index);
                pending.reverse();
                if max_episodes.is_some() { 
                    pending.truncate(max_episodes.unwrap() as usize);
                }
            }
        }

        pending
    }
}
