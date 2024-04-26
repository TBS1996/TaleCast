use crate::display::DownloadBar;
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

fn cached_image(url: &str, ui: &DownloadBar) -> Option<Vec<u8>> {
    let hash = hashed_url(url);
    let path = utils::cache_dir().join(hash);
    let image = read_file_to_vec(&path).ok();

    if image.is_some() {
        ui.log_debug("loaded cached image");
    } else {
        ui.log_debug("failed to load cached image");
    }

    image
}

async fn write_image(url: &str, ui: &DownloadBar) -> Option<()> {
    use std::io::Write;

    let hashed = hashed_url(url);
    let response = match reqwest::get(url).await {
        Ok(res) => {
            ui.log_info("connected to image url");
            res
        }

        Err(e) => {
            ui.log_error(&format!("failed to connect to image url: {:?}", e));
            return None;
        }
    };

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
    } else {
        ui.log_error("response status to image url connection not successful");
    };
    Some(())
}

pub async fn get_image(
    url: &str,
    picture_type: id3::frame::PictureType,
    ui: &DownloadBar,
) -> Option<id3::frame::Frame> {
    let data = match cached_image(url, ui) {
        Some(data) => data,
        None => {
            write_image(url, ui).await?;
            cached_image(url, ui)?
        }
    };

    let mime_type = match MimeMap::get_mime(url) {
        Some(mime) => mime,
        None => {
            ui.log_warn(&format!("failed to load mime for: {:?}", url));
            return None;
        }
    };

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
