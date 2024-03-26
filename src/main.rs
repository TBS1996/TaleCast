use std::io::Write as IoWrite;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;

use crate::config::{CombinedConfig, GlobalConfig, PodcastConfig};

use std::collections::{HashMap, HashSet};

use anyhow::Result;

use indicatif::MultiProgress;

mod config;

fn main() -> Result<()> {
    let global_config = Arc::new(GlobalConfig::load()?);

    println!("Checking for new episodes");
    tokio::runtime::Runtime::new()?.block_on(async {
        let mp = MultiProgress::new();
        let mut futures = vec![];

        for podcast in Podcast::load_all(global_config).unwrap() {
            let pb = mp.add(ProgressBar::new_spinner());
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner} {msg}")
                    .unwrap(),
            );
            pb.set_message(podcast.name.clone());

            let future = tokio::task::spawn(async move {
                podcast.sync(pb).await.unwrap();
            });

            futures.push(future);
        }

        let _results = futures::future::join_all(futures).await;
    });

    println!("Syncing complete!");

    Ok(())
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

struct Episode {
    title: String,
    url: String,
    guid: String,
    published: i64,
}

impl Episode {
    fn new(item: rss::Item) -> Result<Self> {
        Ok(Self {
            title: item
                .title()
                .ok_or(anyhow::Error::msg("title not found"))?
                .to_owned(),
            url: item
                .enclosure()
                .ok_or(anyhow::Error::msg("enclosure not found"))?
                .url()
                .to_owned(),
            guid: item
                .guid()
                .ok_or(anyhow::Error::msg("guid not found"))?
                .value()
                .to_string(),
            published: chrono::DateTime::parse_from_rfc2822(
                item.pub_date()
                    .ok_or(anyhow::Error::msg("published date not found"))?,
            )?
            .timestamp(),
        })
    }

    async fn download(&self, folder: &Path, pb: &ProgressBar) -> Result<()> {
        let response = Client::new().get(&self.url).send().await?;
        let total_size = response.content_length().unwrap_or(0);

        pb.set_length(total_size);

        let mut file = {
            let file_name = self.title.replace(" ", "_") + ".mp3";
            let file_path = folder.join(file_name);
            std::fs::File::create(&file_path)?
        };

        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();

        while let Some(item) = stream.next().await {
            let chunk = item?;
            file.write_all(&chunk)?;
            let new = std::cmp::min(downloaded + (chunk.len() as u64), total_size);
            pb.set_position(new);
            downloaded = new;
        }

        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );

        Ok(())
    }
}

struct Podcast {
    name: String,
    config: CombinedConfig,
    downloaded: DownloadedEpisodes,
}

impl Podcast {
    fn load_all(global_config: Arc<GlobalConfig>) -> Result<Vec<Self>> {
        let configs: HashMap<String, PodcastConfig> = {
            let config_str = std::fs::read_to_string(podcasts_path()?)?;
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
        let response = reqwest::get(self.config.url()).await?;

        if response.status().is_success() {
            let data = response.bytes().await?;

            rss::Channel::read_from(&data[..])?
                .into_items()
                .into_iter()
                .map(Episode::new)
                .collect()
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

    fn should_download(&self, episode: &Episode) -> bool {
        if self.downloaded.contains_episode(episode) {
            return false;
        };

        if self
            .config
            .max_age()
            .is_some_and(|max_age| (current_unix() - episode.published) > max_age as i64 * 86400)
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

    async fn sync(&self, pb: ProgressBar) -> Result<()> {
        let namepad = format!("{:<35}", &self.name);
        let episodes: Vec<Episode> = self
            .load_episodes()
            .await?
            .into_iter()
            .filter(|episode| self.should_download(episode))
            .collect();

        if episodes.is_empty() {
            pb.finish_with_message(format!("✅  {}", &self.name));
            return Ok(());
        }

        let download_folder = self.download_folder()?;
        for (index, episode) in episodes.iter().enumerate() {
            let current = index + 1;
            let total = episodes.len();
            {
                pb.set_position(0);
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template("{spinner:.green} {msg} {bar:25.cyan/blue} {bytes}/{total_bytes}")
                        .unwrap(),
                );
                pb.set_message(format!("{} {}/{}", &namepad, current, total));
            }

            episode.download(&download_folder, &pb).await?;
            self.mark_downloaded(&episode)?;
        }

        {
            pb.finish_with_message(format!("✅  {}", &self.name));
        }

        Ok(())
    }
}

/// Keeps track of which episodes have already been downloaded.
#[derive(Default)]
struct DownloadedEpisodes(HashSet<String>);

impl DownloadedEpisodes {
    fn contains_episode(&self, episode: &Episode) -> bool {
        self.0.contains(&episode.guid)
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

        Ok(Self(
            s.lines()
                .filter_map(|line| line.split_whitespace().next().map(String::from))
                .collect(),
        ))
    }

    fn append(name: &str, config: &CombinedConfig, episode: &Episode) -> Result<()> {
        let path = Self::file_path(config, name);

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)?;

        writeln!(file, "{}", Self::format(&episode))?;
        Ok(())
    }

    fn file_path(config: &CombinedConfig, pod_name: &str) -> PathBuf {
        config.base_path().join(pod_name).join(".downloaded")
    }

    fn format(episode: &Episode) -> String {
        format!("{} \"{}\"", &episode.guid, &episode.title)
    }
}
