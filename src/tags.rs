use crate::cache;
use crate::episode::DownloadedEpisode;
use chrono::Datelike;
use id3::frame::PictureType;
use id3::TagLike;

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

fn has_picture_type(tag: &id3::Tag, ty: id3::frame::PictureType) -> bool {
    tag.pictures().any(|pic| pic.picture_type == ty)
}

use crate::podcast::Podcast;
pub async fn set_mp3_tags<'a>(podcast: &Podcast, episode: &DownloadedEpisode<'a>) -> id3::Tag {
    let custom_tags = &episode.inner().config.id3_tags;
    let file_path = &episode.path();
    let episode = &episode.inner();

    let mut tags = id3::Tag::read_from_path(file_path).unwrap_or_default();

    for (id, value) in custom_tags {
        tags.set_text(id, value);
    }

    if tags.title().is_none() {
        tags.set_title(&episode.title);
    }

    if tags.artist().is_none() {
        if let Some(author) = episode.author() {
            tags.set_artist(author);
        }
    }

    if tags.album().is_none() {
        tags.set_album(podcast.title());
    }

    if tags.genre().is_none() {
        tags.set_genre("podcast");
    }

    if tags.track().is_none() {
        if let Some(episode) = episode.itunes_episode() {
            if let Ok(episode) = episode.parse::<u32>() {
                tags.set_track(episode);
            }
        }
    }

    if tags.year().is_none() {
        let year = chrono::DateTime::from_timestamp(episode.published.as_secs() as i64, 0)
            .unwrap()
            .year();
        tags.set_year(year);
    }

    if tags.get(Id3Tag::COPYRIGHT).is_none() {
        if let Some(desc) = podcast.copyright() {
            tags.set_text(Id3Tag::COPYRIGHT, desc);
        }
    }

    if tags.get(Id3Tag::DESCRIPTION).is_none() {
        if let Some(desc) = episode.description() {
            tags.set_text(Id3Tag::DESCRIPTION, desc);
        }
    }

    if !has_picture_type(&tags, id3::frame::PictureType::CoverFront) {
        if let Some(img_url) = episode.image().or(podcast.image()) {
            if let Some(frame) = cache::get_image(img_url, PictureType::CoverFront).await {
                tags.add_frame(frame);
            }
        }
    }

    if tags.get(Id3Tag::PODCASTCATEGORY).is_none() {
        let mut strs = vec![];
        for cat in podcast.categories() {
            strs.push(cat);
        }

        tags.set_text_values(Id3Tag::PODCASTCATEGORY, strs);
    }

    if tags.date_released().is_none() {
        use chrono::TimeZone;
        use chrono::Timelike;
        let datetime = chrono::Utc
            .timestamp_opt(episode.published.as_secs() as i64, 0)
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
    }

    if tags.get(Id3Tag::LANGUAGE).is_none() {
        if let Some(language) = podcast.language() {
            tags.set_text(Id3Tag::LANGUAGE, language);
        }
    }

    if tags.get(Id3Tag::DURATION).is_none() {
        if let Some(dur) = episode.itunes_duration() {
            if let Ok(secs) = dur.parse::<u32>() {
                let millis = secs * 1000;
                tags.set_text(Id3Tag::DURATION, millis.to_string());
            }
        }
    }

    if tags.get(Id3Tag::PUBLISHER).is_none() {
        if let Some(author) = podcast.author() {
            tags.set_text(Id3Tag::PUBLISHER, author);
        }
    }

    if tags.get(Id3Tag::PODCAST_ID).is_none() {
        tags.set_text(Id3Tag::PODCAST_ID, &episode.guid);
    }

    tags.write_to_path(file_path, id3::Version::Id3v24).unwrap();

    tags
}
