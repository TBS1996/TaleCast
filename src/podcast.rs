use crate::config::{Config, GlobalConfig, PodcastConfig};

use crate::config::DownloadMode;
use crate::episode::Episode;
use crate::utils::current_unix;
use crate::utils::get_guid;
use crate::utils::remove_xml_namespaces;
use crate::utils::truncate_string;
use crate::utils::Unix;
use crate::utils::NAMESPACE_ALTER;
use anyhow::Result;
use id3::TagLike;
use indicatif::{ProgressBar, ProgressStyle};
use quickxml_to_serde::{xml_string_to_json, Config as XmlConfig};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

fn xml_to_value(xml: &str) -> Value {
    let xml = remove_xml_namespaces(&xml, NAMESPACE_ALTER);
    let conf = XmlConfig::new_with_defaults();
    xml_string_to_json(xml, &conf).unwrap()
}

#[derive(Debug)]
pub struct Podcast {
    name: String,
    channel: rss::Channel,
    xml: serde_json::Value,
    config: Config,
    downloaded: DownloadedEpisodes,
}

impl Podcast {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub async fn load_all(global_config: &GlobalConfig) -> Result<Vec<Self>> {
        let configs: HashMap<String, PodcastConfig> = {
            let path = crate::utils::podcasts_toml()?;
            if !path.exists() {
                eprintln!("You need to create a 'podcasts.toml' file to get started");
                std::process::exit(1);
            }
            let config_str = std::fs::read_to_string(path)?;
            toml::from_str(&config_str)?
        };

        let mut podcasts = vec![];
        for (name, config) in configs {
            let config = Config::new(&global_config, config);
            let downloaded = DownloadedEpisodes::load(&name, &config)?;
            let xml_string = Self::load_xml(&config.url).await?;
            let channel = rss::Channel::read_from(xml_string.as_bytes())?;
            let xml_value = xml_to_value(&xml_string);

            podcasts.push(Self {
                name,
                channel,
                xml: xml_value,
                config,
                downloaded,
            });
        }

        Ok(podcasts)
    }

    fn get_text_attribute(&self, key: &str) -> Option<&str> {
        let rss = self.xml.get("rss").unwrap();
        let channel = rss.get("channel").unwrap();
        channel.get(key)?.as_str()
    }

    fn episodes(&self) -> Vec<Episode<'_>> {
        let mut vec = vec![];

        let mut map = HashMap::<&str, &serde_json::Map<String, serde_json::Value>>::new();

        let rss = self.xml.get("rss").unwrap();
        let channel = rss.get("channel").unwrap();
        let raw_items = channel
            .get("item")
            .expect("items not found?")
            .as_array()
            .unwrap();

