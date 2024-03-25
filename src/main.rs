use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use std::collections::{HashMap, HashSet};

fn main() {
    let global_config = Arc::new(GlobalConfig::load());
    let podcasts = Podcast::load_all();

    let mut handles = vec![];
    for podcast in podcasts {
        let config = Arc::clone(&global_config);

        handles.push(std::thread::spawn(move || {
            podcast.sync(config);
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap())
}

fn config_dir() -> PathBuf {
    let p = home().join(".config").join("cringecast");
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn channel_path() -> PathBuf {
    config_dir().join("channels.toml")
}

#[derive(Clone)]
struct CombinedConfig<'a> {
    global: Arc<GlobalConfig>,
    specific: &'a PodcastConfig,
}

impl<'a> CombinedConfig<'a> {
    fn new(global: Arc<GlobalConfig>, specific: &'a PodcastConfig) -> Self {
        Self { global, specific }
    }

    fn max_age(&self) -> Option<u32> {
        self.specific.max_age.or(self.global.max_age)
    }

    fn base_path(&self) -> PathBuf {
        PathBuf::from(self.specific.path.as_ref().unwrap_or(&self.global.path))
    }
}

#[derive(Deserialize, Debug, Clone)]
struct PodcastConfig {
    url: String,
    max_age: Option<u32>,
    path: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct GlobalConfig {
    max_age: Option<u32>,
    path: String,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            max_age: Some(120),
            path: home().join("cringecast").to_string_lossy().to_string(),
        }
    }
}

impl GlobalConfig {
    fn load() -> Self {
        let p = config_dir().join("config.toml");

        if !p.exists() {
            let default = Self::default();
            let s = toml::to_string_pretty(&default).unwrap();
            let mut f = std::fs::File::create(&p).unwrap();
            f.write_all(s.as_bytes()).unwrap();
        }

        let str = std::fs::read_to_string(p).unwrap();
        toml::from_str(&str).unwrap()
    }
}

struct Episode(rss::Item);

impl Episode {
    fn title(&self) -> &str {
        self.0.title().unwrap()
    }

    fn url(&self) -> &str {
        self.0.enclosure().unwrap().url()
    }

    fn guid(&self) -> &str {
        self.0.guid().unwrap().value()
    }

    fn download(&self, folder: &Path) {
        let url = self.url();
        let title = self.title();
        let file_name = title.replace(" ", "_") + ".mp3";
        let destination = folder.join(file_name);

        let response = ureq::get(&url).call().unwrap();
        let mut reader = response.into_reader();
        let mut file = std::fs::File::create(destination).unwrap();
        std::io::copy(&mut reader, &mut file).unwrap();
    }

    fn should_download(&self, config: &CombinedConfig, downloaded: &DownloadedEpisodes) -> bool {
        if downloaded.contains_episode(&self.0) {
            return false;
        }

        if let Some(max_age) = config.max_age() {
            let pub_date = self.0.pub_date().unwrap();
            let published_unix = chrono::DateTime::parse_from_rfc2822(pub_date)
                .unwrap()
                .timestamp();

            let current_unix = chrono::Utc::now().timestamp();

            if (current_unix - published_unix) > max_age as i64 * 86400 {
                return false;
            }
        }

        true
    }
}

struct Podcast {
    name: String,
    config: PodcastConfig,
}

impl Podcast {
    fn load_all() -> Vec<Self> {
        let path = channel_path();
        let config_str = std::fs::read_to_string(path).expect("Failed to read config file");

        let mut podcasts = vec![];

        let configs: HashMap<String, PodcastConfig> = toml::from_str(&config_str).unwrap();

        for (name, config) in configs {
            podcasts.push(Self { name, config });
        }

        podcasts
    }

    fn combined_config(&self, global_config: Arc<GlobalConfig>) -> CombinedConfig {
        CombinedConfig::new(global_config, &self.config)
    }

    fn load_episodes(&self) -> Vec<Episode> {
        let agent = ureq::builder().build();

        let mut reader = agent.get(&self.config.url).call().unwrap().into_reader();
        let mut data = vec![];

        reader.read_to_end(&mut data).unwrap();
        let channel = rss::Channel::read_from(&data[..]).unwrap();
        let items = channel.into_items();
        let mut episodes = vec![];

        for item in items {
            episodes.push(Episode(item));
        }

        episodes
    }

    fn download_folder(&self, config: &CombinedConfig) -> PathBuf {
        let destination_folder = config.base_path().join(&self.name);
        std::fs::create_dir_all(&destination_folder).unwrap();
        destination_folder
    }

    fn sync(&self, global_config: Arc<GlobalConfig>) {
        println!("Syncing {}", &self.name);

        let config = self.combined_config(global_config);
        let downloaded = DownloadedEpisodes::load(&self.name, &config);
        let download_folder = self.download_folder(&config);

        let episodes: Vec<Episode> = self
            .load_episodes()
            .into_iter()
            .filter(|episode| episode.should_download(&config, &downloaded))
            .collect();

        if episodes.is_empty() {
            println!("nothing to download!");
        } else {
            println!("downloading {} episodes!", episodes.len());

            for episode in episodes {
                println!("downloading {}", episode.title());
                episode.download(download_folder.as_path());
                DownloadedEpisodes::append(&self.name, &config, &episode);
            }
        }

        println!("all done!");
    }
}

/// Keeps track of which episodes have already been downloaded.
#[derive(Default)]
struct DownloadedEpisodes(HashSet<String>);

impl DownloadedEpisodes {
    fn contains_episode(&self, item: &rss::Item) -> bool {
        self.0.contains(item.guid().unwrap().value())
    }

    fn load(name: &str, config: &CombinedConfig) -> Self {
        let path = Self::file_path(&config, name);

        let s = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Self::default();
            }
            Err(_e) => panic!(),
        };

        let set: HashSet<String> = s
            .lines()
            .filter_map(|line| line.split_whitespace().next().map(String::from))
            .collect();

        Self(set)
    }

    fn append(name: &str, config: &CombinedConfig, episode: &Episode) {
        let path = Self::file_path(config, name);

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .unwrap();

        let line = Self::format(&episode);

        writeln!(file, "{}", line).unwrap();
    }

    fn file_path(config: &CombinedConfig, pod_name: &str) -> PathBuf {
        config.base_path().join(pod_name).join(".downloaded")
    }

    fn format(episode: &Episode) -> String {
        format!("{} \"{}\"", episode.guid(), episode.title())
    }
}
