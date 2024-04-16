use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Episode<'a> {
    pub title: &'a str,
    pub url: &'a str,
    pub guid: &'a str,
    pub published: i64,
    pub index: usize,
    pub inner: &'a rss::Item,
    pub raw: &'a serde_json::Map<String, serde_json::Value>,
}

impl<'a> Episode<'a> {
    pub fn new(
        item: &'a rss::Item,
        index: usize,
        raw: &'a serde_json::Map<String, serde_json::Value>,
    ) -> Option<Self> {
        Some(Self {
            title: item.title.as_ref().unwrap(),
            url: item.enclosure().unwrap().url(),
            guid: item.guid().unwrap().value(),
            published: dateparser::parse(item.pub_date().unwrap())
                .unwrap()
                .timestamp(),
            index,
            inner: item,
            raw,
        })
    }

    pub fn get_text_value(&self, tag: &str) -> Option<&str> {
        self.raw.get(tag).unwrap().as_str()
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
