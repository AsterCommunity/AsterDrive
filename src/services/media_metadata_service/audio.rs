use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::prelude::Accessor;
use lofty::probe::Probe;
use lofty::tag::{ItemKey, Tag};

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::types::AudioMediaMetadata;

pub(super) fn parse_audio_metadata_from_path(path: &Path) -> Result<AudioMediaMetadata> {
    let mut options = ParseOptions::new();
    options = options.read_cover_art(true);
    let tagged_file = Probe::open(path)
        .map_aster_err_ctx(
            "open audio metadata source",
            AsterError::storage_driver_error,
        )?
        .guess_file_type()
        .map_aster_err_ctx("guess audio metadata format", AsterError::validation_error)?
        .options(options)
        .read()
        .map_aster_err_ctx("read audio metadata", AsterError::validation_error)?;
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());
    let properties = tagged_file.properties();
    let picture = tag.and_then(|tag| tag.pictures().first());

    Ok(AudioMediaMetadata {
        title: tag.and_then(Accessor::title).map(clean_tag_text),
        artist: tag.and_then(Accessor::artist).map(clean_tag_text),
        artists: tag.map(track_artists).unwrap_or_default(),
        album: tag.and_then(Accessor::album).map(clean_tag_text),
        album_artist: tag
            .and_then(|tag| tag.get_string(ItemKey::AlbumArtist))
            .map(clean_tag_text),
        duration_ms: duration_ms(properties.duration()),
        sample_rate: properties.sample_rate(),
        channels: properties.channels(),
        bit_depth: properties.bit_depth(),
        overall_bitrate: properties.overall_bitrate(),
        audio_bitrate: properties.audio_bitrate(),
        track_number: tag.and_then(Accessor::track),
        track_total: tag.and_then(Accessor::track_total),
        disc_number: tag.and_then(Accessor::disk),
        disc_total: tag.and_then(Accessor::disk_total),
        genre: tag.and_then(Accessor::genre).map(clean_tag_text),
        date: tag
            .and_then(Accessor::date)
            .map(|timestamp| timestamp.to_string()),
        has_embedded_picture: picture.is_some(),
        embedded_picture_mime_type: picture
            .and_then(|picture| picture.mime_type())
            .map(|mime| mime.as_str().to_string()),
    })
}

fn track_artists(tag: &Tag) -> Vec<String> {
    let artists = tag
        .get_strings(ItemKey::TrackArtists)
        .map(clean_tag_text)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if !artists.is_empty() {
        return artists;
    }

    tag.artist()
        .map(clean_tag_text)
        .filter(|value| !value.is_empty())
        .into_iter()
        .collect()
}

fn clean_tag_text(value: impl AsRef<str>) -> String {
    value.as_ref().trim().to_string()
}

fn duration_ms(duration: std::time::Duration) -> Option<u64> {
    if duration.is_zero() {
        return None;
    }
    u64::try_from(duration.as_millis()).ok()
}
