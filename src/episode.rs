use crate::cache;
use crate::config::Config;
use crate::config::DownloadMode;
use crate::display::DownloadBar;
use crate::download_tracker::DownloadedEpisodes;
use crate::utils;
use futures_util::StreamExt;
use std::cmp;
use std::fs;
use std::io::Seek;
use std::io::Write as IOWrite;
use std::path::Path;
use std::path::PathBuf;
use std::time;
use tokio::task::JoinHandle;

pub trait XmlWrapper {
    fn inner(&self) -> &serde_json::Map<String, serde_json::Value>;

    fn get_str(&self, key: &str) -> Result<&str, String> {
        let val = self.get_val(key)?;

        utils::val_to_str(val).ok_or_else(|| "value could not be parsed as string".into())
    }

    fn get_url(&self, key: &str) -> Result<&str, String> {
        let val = self.get_val(key)?;
        match utils::val_to_url(val) {
            Some(val) => Ok(val),
            None => return Err("failed to parse val as url".to_string()),
        }
    }

    fn get_val(&self, key: &str) -> Result<&serde_json::Value, String> {
        self.inner()
            .get(key)
            .ok_or_else(|| format!("missing key: {}", key))
    }

    fn get_string(&self, key: &str) -> Result<String, String> {
        self.get_str(key).map(|s| s.to_string())
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
pub struct Attributes {
    pub title: String,
    pub url: String,
    pub mime: Option<String>,
    pub guid: String,
    pub published: time::Duration,
    pub raw: RawEpisode,
}

impl Attributes {
    pub fn new(raw: RawEpisode) -> Result<Self, String> {
        let title = raw.get_string("title")?;
        let enclosure = raw.get_val("enclosure")?;

        let url = enclosure
            .get("@url")
            .ok_or_else(|| "url not found".to_string())?
            .to_string();
        let url = utils::trim_quotes(&url);

        let mime = enclosure
            .get("@type")
            .and_then(|x| Some(x.as_str()?.to_string()));

        let published = raw.get_str("pubDate")?;
        let published = utils::date_str_to_unix(published)?;
        let guid = raw.get_string("guid")?;

        Ok(Self {
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

    pub fn get_str(&self, key: &str) -> Result<&str, String> {
        self.raw.get_str(key)
    }

    pub fn image(&self) -> Result<&str, String> {
        let key = "itunes:image";
        self.raw.get_url(key)
    }
    pub fn author(&self) -> Result<&str, String> {
        self.get_str("author")
    }

    pub fn description(&self) -> Result<&str, String> {
        self.get_str("description")
    }

    pub fn itunes_episode(&self) -> Result<&str, String> {
        let key = "itunes:episode";
        self.get_str(&key)
    }

    pub fn itunes_duration(&self) -> Result<&str, String> {
        let key = "itunes:duration";
        self.get_str(&key)
    }
}

#[derive(Debug, Clone)]
pub struct Episode {
    pub config: Config,
    pub tags: Option<id3::Tag>,
    pub index: usize,
    pub attrs: Attributes,
    pub image_url: Option<String>,
}

impl Episode {
    pub fn new(
        attrs: Attributes,
        index: usize,
        config: Config,
        tags: Option<id3::Tag>,
        image_url: Option<String>,
    ) -> Self {
        Self {
            attrs,
            config,
            tags,
            index,
            image_url,
        }
    }

    const TITLELEN: usize = 30;

    pub fn _log_error(&self, ui: &DownloadBar, msg: impl Into<String>) {
        let ep_name = utils::truncate_string(self.attrs.title(), Self::TITLELEN, true);
        let msg = format!("{}: {}", ep_name, msg.into());
        ui.log_error(msg);
    }

    pub fn log_warn(&self, ui: &DownloadBar, msg: impl Into<String>) {
        let ep_name = utils::truncate_string(self.attrs.title(), Self::TITLELEN, true);
        let msg = format!("{}: {}", ep_name, msg.into());
        ui.log_warn(msg);
    }

    pub fn log_trace(&self, ui: &DownloadBar, msg: impl Into<String>) {
        let ep_name = utils::truncate_string(self.attrs.title(), Self::TITLELEN, true);
        let msg = format!("{}: {}", ep_name, msg.into());
        ui.log_trace(msg);
    }

    pub fn log_debug(&self, ui: &DownloadBar, msg: impl Into<String>) {
        let ep_name = utils::truncate_string(self.attrs.title(), Self::TITLELEN, true);
        let msg = format!("{}: {}", ep_name, msg.into());
        ui.log_debug(msg);
    }

    fn is_downloaded(&self) -> bool {
        let id = self.get_id();
        let path = self.tracker_path();
        DownloadedEpisodes::load(&path).contains_episode(&id)
    }

    pub fn within_age_limits(&self, mode: &DownloadMode, episode_qty: usize) -> bool {
        let passed_filter = match mode {
            
            DownloadMode::Backlog { start, interval, max_episodes: _ } => {
                
                let time_passed = utils::current_unix() - *start;
                let intervals_passed = time_passed.as_secs() / interval.as_secs();
                intervals_passed >= self.index as u64
            }

            DownloadMode::Standard {
                max_time,
                max_episodes: _,
                earliest_date,
            } => {
                let max_time_exceeded = max_time.map_or(false, |max_time| {
                    (utils::current_unix() - self.attrs.published) > max_time
                });

                let episode_too_old =
                    earliest_date.map_or(false, |date| date > self.attrs.published);

                !max_time_exceeded && !episode_too_old
            }
        };

        passed_filter && !self.is_downloaded()

    }

    /// Filename of episode when it's being downloaded.
    fn partial_name(&self) -> String {
        let file_name = sanitize_filename::sanitize(&self.attrs.guid);
        format!("{}.partial", file_name)
    }

    fn get_id(&self) -> String {
        self.config.id_pattern.replace(" ", "_")
    }

    fn tracker_path(&self) -> &Path {
        self.config.tracker_path.as_path()
    }

    fn into_downloaded(&self, path: PathBuf) -> DownloadedEpisode<'_> {
        DownloadedEpisode::new(self, path)
    }

    pub async fn download<'a>(
        &'a self,
        client: &reqwest::Client,
        ui: &DownloadBar,
    ) -> Result<DownloadedEpisode<'a>, String> {
        self.log_debug(ui, "downloading episode");
        let audio_file = self.download_enclosure(client, ui).await?;
        let mut episode = self.into_downloaded(audio_file);
        episode.process(ui).await?;
        episode.run_download_hook(ui);
        episode.mark_downloaded()?;
        Ok(episode)
    }

    async fn download_enclosure<'a>(
        &'a self,
        client: &reqwest::Client,
        ui: &DownloadBar,
    ) -> Result<PathBuf, String> {
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

        self.log_trace(ui, format!("connecting to url: {:?}", self.as_ref().url()));
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
            downloaded = cmp::min(downloaded + (chunk.len() as u64), total_size);
            ui.set_progress(downloaded);
        }

        let path = {
            let mut path = config
                .download_path
                .to_path_buf()
                .join(&self.partial_name());
            path.set_extension(extension);
            path
        };

        fs::rename(partial_path, &path).map_err(|_| "failed to rename episode file".to_string())?;

        Ok(path)
    }
}

