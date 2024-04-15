use quick_xml::{
    events::{BytesEnd, BytesStart, Event},
    Reader, Writer,
};
use serde::Serialize;
use serde_json::Value;
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Cursor;
use std::io::Write as IOWrite;
use std::path::PathBuf;

pub type Unix = std::time::Duration;

/// Refer to [`remove_xml_namespaces`] for an explanation.
pub const NAMESPACE_ALTER: &'static str = "__placeholder__";

#[allow(dead_code)]
pub fn log<S: AsRef<str>>(message: S) {
    let log_file_path = default_download_path().join("logfile");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file_path)
        .unwrap();
    writeln!(file, "{}", message.as_ref()).unwrap();
}

fn config_dir() -> PathBuf {
    let path = match std::env::var("XDG_CONFIG_HOME") {
        Ok(path) => PathBuf::from(path),
        Err(_) => dirs::home_dir()
            .expect("unable to locate home directory. Try setting 'XDG_CONFIG_HOME' manually")
            .join(".config"),
    }
    .join(crate::APPNAME);

    std::fs::create_dir_all(&path).unwrap();

    path
}

pub fn podcasts_toml() -> PathBuf {
    config_dir().join("podcasts.toml")
}

pub fn config_toml() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn current_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

pub fn default_download_path() -> PathBuf {
    let p = dirs::home_dir().unwrap().join(crate::APPNAME);
    std::fs::create_dir_all(&p).unwrap();
    p
}

pub fn get_guid(item: &serde_json::Map<String, Value>) -> &str {
    let guid_obj = item.get("guid").unwrap();
    if let Some(guid) = guid_obj.as_str() {
        return guid;
    }

    guid_obj
        .as_object()
        .unwrap()
        .get("#text")
        .unwrap()
        .as_str()
        .unwrap()
}

/// The quickxml_to_serde library merges tags that have same name but different namespaces.
/// This is not the behaviour i want, as users should be able to fetch specific names with
/// patterns. This is a hack to avoid it, by replacing the colon (which marks a namespace)
/// with a replacement symbol. When the user then queries a tag with a pattern,
/// we replace the colons in their pattern with the same replacement.
pub fn remove_xml_namespaces(xml: &str, replacement: &str) -> String {
    fn modify_name<'a>(original_name: &'a [u8], replacement: &'a str) -> Cow<'a, [u8]> {
        if let Some(pos) = original_name.iter().position(|&b| b == b':') {
            let mut new_name = Vec::from(&original_name[..pos]);
            new_name.extend_from_slice(replacement.as_bytes());
            new_name.extend_from_slice(&original_name[pos + 1..]);
            Cow::Owned(new_name)
        } else {
            Cow::Borrowed(original_name)
        }
    }

    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);
    let mut writer = Writer::new(Cursor::new(Vec::new()));

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let modified_name = modify_name(name.as_ref(), replacement);
                let elem_name_str = String::from_utf8_lossy(&modified_name);
                let elem = BytesStart::new(elem_name_str.as_ref());
                writer
                    .write_event(Event::Start(elem))
                    .expect("Unable to write event");
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let modified_name = modify_name(name.as_ref(), replacement);
                let elem_name_str = String::from_utf8_lossy(&modified_name);
                let elem = BytesEnd::new(elem_name_str.as_ref());
                writer
                    .write_event(Event::End(elem))
                    .expect("Unable to write event");
            }
            Ok(Event::Eof) => break,
            Ok(e) => writer.write_event(e).expect("Unable to write event"),
            Err(e) => panic!("Error at position {}: {:.?}", reader.buffer_position(), e),
        }
    }

    let result = writer.into_inner().into_inner();
    String::from_utf8(result).expect("Found invalid UTF-8")
}

pub fn truncate_string(s: &str, max_width: usize) -> String {
    let mut width = 0;
    let mut truncated = String::new();

    for c in s.chars() {
        let mut buf = [0; 4];
        let encoded_char = c.encode_utf8(&mut buf);
        let char_width = unicode_width::UnicodeWidthStr::width(encoded_char);
        if width + char_width > max_width {
            break;
        }
        truncated.push(c);
        width += char_width;
    }

    truncated
}

#[derive(Serialize)]
struct BasicPodcast {
    url: String,
}

/// Extends the podcasts.toml file with new podcasts.
///
/// The reason it doesn't simply deserialize, modify, then serialize, is to not overrwite comments
/// in the config.
pub fn append_podcasts(name_and_url: Vec<(String, String)>) -> bool {
    let path = crate::utils::podcasts_toml();
    let current_podcasts = crate::config::PodcastConfig::load_all();

    let config_appendix = {
        let mut map = HashMap::new();

        for (name, url) in name_and_url {
            if current_podcasts.contains_key(&name) {
                continue;
            }
            let pod = BasicPodcast { url };
            map.insert(name, pod);
        }

        if map.is_empty() {
            return false;
        }

        toml::to_string_pretty(&map).unwrap()
    };

    let new_config = match path.exists() {
        true => {
            let binding = std::fs::read_to_string(&path).unwrap();
            let old_string = binding.trim_end_matches('\n');

            if old_string.is_empty() {
                config_appendix
            } else {
                format!("{}\n\n{}", old_string, config_appendix)
            }
        }
        false => config_appendix,
    };

    std::fs::write(&path, new_config).unwrap();

    true
}

pub async fn download_text(url: &str) -> String {
    reqwest::Client::new()
        .get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (X11; Linux x86_64; rv:124.0) Gecko/20100101 Firefox/124.0",
        )
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modify_xml_tags() {
        let xml = r#"<root><foo:bar>Content</foo:bar><baz:qux>More Content</baz:qux></root>"#;
        let replacement = "___placeholder___";

        let expected = r#"<root><foo___placeholder___bar>Content</foo___placeholder___bar><baz___placeholder___qux>More Content</baz___placeholder___qux></root>"#;

        let modified_xml = remove_xml_namespaces(xml, replacement);

        assert_eq!(
            modified_xml, expected,
            "The modified XML does not match the expected output."
        );
    }
}
