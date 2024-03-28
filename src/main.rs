use crate::config::{CombinedConfig, GlobalConfig, PodcastConfig};
use anyhow::Result;
use config::DownloadMode;
use futures_util::StreamExt;
use id3::TagLike;
use indicatif::MultiProgress;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use std::collections::HashMap;
use std::io::Write as IoWrite;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

mod config;

pub type Unix = i64;

#[tokio::main]
async fn main() -> Result<()> {
    let global_config = Arc::new(GlobalConfig::load()?);

    eprintln!("Checking for new episodes...");
    let mp = MultiProgress::new();
    let mut futures = vec![];

    let mut podcasts = Podcast::load_all(global_config)?;
    podcasts.sort_by_key(|pod| pod.name.clone());

    // Longest podcast name is used for formatting.
    let Some(longest_name) = podcasts
        .iter()
        .map(|podcast| podcast.name.chars().count())
        .max()
    else {
        eprintln!("no podcasts configured");
        std::process::exit(1);
    };

    for podcast in podcasts {
        let pb = mp.add(ProgressBar::new_spinner());
        pb.set_style(ProgressStyle::default_spinner().template("{spinner}  {msg}")?);
        pb.set_message(podcast.name.clone());
        pb.enable_steady_tick(std::time::Duration::from_millis(100));

        let future = tokio::task::spawn(async move { podcast.sync(pb, longest_name).await });

        futures.push(future);
    }

    let mut episodes_downloaded = 0;
    for future in futures {
        episodes_downloaded += future.await??;
    }

    eprintln!("Syncing complete!");
    eprintln!("{} episodes downloaded.", episodes_downloaded);

    Ok(())
}

fn truncate_string(s: &str, max_width: usize) -> String {
    let mut width = 0;
    let mut truncated = String::new();

    for c in s.chars() {
        let mut buf = [0; 4];
        let encoded_char = c.encode_utf8(&mut buf);
        let char_width = unicode_width::UnicodeWidthStr::width(encoded_char);
        if width + char_width > max_width {
            break;
        }
        truncated.push(c);
        width += char_width;
    }

    truncated
}

fn podcasts_path() -> Result<PathBuf> {
    Ok(dirs::config_dir()
        .ok_or(anyhow::Error::msg("no config dir found"))?
        .join("cringecast")
        .join("podcasts.toml"))
}

fn current_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

#[derive(Debug, Clone)]
struct Episode {
    title: String,
    url: String,
    guid: String,
    published: i64,
    index: usize,
    _inner: rss::Item,
}

impl Episode {
    fn new(item: rss::Item, index: usize) -> Option<Self> {
        Some(Self {
            title: item.title()?.to_owned(),
            url: item.enclosure()?.url().to_owned(),
            guid: item.guid()?.value().to_string(),
            published: chrono::DateTime::parse_from_rfc2822(item.pub_date()?)
                .ok()?
                .timestamp(),
            index,
            _inner: item,
        })
    }

    async fn download(&self, folder: &Path, pb: &ProgressBar) -> Result<PathBuf> {
        let response = Client::new().get(&self.url).send().await?;
        let total_size = response.content_length().unwrap_or(0);

        pb.set_length(total_size);

        let path = {
            let file_name = self.title.replace(" ", "_") + ".mp3";
            folder.join(file_name)
        };

        let mut file = std::fs::File::create(&path)?;
        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();

        while let Some(item) = stream.next().await {
            let chunk = item?;
            file.write_all(&chunk)?;
            let new = std::cmp::min(downloaded + (chunk.len() as u64), total_size);
            pb.set_position(new);
            downloaded = new;
        }

        Ok(path)
    }
}

#[derive(Debug)]
struct Podcast {
    name: String,
    config: CombinedConfig,
    downloaded: DownloadedEpisodes,
}

impl Podcast {
    fn load_all(global_config: Arc<GlobalConfig>) -> Result<Vec<Self>> {
        let configs: HashMap<String, PodcastConfig> = {
            let path = podcasts_path()?;
            if !path.exists() {
                eprintln!("You need to create a 'podcasts.toml' file to get started");
                std::process::exit(1);
            }
            let config_str = std::fs::read_to_string(path)?;
            toml::from_str(&config_str)?
        };

        let mut podcasts = vec![];
        for (name, config) in configs {
            let config = CombinedConfig::new(Arc::clone(&global_config), config);
            let downloaded = DownloadedEpisodes::load(&name, &config)?;

            podcasts.push(Self {
                name,
                config,
                downloaded,
            });
        }

        Ok(podcasts)
    }

