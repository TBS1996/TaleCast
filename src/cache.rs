use crate::utils;
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::path::PathBuf;

fn read_file_to_vec(path: &Path) -> io::Result<Vec<u8>> {
    let mut file = fs::File::open(path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).unwrap();
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

async fn write_image(url: &str) {
    use std::io::Write;

    let hashed = hashed_url(url);
    let response = reqwest::get(url).await.unwrap();
    if response.status().is_success() {
        let mime_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string();
        let data = response.bytes().await.unwrap().to_vec();
        let path = utils::cache_dir().join(&hashed);
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(&data).unwrap();
        let mime_path = mime_types_path();
        utils::append_to_config(&mime_path, &hashed, &mime_type).unwrap();
    }
}

fn mime_types_path() -> PathBuf {
    utils::cache_dir().join("mime_types")
}

fn get_mime_type(url: &str) -> Option<String> {
    let hashed = hashed_url(url);
    let path = mime_types_path();
    utils::get_file_map_val(&path, &hashed)
}

pub async fn get_image(
    url: &str,
    picture_type: id3::frame::PictureType,
) -> Option<id3::frame::Frame> {
    let data = match cached_image(url) {
        Some(data) => data,
        None => {
            write_image(url).await;
            cached_image(url)?
        }
    };

    let mime_type = get_mime_type(url).unwrap();

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
