use crate::utils;
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::path::PathBuf;

struct MimeMap;

impl MimeMap {
    fn get_mime(url: &str) -> Option<String> {
        let hashed = hashed_url(url);
        let path = Self::path();
        utils::get_file_map_val(&path, &hashed)
    }

    fn append(url: &str, mime: &str) -> Option<()> {
        let path = Self::path();
        let hashed = hashed_url(url);
        utils::append_to_config(&path, &hashed, &mime).ok()?;
        Some(())
    }

    fn path() -> PathBuf {
        utils::cache_dir().join("mime_types")
    }
}

fn read_file_to_vec(path: &Path) -> io::Result<Vec<u8>> {
    let mut file = fs::File::open(path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    Ok(data)
}

fn hashed_url(url: &str) -> String {
    use std::hash::Hasher;
    let mut hasher = fnv::FnvHasher::default();
    hasher.write(url.as_bytes());
    let hash = hasher.finish();
    format!("{:x}", hash)
}

fn cached_image(url: &str) -> Option<Vec<u8>> {
    let hash = hashed_url(url);
    let path = utils::cache_dir().join(hash);
    read_file_to_vec(&path).ok()
}

async fn write_image(url: &str) -> Option<()> {
    use std::io::Write;

    let hashed = hashed_url(url);
    let response = reqwest::get(url).await.ok()?;
    if response.status().is_success() {
        let mime_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string();
        let data = response.bytes().await.ok()?.to_vec();
        let path = utils::cache_dir().join(&hashed);
        let mut file = fs::File::create(&path).ok()?;
        file.write_all(&data).ok()?;
        MimeMap::append(url, &mime_type)?;
    }
    Some(())
}

pub async fn get_image(
    url: &str,
    picture_type: id3::frame::PictureType,
) -> Option<id3::frame::Frame> {
    let data = match cached_image(url) {
        Some(data) => data,
        None => {
            write_image(url).await?;
            cached_image(url)?
        }
    };

    let mime_type = MimeMap::get_mime(url)?;

    let pic = id3::frame::Picture {
        data,
        mime_type,
        description: String::default(),
        picture_type,
    };

    Some(id3::frame::Frame::with_content(
        "APIC",
        id3::frame::Content::Picture(pic),
    ))
}
