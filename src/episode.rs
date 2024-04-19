use crate::utils;
use std::path::Path;
use std::path::PathBuf;
use std::time;

#[derive(Debug, Clone)]
pub struct Episode<'a> {
    pub title: &'a str,
    pub url: &'a str,
    pub mime: Option<&'a str>,
    pub guid: &'a str,
    pub published: time::Duration,
    pub index: usize,
    pub raw: &'a serde_json::Map<String, serde_json::Value>,
}

impl<'a> Episode<'a> {
    pub fn new(index: usize, raw: &'a serde_json::Map<String, serde_json::Value>) -> Option<Self> {
        let title = raw.get("title").unwrap().as_str().unwrap();
        let enclosure = raw.get("enclosure").unwrap();
        let url = enclosure.get("@url").map(|x| x.as_str()).unwrap().unwrap();
        let mime = enclosure.get("@type").and_then(|x| x.as_str());
        let published = utils::date_str_to_unix(raw.get("pubDate").unwrap().as_str().unwrap());
        let guid = utils::val_to_str(raw.get("guid")?)?;

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
        let key = format!("itunes{}image", utils::NAMESPACE_ALTER);
        utils::val_to_url(self.raw.get(&key)?)
    }

    pub fn author(&self) -> Option<&str> {
        self.get_str("author")
    }

    pub fn description(&self) -> Option<&str> {
        self.get_str("description")
    }

    pub fn itunes_episode(&self) -> Option<&str> {
        let key = format!("itunes{}episode", utils::NAMESPACE_ALTER);
        self.get_str(&key)
    }

    pub fn itunes_duration(&self) -> Option<&str> {
        let key = format!("itunes{}duration", utils::NAMESPACE_ALTER);
        self.get_str(&key)
    }

    /// Filename of episode when it's being downloaded.
    pub fn partial_name(&self) -> String {
        let file_name = sanitize_filename::sanitize(&self.guid);
        format!("{}.partial", file_name)
    }
}

pub struct DownloadedEpisode<'a> {
    inner: Episode<'a>,
    path: PathBuf,
}

impl<'a> DownloadedEpisode<'a> {
    pub fn new(inner: Episode<'a>, path: PathBuf) -> DownloadedEpisode<'a> {
        Self { inner, path }
    }

    pub fn inner(&self) -> &Episode<'a> {
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

impl<'a> AsRef<Episode<'a>> for DownloadedEpisode<'a> {
    fn as_ref(&self) -> &Episode<'a> {
        &self.inner
    }
}
