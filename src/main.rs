use crate::config::GlobalConfig;

use crate::podcast::Podcast;
use anyhow::Result;
use clap::Parser;
use indicatif::MultiProgress;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;

mod config;
mod episode;
mod opml;
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.tutorial {
        print!("{}", crate::utils::tutorial());
        return Ok(());
    };
    let should_sync = args.import.is_none() && args.export.is_none();

    if let Some(path) = args.import {
        crate::opml::import(&path)?;
    }

    if let Some(path) = args.export {
        crate::opml::export(&path).await?;
    }

    if !should_sync {
        return Ok(());
    }

    eprintln!("Checking for new episodes...");
    let mp = MultiProgress::new();

    let podcasts = {
        let global_config = GlobalConfig::load()?;
        let mut podcasts = Podcast::load_all(&global_config).await?;
        podcasts.sort_by_key(|pod| pod.name().to_owned());
        podcasts
    };

    // Longest podcast name is used for formatting.
    let Some(longest_name) = podcasts
        .iter()
        .map(|podcast| podcast.name().chars().count())
        .max()
    else {
        eprintln!("no podcasts configured");
        std::process::exit(1);
    };

    let mut futures = vec![];
    for podcast in podcasts {
        let pb = {
            let pb = mp.add(ProgressBar::new_spinner());
            pb.set_style(ProgressStyle::default_spinner().template("{spinner:.green}  {msg}")?);
            pb.set_message(podcast.name().to_owned());
            pb.enable_steady_tick(std::time::Duration::from_millis(100));
            pb
        };

        let future = tokio::task::spawn(async move { podcast.sync(pb, longest_name).await });
        futures.push(future);
    }

    let mut paths = vec![];
    for future in futures {
        paths.extend(future.await??);
    }

    eprintln!("Syncing complete!");
    eprintln!("{} episodes downloaded.", paths.len());

    if args.print {
        for path in paths {
            println!("\"{}\"", path.to_str().unwrap());
        }
    }

    Ok(())
}
