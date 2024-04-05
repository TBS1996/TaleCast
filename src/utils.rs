use anyhow::Result;
use std::io::Write as IOWrite;
use std::path::PathBuf;

#[allow(dead_code)]
pub fn log<S: AsRef<str>>(message: S) -> Result<()> {
    let log_file_path = default_download_path()?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file_path)?;
    writeln!(file, "{}", message.as_ref())?;
    Ok(())
}

fn config_dir() -> Result<PathBuf> {
    let p = dirs::config_dir()
        .ok_or(anyhow::Error::msg("no config dir found"))?
        .join(crate::APPNAME);
    std::fs::create_dir_all(&p)?;
    Ok(p)
}

pub fn podcasts_toml() -> Result<PathBuf> {
    Ok(config_dir()?.join("podcasts.toml"))
}

pub fn config_toml() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

pub fn current_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

pub fn default_download_path() -> Result<PathBuf> {
    let p = dirs::home_dir()
        .ok_or(anyhow::Error::msg("unable to get home directory"))?
        .join(crate::APPNAME);
    std::fs::create_dir_all(&p)?;
    Ok(p)
}

pub fn tutorial() -> &'static str {
    r#"""
# Get started

1. find a podcast you like, for example from `https://podcastindex.org`.
2. click 'copy rss'. Then open or create the `podcasts.toml` file in your `~/.config/cringecast/` directory.
3. format it as so:

[podcast_name]
url={url from `copy rss` link}

4. run the program to start downloading episodes

# basic settings 

There are two kind of settings, global ones, or per-podcast. If you set a setting in the podcast, it will override the global setting (if any).   

The global setting is in `~/.config/config.toml`.   
 
Certain settings can be disabled, for example, you might have a global `max_episodes` setting that limits the amount of episodes being downloaded to '10'. But for a particular podcast, you want to download the entire catalog. Simply put `max_episodes=false` under that podcast's settings.   
 
# keeping track of downloaded episodes  
 

you wouldn't want the program to start re-downloading episodes every time you move some episodes out of their download folder, right? So, every time an episode is downloaded, a hidden file called `.downloaded` is appended with the ID of the episode to stop it from being re-downloaded. It also contains the title so that users can manually edit it, which is encouraged. Every episode takes one line on purpose in order to make git versioning easier. 

# opml import/export 

If you want to import some podcasts, run the program with the --import argument and specify the opml file.  
If you want to export to an opml file, run it with the --export argument and specify the path where the opml file shall be created.

# tagging  

If the file downloaded is an mp3 file, this program will attempt to set as many id3v2 tags as possible from the rss feed. Let me know if anything is missing, or better yet, PR's welcome ^^   

you can also set custom id3 tags with the id3_tags setting, which is a map of tags to values. It's both in per-podcast and global, they are combined. 
 
# file naming

You can specify a pattern for how the downloaded episodes shall be named. this can be done with the `name_pattern` setting, can be configured both in the global config file or per-podcast. The standard is an iso-8601 date along with the title.   
 
It uses variables that you put inside of curly braces, which are from different sources.  
 
{rss::episode::title} => This will be replaced with the 'title' tag contents of the episode part of the xml file.  
{rss::channel::language} => This will be replaced with the 'language' tag contents from the top-level channel part of the xml file.  
{id3::TRCK}  => This will be replaced with the 'track number' tag in the ID3 tags (if it's an mp3}.  
{pubdate::%Y-%m-%d} => this is a more custom one i made, uses the published date from the episode and formats it with chrono however you'd like. 

If you have another preference, let me know so I can implement it :)   
 
# piping

run the program with --print and it'll print out the paths of all the downloaded episodes to stdout so that you can do cool piping things. 
 
# download hook

there is a download_hook setting, can be set in both global and by podcast (and can be disabled per-podcast with download_hook=false).  
in this setting, set the path to a script and it'll be executed with the path of the downloaded episodes as the argument.


# backlog mode   

Just found an old podcast with many episodes and you wanna slowly go through them? backlog mode makes it easy!    
It allows you to get podcast episodes starting from the beginning at a certain interval (days per episode) as if they are new!

there are two settings:  

backlog_start, and backlog_interval  
 
backlog start takes in an ISO-8601 date, interval takes in an integer representing amount of days.  

so let's say you want an episode every 3 days, and the current date is 1. april 2024: 

```
[podcast_name]
url={example.com}
backlog_start="2024-04-01"
backlog_interval=3 
``` 

it will calculate which episodes are eligble for download by seeing how many 3-day intervals have passed since the 'backlog_start' date.

Note this can only be configured per-podcast. Also, you can't have any other restrictions like max_episodes, earliest_date etc.. when you have backlog mode enabled.


 

        """#
}
