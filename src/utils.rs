use crate::config;
use crate::episode::Episode;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::fs::File;
use std::io;
use std::io::Write as IOWrite;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::path::PathBuf;
use std::process;
use std::time;

pub type Unix = std::time::Duration;

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

pub fn config_dir() -> PathBuf {
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

pub fn cache_dir() -> PathBuf {
    let path = match std::env::var("XDG_CACHE_HOME") {
        Ok(path) => PathBuf::from(path),
        Err(_) => dirs::cache_dir().unwrap(),
    }
    .join(crate::APPNAME);

    std::fs::create_dir_all(&path).unwrap();

    path
}

pub fn current_unix() -> Unix {
    let secs = chrono::Utc::now().timestamp() as u64;
    Unix::from_secs(secs)
}

pub fn default_download_path() -> PathBuf {
    let p = dirs::home_dir().unwrap().join(crate::APPNAME);
    std::fs::create_dir_all(&p).unwrap();
    p
}

pub fn truncate_string(s: &str, max_width: usize, append_dots: bool) -> String {
    let mut width = 0;
    let mut truncated = String::new();
    let mut reached_max = false;

    for c in s.chars() {
        let mut buf = [0; 4];
        let encoded_char = c.encode_utf8(&mut buf);
        let char_width = unicode_width::UnicodeWidthStr::width(encoded_char);
        if width + char_width > max_width {
            reached_max = true;
            break;
        }
        truncated.push(c);
        width += char_width;
    }

    if reached_max && append_dots {
        truncated.pop();
        truncated.pop();
        truncated.pop();

        truncated.push_str("...");
    }

    truncated
}

#[derive(Serialize)]
struct BasicPodcast {
    url: String,
}

pub fn handle_response(response: Result<reqwest::Response, reqwest::Error>) -> reqwest::Response {
    match response {
        Ok(res) => res,
        Err(e) => {
            let url = e.url().unwrap().clone();

            let error_message = match e {
                e if e.is_builder() => format!("Invalid URL: {}", url),
                e if e.is_connect() => format!(
                    "Failed to connect to following url {}.\nEnsure you're connected to the internet",
                    url
                ),
                e if e.is_timeout() => format!("Timeout reached for URL: {}", url),
                e if e.is_status() => format!("Server error {}: {}", e.status().unwrap(), url),
                e if e.is_redirect() => format!("Too many redirects for URL: {}", url),
                e if e.is_decode() => format!("Failed to decode response from URL: {}", url),
                _ => format!("An unexpected error occurred: {}", e),
            };
            eprintln!("{}", error_message);
            process::exit(1);
        }
    }
}

pub async fn download_text(client: &reqwest::Client, url: &str) -> String {
    let response = client.get(url).send().await;
    let response = handle_response(response);

    match response.text().await {
        Ok(text) => text,
        Err(e) => {
            eprintln!("failed to decode response from url: {}\nerror:{}", url, e);
            process::exit(1);
        }
    }
}

pub fn edit_file(path: &Path) {
    if !path.exists() {
        eprintln!("error: path does not exist: {:?}", path);
    }

    let editor = match std::env::var("EDITOR") {
        Ok(editor) => editor,
        Err(_) => {
            eprintln!("Unable to edit {:?}", path);
            eprintln!("Please configure your $EDITOR environment variable");
            std::process::exit(1);
        }
    };

    std::process::Command::new(editor)
        .arg(path.to_str().unwrap())
        .status()
        .unwrap();
}

pub fn replacer(val: Value, input: &str) -> String {
    let mut inside = false;
    let mut output = String::new();
    let mut pattern = String::new();
    for c in input.chars() {
        if c == '{' {
            if inside {
                panic!();
            } else {
                inside = true;
            }
        } else if c == '}' {
            if !inside {
                panic!();
            } else {
                let p = std::mem::take(&mut pattern);
                let mut replacement = val
                    .get(&p)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("<<{}>>", p))
                    .replace("\\", "");
                replacement.pop();
                replacement.remove(0);
                output.push_str(&replacement);
                inside = false;
            }
        } else {
            if inside {
                pattern.push(c);
            } else {
                output.push(c);
            }
        }
    }

    output
}