pub struct DownloadedEpisode<'a> {
    inner: &'a Episode,
    /// Where the episode is downloaded.
    path: PathBuf,
    /// The handle to the process of an optional post-download hook.
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

    pub fn into_path(self) -> PathBuf {
        self.path
    }

    pub fn mark_downloaded(&self) -> Result<(), String> {
        let id = self.inner.config.id_pattern.replace(" ", "_");
        let path = self.inner.config.tracker_path.as_path();
        DownloadedEpisodes::append(&path, &id, self)
    }

    pub fn inner(&self) -> &Episode {
        &self.inner
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn normalize_id3v2(&self, ui: &DownloadBar) {
        use id3::TagLike;
        if self.path.extension().is_some_and(|ext| ext == "mp3") {
            self.inner.log_trace(ui, "normalizing id3 tags");
            if let Some(xml_tags) = &self.inner.tags {
                let mut file_tags = id3::Tag::read_from_path(&self.path()).unwrap_or_default();

                for frame in xml_tags.frames() {
                    if !file_tags.get(frame.id()).is_some() {
                        file_tags.add_frame(frame.to_owned());
                        self.inner
                            .log_trace(ui, format!("adding frame: {:?}", &frame));
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
                            cache::get_image(img_url, id3::frame::PictureType::CoverFront, ui).await
                        {
                            file_tags.add_frame(frame);
                            self.inner
                                .log_debug(ui, "added cover image to podcast episode");
                        } else {
                            self.inner.log_warn(
                                ui,
                                format!("failed to fetch image from url: {:?}", img_url),
                            );
                        };
                    }
                }

                if let Err(e) = file_tags.write_to_path(&self.path(), id3::Version::Id3v24) {
                    ui.log_error(format!("failed to write tags to file: {:?}", e));
                };
            }
        } else {
            self.inner
                .log_trace(ui, "skipping id3 tag normalization: enclosure not an mp3");
        };
    }

    fn file_name(&self) -> &str {
        self.path.file_name().unwrap().to_str().unwrap()
    }

    pub async fn await_handle(&mut self, ui: &DownloadBar) {
        if let Some(handle) = self.handle.take() {
            self.inner.log_debug(ui, "awaiting download hook");
            let _ = handle.await;
        }
    }

    fn run_download_hook(&mut self, ui: &DownloadBar) {
        let Some(script_path) = self.inner.config.download_hook.clone() else {
            self.inner.log_trace(ui, "no download hook configured");
            return;
        };

        self.inner.log_debug(ui, "running download hook");

        let path = self.path().to_owned();

        let handle = tokio::task::spawn_blocking(move || {
            std::process::Command::new(script_path)
                .arg(path)
                .output()
                .unwrap();
        });

        self.handle = Some(handle);
    }

    fn make_symlink(&mut self, ui: &DownloadBar) -> Result<(), String> {
        if let Some(symlink_path) = self.inner.config.symlink.as_ref() {
            self.inner.log_trace(ui, "creating symlink...");
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

    async fn process(&mut self, ui: &DownloadBar) -> Result<(), String> {
        self.inner.log_debug(ui, "processing episode");
        self.rename()?;
        self.make_symlink(ui)?;
        self.normalize_id3v2(ui).await;

        Ok(())
    }

    fn rename(&mut self) -> Result<(), String> {
        let new_name = &self.inner.config.name_pattern;
        let mut new_name = sanitize_filename::sanitize(new_name);

        let new_path = match self.path.extension() {
            Some(extension) => {
                let max_file_len: usize = 255;
                let ext_len = extension.len() + 1; // + 1 for the dot.
                let overflow = (new_name.len() + ext_len).saturating_sub(max_file_len);
                for _ in 0..overflow {
                    new_name.pop();
                }

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

impl AsRef<Attributes> for Episode {
    fn as_ref(&self) -> &Attributes {
        &self.attrs
    }
}
