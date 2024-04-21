use crate::config::DownloadMode;
use crate::config::EvaluatedConfig;
use crate::display::DownloadBar;
use crate::download_tracker::DownloadedEpisodes;
use crate::utils;
use futures_util::StreamExt;
use std::fs;
use std::io::Seek;
use std::io::Write as IOWrite;
use std::path::Path;
use std::path::PathBuf;
use std::time;
use tokio::task::JoinHandle;

#[derive(Debug, Clone)]
pub struct Episode {
    pub title: String,
    pub config: EvaluatedConfig,
    pub url: String,
    pub mime: Option<String>,
    pub guid: String,
    pub published: time::Duration,
    pub index: usize,
    pub raw: serde_json::Map<String, serde_json::Value>,
}

impl Episode {
    pub fn new(raw: serde_json::Map<String, serde_json::Value>) -> Option<Self> {
        let title = raw.get("title")?.as_str()?.to_string();
        let enclosure = raw.get("enclosure")?;
        let url = enclosure
            .get("@url")
            .and_then(|x| Some(x.as_str()?.to_string()))?;

        let mime = enclosure
            .get("@type")
            .and_then(|x| Some(x.as_str()?.to_string()));
        let published = utils::date_str_to_unix(raw.get("pubDate")?.as_str()?);
        let guid = utils::val_to_str(raw.get("guid")?)?.to_string();
        let index = 0;
        let config = Default::default();

        Self {
            title,
            config,
            url,
            guid,
            published,
            index,
            raw,
            mime,
        }
        .into()
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        let inner = self.raw.get(key)?;
        utils::val_to_str(inner)
    }

    pub fn image(&self) -> Option<&str> {
        let key = "itunes:image";
        utils::val_to_url(self.raw.get(key)?)
    }

    pub fn is_downloaded(&self) -> bool {
        let id = self.get_id();
        let path = self.tracker_path();
        let downloaded = DownloadedEpisodes::load(&path);
        downloaded.contains_episode(&id)
    }

    pub fn author(&self) -> Option<&str> {
        self.get_str("author")
    }

    pub fn description(&self) -> Option<&str> {
        self.get_str("description")
    }

    pub fn itunes_episode(&self) -> Option<&str> {
        let key = "itunes:episode";
        self.get_str(&key)
    }

    pub fn should_download(&self, episode_qty: usize) -> bool {
        if self.is_downloaded() {
            return false;
        };

        match &self.config.mode {
            DownloadMode::Backlog { start, interval } => {
                let time_passed = utils::current_unix() - *start;
                let intervals_passed = time_passed.as_secs() / interval.as_secs();
                intervals_passed >= self.index as u64
            }

            DownloadMode::Standard {
                max_time,
                max_episodes,
                earliest_date,
            } => {
                let max_time_exceeded = max_time.map_or(false, |max_time| {
                    (utils::current_unix() - self.published) > max_time
                });

                let max_episodes_exceeded = max_episodes.map_or(false, |max_episodes| {
                    (episode_qty - max_episodes as usize) > self.index
                });

                let episode_too_old = earliest_date.map_or(false, |date| date > self.published);

                !max_time_exceeded && !max_episodes_exceeded && !episode_too_old
            }
        }
    }

    pub fn itunes_duration(&self) -> Option<&str> {
        let key = "itunes:duration";
        self.get_str(&key)
    }

    /// Filename of episode when it's being downloaded.
    pub fn partial_name(&self) -> String {
        let file_name = sanitize_filename::sanitize(&self.guid);
        format!("{}.partial", file_name)
    }

    pub fn get_id(&self) -> String {
        self.config.id_pattern.replace(" ", "_")
    }

    pub fn tracker_path(&self) -> &Path {
        self.config.tracker_path.as_path()
    }

    pub async fn download<'a>(
        &'a self,
        client: &reqwest::Client,
        ui: &DownloadBar,
    ) -> DownloadedEpisode<'a> {
        let config = &self.config;

        let partial_path = config
            .partial_path
            .clone()
            .unwrap_or_else(|| config.download_path.clone())
            .join(self.partial_name());

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&partial_path)
            .unwrap();

        let mut downloaded = file.seek(std::io::SeekFrom::End(0)).unwrap();

        let response = client
            .get(&self.url)
            .header(reqwest::header::RANGE, format!("bytes={}-", downloaded))
            .send()
            .await;

        let response = utils::handle_response(response);

        let total_size = response.content_length().unwrap_or(0);
        let extension = utils::get_extension_from_response(&response, &self);

        ui.init_download_bar(downloaded, total_size);

        let mut stream = response.bytes_stream();

        while let Some(item) = stream.next().await {
            let chunk = item.unwrap();
            file.write_all(&chunk).unwrap();
            downloaded = std::cmp::min(downloaded + (chunk.len() as u64), total_size);
            ui.set_progress(downloaded);
        }

        let path = {
            let mut path = config.download_path.to_path_buf();
            path.set_extension(extension);
            path
        };

        std::fs::rename(partial_path, &path).unwrap();

        DownloadedEpisode::new(self, path)
    }
}

pub struct DownloadedEpisode<'a> {
    inner: &'a Episode,
    path: PathBuf,
    handle: Option<JoinHandle<()>>,
}

impl<'a> DownloadedEpisode<'a> {
    pub fn new(inner: &'a Episode, path: PathBuf) -> DownloadedEpisode<'a> {
        Self {
            inner,
            path,
            handle: None,
        }
    }

    pub fn mark_downloaded(&self) {
        let id = self.inner.config.id_pattern.replace(" ", "_");
        let path = self.inner.config.tracker_path.as_path();
        DownloadedEpisodes::append(&path, &id, self);
    }

    pub fn inner(&self) -> &Episode {
        &self.inner
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn file_name(&self) -> &str {
        self.path.file_name().unwrap().to_str().unwrap()
    }

    pub async fn await_handle(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }
    }

    pub fn run_download_hook(&mut self) {
        let Some(script_path) = self.inner.config.download_hook.clone() else {
            return;
        };

        let path = self.path().to_owned();

        let handle = tokio::task::spawn_blocking(move || {
            std::process::Command::new(script_path)
                .arg(path)
                .output()
                .unwrap();
        });

        self.handle = Some(handle);
    }

    pub async fn process(&mut self) -> Result<(), String> {
        let config = &self.inner.config;
        self.rename(config.name_pattern.clone());

        if let Some(symlink_path) = config.symlink.as_ref() {
            let new_path = symlink_path.join(self.file_name());
            if self.path() == new_path {
                return Err(format!("symlink points to itself"));
            }

            let _ = std::fs::create_dir_all(&symlink_path);
            if !symlink_path.is_dir() {
                return Err("configured symlink path is not a directory".to_string());
            }

            std::os::unix::fs::symlink(self.path(), new_path).unwrap();
        }

        Ok(())
    }

    pub fn rename(&mut self, new_name: String) {
        let new_name = sanitize_filename::sanitize(&new_name);

        let new_path = match self.path.extension() {
            Some(extension) => {
                let mut new_path = self.path.with_file_name(new_name);
                new_path.set_extension(extension);
                new_path
            }
            None => self.path.with_file_name(new_name),
        };

        std::fs::rename(&self.path, &new_path).unwrap();
        self.path = new_path;
    }
}

impl AsRef<Episode> for DownloadedEpisode<'_> {
    fn as_ref(&self) -> &Episode {
        &self.inner
    }
}
