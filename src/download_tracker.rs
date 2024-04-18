use crate::episode::DownloadedEpisode;
use crate::utils;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Keeps track of which episodes have already been downloaded.
#[derive(Debug, Default)]
pub struct DownloadedEpisodes(HashSet<String>);

impl DownloadedEpisodes {
    pub fn contains_episode(&self, episode_id: &str) -> bool {
        self.0.contains(episode_id)
    }

    pub fn load(path: &Path) -> Self {
        let s = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Self::default();
            }
            e @ Err(_) => e.unwrap(),
        };

        let mut hashmap: HashSet<String> = HashSet::new();

        for line in s.trim().lines() {
            let mut parts = line.split_whitespace();
            if let Some(id) = parts.next() {
                hashmap.insert(id.to_string());
            }
        }

        Self(hashmap)
    }

    pub fn append(path: &Path, id: &str, episode: &DownloadedEpisode) {
        use std::io::Write;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(&parent).unwrap();
        }

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .unwrap();

        writeln!(
            file,
            "{} {} \"{}\"",
            id,
            utils::current_unix().as_secs(),
            episode.as_ref().title
        )
        .unwrap();
    }
}
