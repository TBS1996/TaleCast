use crate::utils;
use std::path::Path;
use std::path::PathBuf;
use std::time;

#[derive(Debug, Clone)]
pub struct Episode {
    pub title: String,
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

        Self {
            title,
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

    /// Filename of episode when it's being downloaded.
    pub fn partial_name(&self) -> String {
        let file_name = sanitize_filename::sanitize(&self.guid);
        format!("{}.partial", file_name)
    }
}

pub struct DownloadedEpisode<'a> {
    inner: &'a Episode,
    path: PathBuf,
}

impl<'a> DownloadedEpisode<'a> {
    pub fn new(inner: &'a Episode, path: PathBuf) -> DownloadedEpisode<'a> {
        Self { inner, path }
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
