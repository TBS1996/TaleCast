use crate::config::GlobalConfig;
use crate::podcast::Podcast;
use clap::Parser;
use indicatif::MultiProgress;
use std::path::PathBuf;

mod config;
mod episode;
mod opml;
mod patterns;
mod podcast;
mod tags;
mod utils;

pub const APPNAME: &'static str = "talecast";

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, value_name = "FILE")]
    import: Option<PathBuf>,
    #[arg(short, long, value_name = "FILE")]
    export: Option<PathBuf>,
    #[arg(short, long)]
    print: bool,
    #[arg(long)]
    tutorial: bool,
    #[arg(short, long, num_args = 2)]
    add: Vec<String>,
    #[arg(short, long)]
    filter: Option<regex::Regex>,
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,
}

impl From<Args> for Action {
    fn from(val: Args) -> Self {
        let filter = val.filter;
        let print = val.print;

        if val.tutorial {
            return Self::Tutorial;
        }

        if let Some(path) = val.import {
            return Self::Import { path };
        }

        if let Some(path) = val.export {
            return Self::Export { path, filter };
        }

        if !val.add.is_empty() {
            assert_eq!(val.add.len(), 2);
            let url = val.add[0].to_string();
            let name = val.add[1].to_string();
            return Self::Add { url, name };
        }

        Self::Sync { filter, print }
    }
}

enum Action {
    Import {
        path: PathBuf,
    },
    Export {
        path: PathBuf,
        filter: Option<regex::Regex>,
    },
    Tutorial,
    Add {
        url: String,
        name: String,
    },
    Sync {
        filter: Option<regex::Regex>,
        print: bool,
    },
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let global_config = {
        let config_path = args
            .config
            .clone()
            .unwrap_or_else(crate::utils::config_toml);
        GlobalConfig::load(&config_path)
    };

    match Action::from(args) {
        Action::Tutorial => print!("{}", crate::utils::tutorial()),
        Action::Import { path } => crate::opml::import(&path),
        Action::Export { path, filter } => {
            crate::opml::export(&path, &global_config, filter.as_ref()).await
        }
        Action::Add { name, url } => {
            crate::utils::append_podcasts(vec![(name.clone(), url)]);
            eprintln!("'{}' added!", name);
        }
        Action::Sync { filter, print } => {
            eprintln!("Checking for new episodes...");

            let mp = MultiProgress::new();

            let podcasts = Podcast::load_all(&global_config, filter.as_ref(), Some(&mp)).await;
            let longest_name = longest_podcast_name_len(&podcasts); // Used for formatting.

            let mut futures = vec![];
            for podcast in podcasts {
                let future = tokio::task::spawn(async move { podcast.sync(longest_name).await });
                futures.push(future);
            }

            let mut paths = vec![];
            for future in futures {
                paths.extend(future.await.unwrap());
            }

            eprintln!("Syncing complete!");
            eprintln!("{} episodes downloaded.", paths.len());

            if print {
                for path in paths {
                    println!("{}", path.to_str().unwrap());
                }
            }
        }
    }
}

/// Longest podcast name is used for formatting.
fn longest_podcast_name_len(pods: &Vec<Podcast>) -> usize {
    match pods
        .iter()
        .map(|podcast| podcast.name().chars().count())
        .max()
    {
        Some(len) => len,
        None => {
            eprintln!("no podcasts configured");
            std::process::exit(1);
        }
    }
}
