use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::episode::Episode;
use crate::podcast::Podcast;
use crate::utils;

use regex::Regex;

#[derive(Debug, Clone)]
pub struct FullPattern(Vec<Segment>);

impl FullPattern {
    pub fn from_str(s: &str) -> Self {
        let mut segments: Vec<Segment> = vec![];
        let mut text = String::new();
        let mut pattern = String::new();

        let mut is_inside = false;

        for c in s.chars() {
            if c == '}' {
                assert!(is_inside);
                let text_pattern = std::mem::take(&mut pattern);
                let pattern = Pattern::from_str(&text_pattern);
                let segment = Segment::Pattern(pattern);
                segments.push(segment);
                is_inside = false;
            } else if c == '{' {
                assert!(!is_inside);
                let text = std::mem::take(&mut text);
                segments.push(Segment::Text(text));
                is_inside = true;
            } else {
                if is_inside {
                    pattern.push(c);
                } else {
                    text.push(c);
                }
            }
        }

        assert!(!is_inside);
        if !text.is_empty() {
            segments.push(Segment::Text(text));
        }

        Self(segments)
    }
}

#[derive(Clone, Debug)]
enum Segment {
    Text(String),
    Pattern(Pattern),
}

#[derive(Clone, Copy)]
pub struct DataSources<'a> {
    podcast: &'a Podcast,
    episode: &'a Episode<'a>,
}

impl<'a> DataSources<'a> {
    pub fn new(podcast: &'a Podcast, episode: &'a Episode) -> Self {
        Self { podcast, episode }
    }
}

#[derive(Debug, Clone)]
enum Pattern {
    Unit(UnitPattern),
    Data(DataPattern),
}

impl Pattern {
    fn from_str(s: &str) -> Self {
        if let Some(unit) = UnitPattern::from_str(s) {
            Self::Unit(unit)
        } else if let Some(data) = DataPattern::from_str(s) {
            Self::Data(data)
        } else {
            eprintln!("invalid pattern: \"{}\"", s);
            std::process::exit(1);
        }
    }
}

#[derive(Clone, Debug)]
struct DataPattern {
    ty: DataPatternType,
    data: String,
}

impl DataPattern {
    fn from_str(s: &str) -> Option<Self> {
        for ty in DataPatternType::iter() {
            if let Some(caps) = ty.regex().captures(s) {
                if let Some(match_str) = caps.get(1) {
                    return Some(Self {
                        ty,
                        data: match_str.as_str().to_owned(),
                    });
                }
            }
        }
        None
    }
}

impl Evaluate for DataPattern {
    fn evaluate(&self, sources: DataSources<'_>) -> String {
        use chrono::TimeZone;
        use DataPatternType as Ty;
        let null = "<value not found>";

        match self.ty {
            Ty::CurrDate => {
                let now = utils::current_unix().as_secs() as i64;
                let formatting = &self.data;
                let datetime = chrono::Utc.timestamp_opt(now, 0).unwrap();

                if formatting == "unix" {
                    now.to_string()
                } else {
                    datetime.format(formatting).to_string()
                }
            }
            Ty::PubDate => {
                let episode = sources.episode;
                let formatting = &self.data;

                let datetime = chrono::Utc
                    .timestamp_opt(episode.published.as_secs() as i64, 0)
                    .unwrap();

                if formatting == "unix" {
                    episode.published.as_secs().to_string()
                } else {
                    datetime.format(formatting).to_string()
                }
            }
            Ty::RssEpisode => {
                let episode = sources.episode;
                let key = &self.data;

                let key = key.replace(":", utils::NAMESPACE_ALTER);
                episode.get_text_value(&key).unwrap_or(null).to_string()
            }
            Ty::RssChannel => {
                let channel = sources.podcast;
                let key = &self.data;

                let key = key.replace(":", utils::NAMESPACE_ALTER);
                channel.get_text_attribute(&key).unwrap_or(null).to_string()
            }
        }
    }
}

#[derive(Clone, Debug, EnumIter)]
enum DataPatternType {
    RssEpisode,
    RssChannel,
    PubDate,
    CurrDate,
}

impl DataPatternType {
    fn regex(&self) -> Regex {
        let s = match self {
            Self::PubDate => "pubdate",
            Self::CurrDate => "currdate",
            Self::RssEpisode => "rss::episode",
            Self::RssChannel => "rss::channel",
        };

        let s = format!("{}::(.+)", s);

        Regex::new(&s).unwrap()
    }
}

#[derive(Clone, Debug)]
enum UnitPattern {
    Guid,
    Url,
    PodName,
    AppName,
    Home,
}

impl UnitPattern {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "guid" => Self::Guid,
            "url" => Self::Url,
            "podname" => Self::PodName,
            "appname" => Self::AppName,
            "home" => Self::Home,
            _ => return None,
        }
        .into()
    }
}

impl Evaluate for UnitPattern {
    fn evaluate(&self, sources: DataSources<'_>) -> String {
        match self {
            Self::Guid => sources.episode.guid.to_string(),
            Self::Url => sources.episode.url.to_string(),
            Self::PodName => sources.podcast.name().to_string(),
            Self::AppName => crate::APPNAME.to_string(),
            Self::Home => home(),
        }
    }
}

fn home() -> String {
    dirs::home_dir()
        .unwrap()
        .as_os_str()
        .to_str()
        .unwrap()
        .to_owned()
}

pub trait Evaluate {
    fn evaluate(&self, sources: DataSources<'_>) -> String;
}

impl Evaluate for FullPattern {
    fn evaluate(&self, sources: DataSources<'_>) -> String {
        let mut output = String::new();

        for segment in &self.0 {
            let text = match segment {
                Segment::Text(text) => text.clone(),
                Segment::Pattern(Pattern::Unit(pattern)) => pattern.evaluate(sources),
                Segment::Pattern(Pattern::Data(pattern)) => pattern.evaluate(sources),
            };
            output.push_str(&text);
        }

        output
    }
}
