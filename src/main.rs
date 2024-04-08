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

pub const APPNAME: &'static str = "cringecast";

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
    #[arg(short, long)]
    quiet: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let config_path = args.config.unwrap_or_else(crate::utils::config_toml);

    let should_sync =
        args.import.is_none() && args.export.is_none() && args.add.is_empty() && !args.tutorial;

    if args.tutorial {
        print!("{}", crate::utils::tutorial());
    };

    if let Some(path) = args.import {
        crate::opml::import(&path);
    }

    if let Some(path) = args.export {
        crate::opml::export(&path, &config_path, args.filter.as_ref()).await;
    }

    if !args.add.is_empty() {
        assert_eq!(args.add.len(), 2);
        let url = &args.add[0];
        let name = &args.add[1];
        crate::utils::append_podcasts(vec![(name.to_string(), url.to_string())]);
        eprintln!("'{}' added!", name);
    }

    if !should_sync {
        return;
    }

    let mp = (!args.quiet).then_some(MultiProgress::new());

    let podcasts = {
        let global_config = GlobalConfig::load(&config_path);
        let mut podcasts =
            Podcast::load_all(&global_config, args.filter.as_ref(), mp.as_ref()).await;
        podcasts.sort_by_key(|pod| pod.name().to_owned());
        podcasts
    };

    // Longest podcast name is used for formatting.
    let longest_name = match podcasts
        .iter()
        .map(|podcast| podcast.name().chars().count())
        .max()
    {
        Some(len) => len,
        None => {
            eprintln!("no podcasts configured");
            std::process::exit(1);
        }
    };

    let mut futures = vec![];

    eprintln!("Checking for new episodes...");
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

    if args.print {
        for path in paths {
            println!("\"{}\"", path.to_str().unwrap());
        }
    }
}
