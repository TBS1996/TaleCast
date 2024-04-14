use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::episode::Episode;
use crate::podcast::Podcast;
use crate::utils;

use regex::Regex;

#[derive(Debug, Clone)]
pub struct FullPattern(Vec<Segment>);

impl FullPattern {
    pub fn from_str(s: &str, available_sources: Vec<SourceType>) -> Self {
        let mut segments: Vec<Segment> = vec![];
        let mut text = String::new();
        let mut pattern = String::new();

        let mut is_inside = false;

        for c in s.chars() {
            if c == '}' {
                assert!(is_inside);
                let text_pattern = std::mem::take(&mut pattern);
                let pattern = Pattern::from_str(&text_pattern);
                if let Some(required_source) = pattern.required_source() {
                    if !available_sources.contains(&required_source) {
                        eprintln!("CONFIGURATION ERROR");
                        eprintln!(
                            "invalid pattern: {}\n{:?} requires the \"{:?}\"-source which is not available for this configuration setting.",
                            s, text_pattern, required_source
                        );

                        std::process::exit(1);
                    }
                }
                let segment = Segment::Pattern(pattern);
                segments.push(segment);
                is_inside = false;
            } else if c == '{' {
                assert!(!is_inside);
                let text = std::mem::take(&mut text);
                let segment = Segment::Text(text);
                segments.push(segment);
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

#[derive(PartialEq, Debug)]
pub enum SourceType {
    Episode,
    Podcast,
    Id3,
}

impl SourceType {
    pub fn all() -> Vec<Self> {
        vec![Self::Episode, Self::Podcast, Self::Id3]
    }
}

#[derive(Default, Clone, Copy)]
pub struct DataSources<'a> {
    id3: Option<&'a id3::Tag>,
    episode: Option<&'a Episode<'a>>,
    podcast: Option<&'a Podcast>,
}

impl<'a> DataSources<'a> {
    fn id3(&self) -> &'a id3::Tag {
        self.id3.unwrap()
    }

    fn episode(&self) -> &'a Episode<'a> {
        self.episode.unwrap()
    }

    fn podcast(&self) -> &'a Podcast {
        self.podcast.unwrap()
    }

    pub fn set_episode(mut self, episode: &'a Episode<'a>) -> Self {
        self.episode = Some(episode);
        self
    }

    pub fn set_podcast(mut self, podcast: &'a Podcast) -> Self {
        self.podcast = Some(podcast);
        self
    }

    pub fn set_id3(mut self, id3: &'a id3::Tag) -> Self {
        self.id3 = Some(id3);
        self
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

    fn required_source(&self) -> Option<SourceType> {
        use DataPattern as DP;
        use UnitPattern as UP;

        match self {
            Self::Unit(UP::Home) => None,
            Self::Unit(UP::AppName) => None,
            Self::Unit(UP::PodName) => Some(SourceType::Podcast),
            Self::Unit(UP::Url) => Some(SourceType::Episode),
            Self::Unit(UP::Guid) => Some(SourceType::Episode),
            Self::Data(DP {
                ty: DataPatternType::RssChannel,
                ..
            }) => Some(SourceType::Podcast),
            Self::Data(DP {
                ty: DataPatternType::RssEpisode,
                ..
            }) => Some(SourceType::Episode),
            Self::Data(DP {
                ty: DataPatternType::Id3Tag,
                ..
            }) => Some(SourceType::Id3),
            Self::Data(DP {
                ty: DataPatternType::PubDate,
                ..
            }) => Some(SourceType::Episode),
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
            Ty::PubDate => {
                let episode = sources.episode();
                let formatting = &self.data;

                let datetime = chrono::Utc.timestamp_opt(episode.published, 0).unwrap();

                if formatting == "unix" {
                    episode.published.to_string()
                } else {
                    datetime.format(formatting).to_string()
                }
            }
            Ty::RssEpisode => {
                let episode = sources.episode();
                let key = &self.data;

                let key = key.replace(":", utils::NAMESPACE_ALTER);
                episode.get_text_value(&key).unwrap_or(null).to_string()
            }
            Ty::RssChannel => {
                let channel = sources.podcast();
                let key = &self.data;

                let key = key.replace(":", utils::NAMESPACE_ALTER);
                channel.get_text_attribute(&key).unwrap_or(null).to_string()
            }
            Ty::Id3Tag => {
                use id3::TagLike;

                let tag_key = &self.data;
                sources
                    .id3()
                    .get(tag_key)
                    .and_then(|tag| tag.content().text())
                    .unwrap_or(null)
                    .to_string()
            }
        }
    }
}

#[derive(Clone, Debug, EnumIter)]
enum DataPatternType {
    RssEpisode,
    RssChannel,
    PubDate,
    Id3Tag,
}

impl DataPatternType {
    fn regex(&self) -> Regex {
        let s = match self {
            Self::Id3Tag => "id3",
            Self::PubDate => "pubdate",
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
            Self::Guid => sources.episode().guid.to_string(),
            Self::Url => sources.episode().url.to_string(),
            Self::PodName => sources.podcast().name().to_string(),
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
