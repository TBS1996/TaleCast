use anyhow::Result;
use futures_util::StreamExt;
use indicatif::ProgressBar;
use reqwest::Client;
use std::io::Write as IoWrite;
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
            title: item.title.as_ref()?,
            url: item.enclosure()?.url(),
            guid: item.guid()?.value(),
            published: chrono::DateTime::parse_from_rfc2822(item.pub_date()?)
                .ok()?
                .timestamp(),
            index,
            inner: item,
            raw,
        })
    }

    pub fn get_text_value(&self, tag: &str) -> Option<&str> {
        self.raw.get(tag)?.as_str()
    }

    pub async fn download(&self, folder: &Path, pb: &ProgressBar) -> Result<PathBuf> {
        let response = Client::new().get(self.url).send().await?;
        let total_size = response.content_length().unwrap_or(0);

        pb.set_length(total_size);

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|ct| ct.to_str().ok())
            .unwrap_or("application/octet-stream");

        let extensions = mime_guess::get_mime_extensions_str(&content_type).unwrap();

        let ext = match extensions.contains(&"mp3") {
            true => "mp3",
            false => extensions.first().expect("extension not found."),
        };

        let path = {
            let file_name = format!("{}.{}", self.guid, ext);
            folder.join(file_name)
        };

        let mut file = std::fs::File::create(&path)?;
        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();

        while let Some(item) = stream.next().await {
            let chunk = item?;
            file.write_all(&chunk)?;
            let new = std::cmp::min(downloaded + (chunk.len() as u64), total_size);
            pb.set_position(new);
            downloaded = new;
        }

        Ok(path)
    }
}
