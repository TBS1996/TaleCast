use crate::config;
use crate::config::GlobalConfig;
use crate::config::PodcastConfig;
use crate::podcast;
use opml::OPML;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::io::Write as IoWrite;
use std::path::Path;

pub async fn export(p: &Path, global_config: GlobalConfig, filter: Option<Regex>) {
    let podcast_configs = config::PodcastConfigs::load().filter(filter);
    let podcasts = podcast::Podcasts::new(global_config, podcast_configs).await;

    let opml = OPML::from(podcasts);
    let xml_string = opml.to_string().unwrap();

    fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(p)
        .unwrap()
        .write_all(xml_string.as_bytes())
        .unwrap();
}

pub fn import(p: &Path, catch_up: bool) {
    let opml_string = std::fs::read_to_string(p).unwrap();
    let opml = opml::OPML::from_str(&opml_string).unwrap();

    let mut podcasts = HashMap::default();

    for podcast in opml.body.outlines.into_iter() {
        let title = {
            let title = podcast.title.unwrap_or(podcast.text);

            if title.is_empty() {
                None
            } else {
                Some(title)
            }
        };

        let (name, mut podcast) = match (title, podcast.xml_url) {
            (None, None) => {
                eprintln!("importing failed due to feed with missing title and url");
                std::process::exit(1);
            }
            (Some(title), None) => {
                eprintln!(
                    "importing failed due to following podcast missing its' url: {}",
                    title
                );
                std::process::exit(1);
            }
            (None, Some(url)) => {
                eprintln!(
                    "importing failed due to podcast with following url missing a title: {}",
                    url
                );
                std::process::exit(1);
            }
            (Some(title), Some(url)) => (title, PodcastConfig::new(url)),
        };

        if catch_up {
            podcast.catch_up();
        }

        podcasts.insert(name, podcast);
    }

    if podcasts.is_empty() {
        eprintln!("no podcasts found.");
    } else {
        config::PodcastConfigs::extend(podcasts);
    }
}
