use crate::episode::EpisodeAttributes;
use crate::podcast::RawPodcast;
use chrono::Datelike;
use id3::TagLike;

pub async fn extract_tags_from_raw(
    podcast: &RawPodcast,
    episode: &EpisodeAttributes,
) -> Option<id3::Tag> {
    let mut tags = id3::Tag::new();

    tags.set_title(episode.title());

    if let Some(author) = episode.author() {
        tags.set_artist(author);
    }

    tags.set_album(podcast.title());

    tags.set_genre("podcast");

    if let Some(episode) = episode.itunes_episode() {
        if let Ok(episode) = episode.parse::<u32>() {
            tags.set_track(episode);
        }
    }

    let year = chrono::DateTime::from_timestamp(episode.published().as_secs() as i64, 0)
        .unwrap()
        .year();
    tags.set_year(year);

    if let Some(desc) = podcast.copyright() {
        tags.set_text(Id3Tag::COPYRIGHT, desc);
    }

    if let Some(desc) = episode.description() {
        tags.set_text(Id3Tag::DESCRIPTION, desc);
    }

    let mut strs = vec![];
    for cat in podcast.categories() {
        strs.push(cat);
    }

    tags.set_text_values(Id3Tag::PODCASTCATEGORY, strs);

    use chrono::TimeZone;
    use chrono::Timelike;
    let datetime = chrono::Utc
        .timestamp_opt(episode.published().as_secs() as i64, 0)
        .unwrap();

    let ts = id3::frame::Timestamp {
        year: datetime.year(),
        month: Some(datetime.month() as u8),
        day: Some(datetime.day() as u8),
        hour: Some(datetime.hour() as u8),
        minute: Some(datetime.minute() as u8),
        second: Some(datetime.second() as u8),
    };

    tags.set_date_released(ts);

    if let Some(language) = podcast.language() {
        tags.set_text(Id3Tag::LANGUAGE, language);
    }

    if let Some(dur) = episode.itunes_duration() {
        if let Ok(secs) = dur.parse::<u32>() {
            let millis = secs * 1000;
            tags.set_text(Id3Tag::DURATION, millis.to_string());
        }
    }

    if let Some(author) = podcast.author() {
        tags.set_text(Id3Tag::PUBLISHER, author);
    }

    tags.set_text(Id3Tag::PODCAST_ID, episode.guid());

    Some(tags)
}

struct Id3Tag;

impl Id3Tag {
    const COPYRIGHT: &'static str = "TCOP";
    const DESCRIPTION: &'static str = "TDES";
    const PODCASTCATEGORY: &'static str = "TCAT";
    const LANGUAGE: &'static str = "TLAN";
    const DURATION: &'static str = "TLEN";
    const PUBLISHER: &'static str = "TPUB";
    const PODCAST_ID: &'static str = "TGID";
}
