use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use crate::config::{CombinedConfig, GlobalConfig, PodcastConfig};

use std::collections::{HashMap, HashSet};

use anyhow::Result;

mod config;

fn main() -> Result<()> {
    let global_config = Arc::new(GlobalConfig::load()?);

    for podcast in Podcast::load_all(global_config)? {
        podcast.sync()?;
    }

    println!("Syncing complete!");

    Ok(())
}

fn podcasts_path() -> Result<PathBuf> {
    Ok(dirs::config_dir()
        .ok_or(anyhow::Error::msg("no config dir found"))?
        .join("cringecast")
        .join("podcasts.toml"))
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

    fn download(&self, folder: &Path) -> Result<()> {
        let mut reader = ureq::get(&self.url).call()?.into_reader();

        let mut file = {
            let file_name = self.title.replace(" ", "_") + ".mp3";
            let file_path = folder.join(file_name);
            std::fs::File::create(&file_path)?
        };

        std::io::copy(&mut reader, &mut file)?;
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

    fn load_episodes(&self) -> Result<Vec<Episode>> {
        let data = {
            let mut reader = ureq::agent().get(&self.config.url()).call()?.into_reader();
            let mut data = vec![];
            reader.read_to_end(&mut data)?;
            data
        };

        rss::Channel::read_from(data.as_slice())?
            .into_items()
            .into_iter()
            .map(Episode::new)
            .collect()
    }

    fn download_folder(&self) -> Result<PathBuf> {
        let destination_folder = self.config.base_path().join(&self.name);
        std::fs::create_dir_all(&destination_folder)?;
        Ok(destination_folder)
    }

    fn should_download(&self, episode: &Episode) -> bool {
        !(self.downloaded.contains_episode(episode)
            || self.config.max_age().is_some_and(|max_age| {
                (chrono::Utc::now().timestamp() - episode.published) > max_age as i64 * 86400
            }))
    }

    fn mark_downloaded(&self, episode: &Episode) -> Result<()> {
        DownloadedEpisodes::append(&self.name, &self.config, &episode)?;
        Ok(())
    }

    fn sync(&self) -> Result<()> {
        println!("Syncing {}", &self.name);

        let episodes: Vec<Episode> = self
            .load_episodes()?
            .into_iter()
            .filter(|episode| self.should_download(episode))
            .collect();

        if episodes.is_empty() {
            println!("Nothing to download.");
            return Ok(());
        }

        println!("downloading {} episodes!", episodes.len());

        for episode in episodes {
            println!("downloading: \"{}\"", &episode.title);

            episode.download(self.download_folder()?.as_path())?;
            self.mark_downloaded(&episode)?;
        }

        println!("{} finished syncing.", &self.name);

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
