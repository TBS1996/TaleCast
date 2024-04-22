use crate::config::Config;
use crate::config::DownloadMode;
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

pub trait XmlWrapper {
    fn inner(&self) -> &serde_json::Map<String, serde_json::Value>;

    fn get_str(&self, key: &str) -> Option<&str> {
        utils::val_to_str(self.inner().get(key)?)
    }

    fn get_url(&self, key: &str) -> Option<&str> {
        utils::val_to_url(self.inner().get(key)?)
    }

    fn get_val(&self, key: &str) -> Option<&serde_json::Value> {
        self.inner().get(key)
    }

    fn get_string(&self, key: &str) -> Option<String> {
        self.get_str(key).map(str::to_owned)
    }
}

impl XmlWrapper for RawEpisode {
    fn inner(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct RawEpisode(serde_json::Map<String, serde_json::Value>);

impl RawEpisode {
    pub fn new(val: serde_json::Map<String, serde_json::Value>) -> Self {
        Self(val)
    }
}

#[derive(Debug, Clone)]
pub struct EpisodeAttributes {
    pub title: String,
    pub url: String,
    pub mime: Option<String>,
    pub guid: String,
    pub published: time::Duration,
    pub raw: RawEpisode,
}

impl EpisodeAttributes {
    pub fn new(raw: RawEpisode) -> Option<Self> {
        let title = raw.get_string("title")?;
        let enclosure = raw.get_val("enclosure")?;
        let url = enclosure
            .get("@url")
            .and_then(|x| Some(x.as_str()?.to_string()))?;

        let mime = enclosure
            .get("@type")
            .and_then(|x| Some(x.as_str()?.to_string()));
        let published = raw.get_str("pubDate")?;
        let published = utils::date_str_to_unix(published)?;
        let guid = raw.get_string("guid")?;

        Some(Self {
            title,
            url,
            mime,
            guid,
            published,
            raw,
        })
    }

    pub fn published(&self) -> time::Duration {
        self.published
    }

    pub fn _mime(&self) -> Option<&str> {
        self.mime.as_deref()
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn guid(&self) -> &str {
        &self.guid
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.raw.get_str(key)
    }

    pub fn image(&self) -> Option<&str> {
        let key = "itunes:image";
        self.raw.get_url(key)
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

    pub fn itunes_duration(&self) -> Option<&str> {
        let key = "itunes:duration";
        self.get_str(&key)
    }
}

#[derive(Debug, Clone)]
pub struct Episode {
    pub config: Config,
    pub tags: Option<id3::Tag>,
    pub index: usize,
    pub attrs: EpisodeAttributes,
    pub image_url: Option<String>,
}

impl Episode {
    pub fn new(
        attrs: EpisodeAttributes,
        index: usize,
        config: Config,
        tags: Option<id3::Tag>,
    ) -> Self {
        Self {
            attrs,
            config,
            tags,
            index,
            image_url: None,
        }
    }

    pub fn is_downloaded(&self) -> bool {
        let id = self.get_id();
        let path = self.tracker_path();
        let downloaded = DownloadedEpisodes::load(&path);
        downloaded.contains_episode(&id)
    }

    pub fn should_download(&self, mode: &DownloadMode, episode_qty: usize) -> bool {
        if self.is_downloaded() {
            return false;
        };

        match mode {
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
                    (utils::current_unix() - self.attrs.published) > max_time
                });

                let max_episodes_exceeded = max_episodes.map_or(false, |max_episodes| {
                    (episode_qty - max_episodes as usize) > self.index
                });

                let episode_too_old =
                    earliest_date.map_or(false, |date| date > self.attrs.published);

                !max_time_exceeded && !max_episodes_exceeded && !episode_too_old
            }
        }
    }

    /// Filename of episode when it's being downloaded.
    pub fn partial_name(&self) -> String {
        let file_name = sanitize_filename::sanitize(&self.attrs.guid);
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
    ) -> Result<DownloadedEpisode<'a>, String> {
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
            .map_err(|_| "failed to write file".to_string())?;

        let mut downloaded = file
            .seek(std::io::SeekFrom::End(0))
            .map_err(|_| "file error".to_string())?;

        let response = client
            .get(self.as_ref().url())
            .header(reqwest::header::RANGE, format!("bytes={}-", downloaded))
            .send()
            .await;

        let response = utils::short_handle_response(response)?;

        let total_size = response.content_length().unwrap_or(0);
        let extension = utils::get_extension_from_response(&response, &self);

        ui.init_download_bar(downloaded, total_size);

        let mut stream = response.bytes_stream();

        while let Some(item) = stream.next().await {
            let chunk = item.map_err(|_| "failed to load chunk".to_string())?;
            file.write_all(&chunk)
                .map_err(|_| "failed to write chunk to file".to_string())?;
            downloaded = std::cmp::min(downloaded + (chunk.len() as u64), total_size);
            ui.set_progress(downloaded);
        }

        let path = {
            let mut path = config.download_path.to_path_buf();
            path.set_extension(extension);
            path
        };

        fs::rename(partial_path, &path).map_err(|_| "failed to rename episode file".to_string())?;

        Ok(DownloadedEpisode::new(self, path))
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

    pub async fn normalize_id3v2(&self) {
        use id3::TagLike;
        if self.path.extension().is_some_and(|ext| ext == "mp3") {
            if let Some(xml_tags) = &self.inner.tags {
                let mut file_tags = id3::Tag::read_from_path(&self.path()).unwrap_or_default();

                for frame in xml_tags.frames() {
                    if !file_tags.get(frame.id()).is_some() {
                        file_tags.add_frame(frame.to_owned());
                    }
                }

                for (id, value) in &self.inner.config.id3_tags {
                    file_tags.set_text(id, value);
                }

                if !file_tags
                    .pictures()
                    .any(|pic| pic.picture_type == id3::frame::PictureType::CoverFront)
                {
                    if let Some(img_url) = self.inner.image_url.as_ref() {
                        if let Some(frame) =
                            crate::cache::get_image(img_url, id3::frame::PictureType::CoverFront)
                                .await
                        {
                            file_tags.add_frame(frame);
                        }
                    }
                }

                let _ = file_tags.write_to_path(&self.path(), id3::Version::Id3v24);
            }
        }
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

    pub fn make_symlink(&mut self) -> Result<(), String> {
        if let Some(symlink_path) = self.inner.config.symlink.as_ref() {
            let new_path = symlink_path.join(self.file_name());
            if self.path() == new_path {
                return Err(format!("symlink points to itself"));
            }

            let _ = std::fs::create_dir_all(&symlink_path);
            if !symlink_path.is_dir() {
                return Err("configured symlink path is not a directory".to_string());
            }

            std::os::unix::fs::symlink(self.path(), new_path)
                .map_err(|_| "failed to create symlink".to_string())?;
        }

        Ok(())
    }

    pub async fn process(&mut self) -> Result<(), String> {
        self.rename()?;
        self.make_symlink()?;
        self.normalize_id3v2().await;

        Ok(())
    }

    pub fn rename(&mut self) -> Result<(), String> {
        let new_name = &self.inner.config.name_pattern;
        let new_name = sanitize_filename::sanitize(new_name);

        let new_path = match self.path.extension() {
            Some(extension) => {
                let mut new_path = self.path.with_file_name(new_name);
                new_path.set_extension(extension);
                new_path
            }
            None => self.path.with_file_name(new_name),
        };

        fs::rename(&self.path, &new_path).map_err(|_| "failed to rename episode".to_string())?;
        self.path = new_path;
        Ok(())
    }
}

impl AsRef<Episode> for DownloadedEpisode<'_> {
    fn as_ref(&self) -> &Episode {
        &self.inner
    }
}

impl AsRef<EpisodeAttributes> for Episode {
    fn as_ref(&self) -> &EpisodeAttributes {
        &self.attrs
    }
}