    async fn load_episodes(&self) -> Result<Vec<Episode>> {
        let response = reqwest::Client::new()
            .get(self.config.url())
            .header(
                "User-Agent",
                "Mozilla/5.0 (X11; Linux x86_64; rv:124.0) Gecko/20100101 Firefox/124.0",
            )
            .send()
            .await?;

        if response.status().is_success() {
            let data = response.bytes().await?;

            let mut items = rss::Channel::read_from(&data[..])?.into_items();
            items.sort_by_key(|item| {
                chrono::DateTime::parse_from_rfc2822(item.pub_date().unwrap_or_default())
                    .map(|x| x.timestamp())
                    .unwrap_or_default()
            });

            Ok(items
                .into_iter()
                .enumerate()
                .filter_map(|(index, item)| Episode::new(item, index))
                .collect())
        } else {
            Err(anyhow::anyhow!(
                "Failed to download RSS feed: HTTP {}",
                response.status()
            ))
        }
    }

    fn download_folder(&self) -> Result<PathBuf> {
        let destination_folder = self.config.base_path().join(&self.name);
        std::fs::create_dir_all(&destination_folder)?;
        Ok(destination_folder)
    }

    fn should_download(&self, episode: &Episode, latest_episode: usize) -> bool {
        if self.downloaded.contains_episode(episode) {
            return false;
        };

        if let DownloadMode::Backlog { start, interval } = self.config.mode() {
            let days_passed = (current_unix() - start) / 86400;
            let current_backlog_index = days_passed / interval;
            if current_backlog_index < episode.index as i64 {
                return false;
            }
        }

        if self
            .config
            .max_days()
            .is_some_and(|max_days| (current_unix() - episode.published) > max_days as i64 * 86400)
        {
            return false;
        };

        if self
            .config
            .max_episodes()
            .is_some_and(|max_episodes| (latest_episode - max_episodes as usize) > episode.index)
        {
            return false;
        };

        if self.config.earliest_date().is_some_and(|date| {
            chrono::DateTime::parse_from_rfc3339(date)
                .unwrap()
                .timestamp()
                > episode.published
        }) {
            return false;
        }

        true
    }

    fn mark_downloaded(&self, episode: &Episode) -> Result<()> {
        DownloadedEpisodes::append(&self.name, &self.config, &episode)?;
        Ok(())
    }

    async fn sync(&self, pb: ProgressBar, longest_podcast_name: usize) -> Result<usize> {
        let mut episodes = self.load_episodes().await?;
        let episode_qty = episodes.len();

        episodes = episodes
            .into_iter()
            .filter(|episode| self.should_download(episode, episode_qty))
            .collect();

        // In backlog mode it makes more sense to download earliest episode first.
        // in standard mode, the most recent episodes are seen as more relevant.
        match self.config.mode() {
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
        for (index, episode) in episodes.iter().enumerate() {
            let fitted_episode_title = {
                let title_length = 30;
                let padded = &format!("{:<width$}", &episode.title, width = title_length);
                truncate_string(padded, title_length)
            };

            let msg = format!(
                "{:<podcast_width$} {}/{} {}",
                &self.name,
                index + 1,
                episodes.len(),
                &fitted_episode_title,
                podcast_width = longest_podcast_name + 3
            );

            pb.set_message(msg);
            pb.set_position(0);

            let file_path = episode.download(&download_folder, &pb).await?;

            let mut tags = id3::Tag::read_from_path(&file_path)?;
            for (id, value) in self.config.custom_tags() {
                tags.set_text(id, value);
            }

            if tags.artist().is_none() {
                if let Some(author) = episode._inner.author() {
                    tags.set_artist(author);
                }
            }

            tags.write_to_path(&file_path, id3::Version::Id3v24)?;
            self.mark_downloaded(&episode)?;

            if let Some(script_path) = self.config.download_hook() {
                std::process::Command::new(script_path)
                    .arg(&file_path)
                    .output()?;
            }
        }

        pb.set_style(ProgressStyle::default_bar().template("{msg}")?);
        pb.finish_with_message(format!("✅ {}", &self.name));

        Ok(episodes.len())
    }
}

/// Keeps track of which episodes have already been downloaded.
#[derive(Debug, Default)]
struct DownloadedEpisodes(HashMap<String, Unix>);

impl DownloadedEpisodes {
    fn contains_episode(&self, episode: &Episode) -> bool {
        self.0.contains_key(&episode.guid)
    }

    fn load(name: &str, config: &CombinedConfig) -> Result<Self> {
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

                hashmap.insert(id, timestamp);
            }
        }

        Ok(Self(hashmap))
    }

    fn append(name: &str, config: &CombinedConfig, episode: &Episode) -> Result<()> {
        let path = Self::file_path(config, name);

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)?;

        writeln!(
            file,
            "{} {} \"{}\"",
            &episode.guid,
            current_unix(),
            &episode.title
        )?;
        Ok(())
    }

    fn file_path(config: &CombinedConfig, pod_name: &str) -> PathBuf {
        config.base_path().join(pod_name).join(".downloaded")
    }
}
