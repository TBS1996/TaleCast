use std::fs::File;
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
            published: chrono::DateTime::parse_from_rfc2822(item.pub_date().unwrap())
                .ok()
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
    pub inner: Episode<'a>,
    pub file: File,
    pub path: PathBuf,
}