        for item in raw_items {
            let item = item.as_object().unwrap();
            let guid = get_guid(item);
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

    fn rename_file(&self, file: &Path, tags: Option<&id3::Tag>, episode: &Episode) -> PathBuf {
        let pattern = &self.config.name_pattern;
        let result = self.evaluate_pattern(pattern, tags, episode);

        let new_name = match file.extension() {
            Some(extension) => {
                let mut new_path = file.with_file_name(result);
                new_path.set_extension(extension);
                new_path
            }
            None => file.with_file_name(result),
        };

        std::fs::rename(file, &new_name).unwrap();
        new_name
    }

    fn evaluate_pattern(
        &self,
        pattern: &str,
        tags: Option<&id3::Tag>,
        episode: &Episode,
    ) -> String {
        let null = "<value not found>";
        let re = regex::Regex::new(r"\{([^\}]+)\}").unwrap();

        let mut result = String::new();
        let mut last_end = 0;

        use chrono::TimeZone;
        let datetime = chrono::Utc.timestamp_opt(episode.published, 0).unwrap();

        for cap in re.captures_iter(&pattern) {
            let match_range = cap.get(0).unwrap().range();
            let key = &cap[1];

            result.push_str(&pattern[last_end..match_range.start]);

            let replacement = match key {
                date if date.starts_with("pubdate::") => {
                    let (_, format) = date.split_once("::").unwrap();
                    if format == "unix" {
                        episode.published.to_string()
                    } else {
                        datetime.format(format).to_string()
                    }
                }
                id3 if id3.starts_with("id3::") && tags.is_some() => {
                    let (_, tag) = id3.split_once("::").unwrap();
                    tags.unwrap()
                        .get(tag)
                        .and_then(|tag| tag.content().text())
                        .unwrap_or(null)
                        .to_string()
                }
                rss if rss.starts_with("rss::episode::") => {
                    let (_, key) = rss.split_once("episode::").unwrap();

                    let key = key.replace(":", NAMESPACE_ALTER);
                    episode.get_text_value(&key).unwrap_or(null).to_string()
                }
                rss if rss.starts_with("rss::channel::") => {
                    let (_, key) = rss.split_once("channel::").unwrap();

                    let key = key.replace(":", NAMESPACE_ALTER);
                    self.get_text_attribute(&key).unwrap_or(null).to_string()
                }

                "guid" => episode.guid.to_string(),
                "url" => episode.url.to_string(),

                invalid_tag => {
                    eprintln!("invalid tag configured: {}", invalid_tag);
                    std::process::exit(1);
                }
            };

            result.push_str(&replacement);

            last_end = match_range.end;
        }

        result.push_str(&pattern[last_end..]);
        result
    }

    async fn load_xml(url: &str) -> Result<String> {
        let response = reqwest::Client::new()
            .get(url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (X11; Linux x86_64; rv:124.0) Gecko/20100101 Firefox/124.0",
            )
            .send()
            .await?;

        if response.status().is_success() {
            let xml = response.text().await?;

            Ok(xml)
        } else {
            Err(anyhow::anyhow!(
                "Failed to download RSS feed: HTTP {}",
                response.status()
            ))
        }
    }

    fn download_folder(&self) -> Result<PathBuf> {
        let destination_folder = self.config.download_path.join(&self.name);
        std::fs::create_dir_all(&destination_folder)?;
        Ok(destination_folder)
    }

    fn should_download(&self, episode: &Episode, latest_episode: usize) -> bool {
        if self.is_downloaded(episode) {
            return false;
        };

        match &self.config.mode {
            DownloadMode::Backlog { start, interval } => {
                let days_passed = (current_unix() - start.as_secs() as i64) / 86400;
                let current_backlog_index = days_passed / interval;

                current_backlog_index >= episode.index as i64
            }

            DownloadMode::Standard {
                max_days,
                max_episodes,
                earliest_date,
            } => {
                let max_days_exceeded = max_days.is_some_and(|max_days| {
                    (current_unix() - episode.published) > max_days as i64 * 86400
                });

                if max_days_exceeded {
                    return false;
                }

                let max_episodes_exceeded = max_episodes.is_some_and(|max_episodes| {
                    (latest_episode - max_episodes as usize) > episode.index
                });

                if max_episodes_exceeded {
                    return false;
                }

                let episode_too_old = earliest_date.as_ref().is_some_and(|date| {
                    chrono::DateTime::parse_from_rfc3339(&date)
                        .unwrap()
                        .timestamp()
                        > episode.published
                });

                if episode_too_old {
                    return false;
                }

                true
            }
        }
    }

    fn mark_downloaded(&self, episode: &Episode) -> Result<()> {
        let id = self.get_id(episode);
        DownloadedEpisodes::append(&self.name, &id, &self.config, &episode)?;
        Ok(())
    }

    fn get_id(&self, episode: &Episode) -> String {
        let id_pattern = &self.config.id_pattern;
        self.evaluate_pattern(id_pattern, None, episode)
            .replace(" ", "_")
    }

    fn is_downloaded(&self, episode: &Episode) -> bool {
        let id = self.get_id(episode);
        self.downloaded.0.contains_key(&id)
    }

    pub async fn sync(&self, pb: ProgressBar, longest_podcast_name: usize) -> Result<Vec<PathBuf>> {
        let mut episodes = self.episodes();
        let episode_qty = episodes.len();

        episodes = episodes
            .into_iter()
            .filter(|episode| self.should_download(episode, episode_qty))
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

        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} {msg} {bar:15.cyan/blue} {bytes}/{total_bytes}")?,
        );

        let download_folder = self.download_folder()?;
        let mut file_paths = vec![];
        let mut hook_handles = vec![];
        for (index, episode) in episodes.iter().enumerate() {
            let fitted_episode_title = {
                let title_length = 30;
                let padded = &format!("{:<width$}", episode.title, width = title_length);
                truncate_string(padded, title_length)
            };

            let msg = format!(
                "{:<podcast_width$} {}/{} {} ",
                &self.name,
                index + 1,
                episodes.len(),
                &fitted_episode_title,
                podcast_width = longest_podcast_name + 3
            );

            pb.set_message(msg);
            pb.set_position(0);

            let file_path = episode.download(&download_folder, &pb).await?;

            let mp3_tags = (file_path.extension().unwrap() == "mp3").then_some(
                crate::tags::set_mp3_tags(
                    &self.channel,
                    &episode,
                    &file_path,
                    &self.config.id3_tags,
                )
                .await?,
            );

            let file_path = self.rename_file(&file_path, mp3_tags.as_ref(), episode);

            self.mark_downloaded(episode)?;
            file_paths.push(file_path.clone());

            if let Some(script_path) = self.config.download_hook.clone() {
                let handle = tokio::task::spawn_blocking(move || {
                    std::process::Command::new(script_path)
                        .arg(&file_path)
                        .output()
                });
                hook_handles.push(handle);
            }
        }

        if !hook_handles.is_empty() {
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} finishing up download hooks...")?,
            );
            futures::future::join_all(hook_handles).await;
        }

        pb.set_style(ProgressStyle::default_bar().template("{msg}")?);
        pb.finish_with_message(format!("✅ {}", &self.name));

        Ok(file_paths)
    }
}

/// Keeps track of which episodes have already been downloaded.
#[derive(Debug, Default)]
struct DownloadedEpisodes(HashMap<String, Unix>);

impl DownloadedEpisodes {
    fn load(name: &str, config: &Config) -> Result<Self> {
        let path = Self::file_path(config, name);

        let s = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            e @ Err(_) => e?,
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

        Ok(Self(hashmap))
    }

    fn append(name: &str, id: &str, config: &Config, episode: &Episode) -> Result<()> {
        use std::io::Write;
        let path = Self::file_path(config, name);

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)?;

        writeln!(file, "{} {} \"{}\"", id, current_unix(), &episode.title)?;
        Ok(())
    }

    fn file_path(config: &Config, pod_name: &str) -> PathBuf {
        config.download_path.join(pod_name).join(".downloaded")
    }
}
