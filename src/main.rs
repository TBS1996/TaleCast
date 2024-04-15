use crate::config::GlobalConfig;
use crate::podcast::Podcast;
use clap::Parser;
use futures::future;
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
#[command(
    name = "TaleCast",
    version,
    about = "A simple CLI podcast manager.",
    long_about = None
)]
struct Args {
    #[arg(
        short,
        long,
        value_name = "FILE",
        help = "Import podcasts from an OPML file"
    )]
    import: Option<PathBuf>,
    #[arg(
        short,
        long,
        value_name = "FILE",
        help = "Export your podcasts to an OPML file"
    )]
    export: Option<PathBuf>,
    #[arg(short, long, help = "Print the downloaded paths to stdout")]
    print: bool,
    #[arg(short, long, help = "Catch up on podcast")]
    catch_up: bool,
    #[arg(short, long, num_args = 2, value_names = &["URL", "NAME"], help = "Add new podcast")]
    add: Vec<String>,
    #[arg(
        short,
        long,
        help = "Filter which podcasts to sync or export with a regex pattern"
    )]
    filter: Option<regex::Regex>,
    #[arg(
        long,
        value_name = "FILE",
        help = "Override the path to the config file"
    )]
    config: Option<PathBuf>,
}

impl From<Args> for Action {
    fn from(val: Args) -> Self {
        let filter = val.filter;
        let print = val.print;

        let global_config = || {
            let config_path = val.config.clone().unwrap_or_else(crate::utils::config_toml);
            GlobalConfig::load(&config_path)
        };

        if let Some(path) = val.import {
            return Self::Import { path };
        }

        if let Some(path) = val.export {
            let config = global_config();
            return Self::Export {
                path,
                filter,
                config,
            };
        }

        if !val.add.is_empty() {
            assert_eq!(val.add.len(), 2);
            let url = val.add[0].to_string();
            let name = val.add[1].to_string();

            return Self::Add { url, name };
        }

        let config = global_config();

        Self::Sync {
            filter,
            print,
            config,
        }
    }
}

enum Action {
    Import {
        path: PathBuf,
    },
    Export {
        path: PathBuf,
        filter: Option<regex::Regex>,
        config: GlobalConfig,
    },
    Add {
        url: String,
        name: String,
    },
    Sync {
        filter: Option<regex::Regex>,
        config: GlobalConfig,
        print: bool,
    },
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    match Action::from(args) {
        Action::Import { path } => opml::import(&path),

        Action::Export {
            path,
            config,
            filter,
        } => opml::export(&path, &config, filter).await,

        Action::Add { name, url } => {
            utils::append_podcasts(vec![(name.clone(), url)]);
            eprintln!("'{}' added!", name);
        }

        Action::Sync {
            filter,
            print,
            config,
        } => {
            let mp = MultiProgress::new();

            let podcasts = Podcast::load_all(&config, filter.as_ref(), Some(&mp)).await;
            let longest_name = longest_podcast_name_len(&podcasts); // Used for formatting.

            let futures = podcasts
                .into_iter()
                .map(|podcast| tokio::task::spawn(async move { podcast.sync(longest_name).await }));

            let episodes: Vec<PathBuf> = future::join_all(futures)
                .await
                .into_iter()
                .filter_map(Result::ok)
                .flatten()
                .collect();

            eprintln!("Syncing complete!\n{} episodes downloaded.", episodes.len());

            if print {
                for path in episodes {
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
