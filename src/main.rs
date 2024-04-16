use crate::config::GlobalConfig;
use clap::Parser;
use futures::future;
use indicatif::MultiProgress;
use regex::Regex;
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
    #[arg(
        short,
        long,
        help = "Configure to skip episodes published prior to current time. Can be combined with filter, add, and import"
    )]
    catch_up: bool,
    #[arg(short, long, num_args = 2, value_names = &["URL", "NAME"], help = "Add new podcast")]
    add: Vec<String>,
    #[arg(
        short,
        long,
        help = "Filter which podcasts to sync or export with a regex pattern"
    )]
    filter: Option<Regex>,
    #[arg(
        long,
        value_name = "FILE",
        help = "Override the path to the config file"
    )]
    config: Option<PathBuf>,
    #[arg(long, help = "Edit the config.toml file")]
    edit_config: bool,
    #[arg(long, help = "Edit the podcasts.toml file")]
    edit_podcasts: bool,
}

impl From<Args> for Action {
    fn from(val: Args) -> Self {
        let filter = val.filter;
        let print = val.print;
        let catch_up = val.catch_up;

        let global_config = || match val.config.as_ref() {
            Some(path) => GlobalConfig::load_from_path(path),
            None => GlobalConfig::load(),
        };

        if val.edit_config {
            let path = config::PodcastConfig::path();
            return Self::Edit { path };
        }

        if val.edit_podcasts {
            let path = GlobalConfig::default_path();
            return Self::Edit { path };
        }

        if let Some(path) = val.import {
            return Self::Import { path, catch_up };
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

            return Self::Add {
                url,
                name,
                catch_up,
            };
        }

        if catch_up {
            return Self::CatchUp { filter };
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
    CatchUp {
        filter: Option<Regex>,
    },
    Edit {
        path: PathBuf,
    },
    Import {
        path: PathBuf,
        catch_up: bool,
    },
    Export {
        path: PathBuf,
        filter: Option<Regex>,
        config: GlobalConfig,
    },
    Add {
        url: String,
        name: String,
        catch_up: bool,
    },
    Sync {
        filter: Option<Regex>,
        config: GlobalConfig,
        print: bool,
    },
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    match Action::from(args) {
        Action::Import { path, catch_up } => opml::import(&path, catch_up),

        Action::Edit { path } => utils::edit_file(&path),

        Action::CatchUp { filter } => config::PodcastConfigs::catch_up(filter),

        Action::Export {
            path,
            config,
            filter,
        } => opml::export(&path, &config, filter).await,

        Action::Add {
            name,
            url,
            catch_up,
        } => {
            let podcast = config::PodcastConfig::new(url);

            if config::PodcastConfigs::push(name.clone(), podcast) {
                eprintln!("'{}' added!", name);
                if catch_up {
                    let filter = Regex::new(&format!("^{}$", &name)).unwrap();
                    config::PodcastConfigs::catch_up(Some(filter));
                }
            } else {
                eprintln!("'{}' already exists!", name);
            }
        }

        Action::Sync {
            filter,
            print,
            config,
        } => {
            let mp = MultiProgress::new();

            let podcast_configs = config::PodcastConfigs::load().filter(filter);
            let longest_name = podcast_configs.longest_name(); // Used for formatting.
            let podcasts = podcast::Podcast::load_all(&config, podcast_configs, Some(&mp)).await;

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