pub async fn search_podcasts(config: &config::GlobalConfig, query: String, catch_up: bool) {
    let response = search(&query).await;
    let mut results = vec![];

    let mut idx = 0;
    for res in response.into_iter() {
        results.push(res);
        idx += 1;
        if idx == config.max_search_results() {
            break;
        }
    }

    if results.is_empty() {
        eprintln!("no podcasts matched your query.");
        return;
    }

    eprintln!("Enter index of podcast to add");
    for (idx, res) in results.iter().enumerate() {
        let line = replacer(res.clone(), &config.search.pattern());
        let line = format!("{}: {}", idx, line);
        let line = truncate_string(&line, config.max_line_width(), true);
        println!("{}", line);
    }

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let input = input.trim();

    if input.is_empty() {
        return;
    }

    let mut indices = vec![];
    for input in input.split(" ") {
        let Ok(num) = input.parse::<usize>() else {
            eprintln!(
                "invalid input: {}. You must enter the index of a podcast",
                input
            );
            return;
        };

        if num > results.len() || num == 0 {
            eprintln!("index {} is out of bounds", num);
            return;
        }

        indices.push(num - 1);
    }

    let mut regex_parts = vec![];
    for index in indices {
        let url = results[index].get("feedUrl").unwrap().to_string();
        let name = results[index].get("artistName").unwrap().to_string();

        let podcast = config::PodcastConfig::new(url);

        if config::PodcastConfigs::push(name.clone(), podcast) {
            eprintln!("'{}' added!", name);
            if catch_up {
                regex_parts.push(format!("^{}$", &name));
            }
        } else {
            eprintln!("'{}' already exists!", name);
        }
    }

    if catch_up && !regex_parts.is_empty() {
        let regex = regex_parts.join("|");
        let filter = Regex::new(&regex).unwrap();
        config::PodcastConfigs::catch_up(Some(filter));
    }
}

pub fn date_str_to_unix(date: &str) -> time::Duration {
    let secs = dateparser::parse(date).unwrap().timestamp();
    time::Duration::from_secs(secs as u64)
}

pub fn get_extension_from_response(response: &reqwest::Response, episode: &Episode) -> String {
    let url = &episode.url;
    let ext = match PathBuf::from(url)
        .extension()
        .and_then(|ext| ext.to_str().map(String::from))
    {
        Some(ext) => ext.to_string(),
        None => {
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|ct| ct.to_str().ok())
                .unwrap_or("application/octet-stream");

            let extensions = mime_guess::get_mime_extensions_str(&content_type).unwrap();

            match extensions.contains(&"mp3") {
                true => "mp3".to_owned(),
                false => extensions
                    .first()
                    .expect("extension not found.")
                    .to_string(),
            }
        }
    };

    // Some urls have these arguments after the extension.
    // feels a bit hacky.
    // todo: find a cleaner way to extract extensions.
    let ext = ext
        .split_once("?")
        .map(|(l, _)| l.to_string())
        .unwrap_or(ext);
    ext
}

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

pub async fn search(terms: &str) -> Vec<Value> {
    let encoded: String = utf8_percent_encode(terms, NON_ALPHANUMERIC).to_string();
    let url = format!(
        "https://itunes.apple.com/search?media=podcast&entity=podcast&term={}",
        encoded
    );
    let resp = reqwest::get(&url).await.unwrap().text().await.unwrap();

    serde_json::from_str::<serde_json::Value>(&resp)
        .unwrap()
        .get("results")
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

pub fn val_to_str<'a>(val: &'a serde_json::Value) -> Option<&'a str> {
    if let Some(val) = val.as_str() {
        return Some(val);
    }

    val.as_object()?.get("#text")?.as_str()
}

pub fn val_to_url<'a>(val: &'a serde_json::Value) -> Option<&'a str> {
    if let Some(val) = val.as_str() {
        return Some(val);
    }

    let obj = val.as_object()?;

    if let Some(url) = obj.get("url") {
        return url.as_str();
    }

    if let Some(url) = obj.get("@href") {
        return url.as_str();
    }

    if let Some(url) = obj.get("src") {
        return url.as_str();
    }

    obj.get("uri")?.as_str()
}

pub fn parse_quoted_words(line: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = line.split('"').collect();
    if parts.len() >= 5 {
        let key = parts[1].to_string();
        let value = parts[3].to_string();
        Some((key, value))
    } else {
        None
    }
}

pub fn get_file_map_val(file_path: &Path, key: &str) -> Option<String> {
    let file = File::open(file_path).ok()?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line.unwrap();
        if let Some((_key, value)) = parse_quoted_words(&line) {
            if _key == key {
                return Some(value);
            }
        }
    }

    None
}

pub fn append_to_config(file_path: &Path, key: &str, value: &str) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .append(true)
        .open(file_path)?;

    let line = format!("\"{}\" \"{}\"\n", key, value);

    file.write_all(line.as_bytes())?;

    Ok(())
}
