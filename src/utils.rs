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
    let log_file_path = default_download_path();
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file_path)
        .unwrap();
    writeln!(file, "{}", message.as_ref()).unwrap();
}

fn config_dir() -> PathBuf {
    let p = dirs::config_dir().unwrap().join(crate::APPNAME);
    std::fs::create_dir_all(&p).unwrap();
    p
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
 

you wouldn't want the program to start re-downloading episodes every time you move some episodes out of their download folder, right.unwrap() So, every time an episode is downloaded, a hidden file called `.downloaded` is appended with the ID of the episode to stop it from being re-downloaded. It also contains the title so that users can manually edit it, which is encouraged. Every episode takes one line on purpose in order to make git versioning easier. 

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

Just found an old podcast with many episodes and you wanna slowly go through them.unwrap() backlog mode makes it easy!    
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
pub fn append_podcasts(name_and_url: Vec<(String, String)>) {
    let path = crate::utils::podcasts_toml();

    let config_appendix = {
        let mut map = HashMap::new();

        for (name, url) in name_and_url {
            let pod = BasicPodcast { url };
            map.insert(name, pod);
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
