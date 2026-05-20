use std::path::Path;

use image::ImageReader;
use nom_exif::{
    EntryValue, Exif, ExifDateTime, ExifTag, IfdIndex, ImageFormatMetadata, MediaParser,
    MediaSource,
};

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::types::ImageMediaMetadata;

const EXIF_ARTIST_TAG_CODE: u16 = 0x013b;

pub(super) fn parse_image_metadata_from_path(path: &Path) -> Result<ImageMediaMetadata> {
    let reader = ImageReader::open(path).map_aster_err_ctx(
        "open image metadata source",
        AsterError::storage_driver_error,
    )?;
    let reader = reader
        .with_guessed_format()
        .map_aster_err_ctx("guess image metadata format", AsterError::validation_error)?;
    let format = reader
        .format()
        .map(|format| format.to_mime_type().to_string());
    let (width, height) = reader
        .into_dimensions()
        .map_aster_err_ctx("read image dimensions", AsterError::validation_error)?;

    let mut metadata = ImageMediaMetadata {
        width,
        height,
        format,
        camera_make: None,
        camera_model: None,
        lens_make: None,
        lens_model: None,
        f_number: None,
        exposure_time_seconds: None,
        iso: None,
        exposure_bias_ev: None,
        flash_fired: None,
        flash_mode: None,
        focal_length_mm: None,
        focal_length_35mm: None,
        taken_at: None,
        orientation: None,
        artist: None,
        copyright: None,
        software: None,
    };

    if let Err(error) = enrich_image_metadata_from_exif(path, &mut metadata) {
        tracing::debug!(
            path = %path.display(),
            error = %error,
            "image exif metadata unavailable"
        );
    }

    Ok(metadata)
}

fn enrich_image_metadata_from_exif(path: &Path, metadata: &mut ImageMediaMetadata) -> Result<()> {
    let mut parser = MediaParser::new();
    let source = MediaSource::open(path)
        .map_aster_err_ctx("open image exif source", AsterError::validation_error)?;
    let image_metadata = parser
        .parse_image_metadata(source)
        .map_aster_err_ctx("parse image exif metadata", AsterError::validation_error)?;
    let image_metadata: nom_exif::ImageMetadata<Exif> = image_metadata.into();

    if let Some(exif) = image_metadata.exif.as_ref() {
        metadata.camera_make = exif_text(exif, ExifTag::Make);
        metadata.camera_model = exif_text(exif, ExifTag::Model);
        metadata.lens_make = exif_text(exif, ExifTag::LensMake);
        metadata.lens_model = exif_text(exif, ExifTag::LensModel);
        metadata.f_number = exif_float(exif, ExifTag::FNumber);
        metadata.exposure_time_seconds = exif_float(exif, ExifTag::ExposureTime);
        metadata.iso = exif_u32(exif, ExifTag::ISOSpeedRatings);
        metadata.exposure_bias_ev = exif_float(exif, ExifTag::ExposureBiasValue);
        metadata.flash_mode = exif_u16(exif, ExifTag::Flash);
        metadata.flash_fired = metadata.flash_mode.map(|mode| mode & 1 == 1);
        metadata.focal_length_mm = exif_float(exif, ExifTag::FocalLength);
        metadata.focal_length_35mm = exif_u32(exif, ExifTag::FocalLengthIn35mmFilm);
        metadata.taken_at = exif_datetime(exif, ExifTag::DateTimeOriginal)
            .or_else(|| exif_datetime(exif, ExifTag::CreateDate))
            .or_else(|| exif_datetime(exif, ExifTag::ModifyDate));
        metadata.orientation = exif_u16(exif, ExifTag::Orientation);
        metadata.artist = exif_text_by_code(exif, EXIF_ARTIST_TAG_CODE);
        metadata.copyright = exif_text(exif, ExifTag::Copyright);
        metadata.software = exif_text(exif, ExifTag::Software);
    }

    if let Some(ImageFormatMetadata::Png(chunks)) = image_metadata.format.as_ref() {
        metadata.artist = metadata
            .artist
            .take()
            .or_else(|| clean_metadata_string(chunks.get("Author")));
        metadata.copyright = metadata
            .copyright
            .take()
            .or_else(|| clean_metadata_string(chunks.get("Copyright")));
        metadata.software = metadata
            .software
            .take()
            .or_else(|| clean_metadata_string(chunks.get("Software")));
    }

    Ok(())
}

fn exif_entry<'a>(exif: &'a Exif, tag: ExifTag) -> Option<&'a EntryValue> {
    exif.get(tag).or_else(|| {
        exif.iter()
            .find_map(|entry| (entry.tag.tag() == Some(tag)).then_some(entry.value))
    })
}

fn exif_entry_by_code(exif: &Exif, code: u16) -> Option<&EntryValue> {
    exif.get_by_code(IfdIndex::MAIN, code)
        .or_else(|| {
            exif.iter()
                .find_map(|entry| (entry.tag.code() == code).then_some(entry.value))
        })
}

fn clean_metadata_string(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim();
    if normalized.is_empty() {
        return None;
    }
    Some(normalized.to_string())
}

fn exif_text(exif: &Exif, tag: ExifTag) -> Option<String> {
    clean_metadata_string(exif_entry(exif, tag).and_then(EntryValue::as_str))
}

fn exif_text_by_code(exif: &Exif, code: u16) -> Option<String> {
    clean_metadata_string(exif_entry_by_code(exif, code).and_then(EntryValue::as_str))
}

fn exif_float(exif: &Exif, tag: ExifTag) -> Option<f64> {
    let value = exif_entry(exif, tag)?.try_as_float()?;
    value.is_finite().then_some(value)
}

fn exif_u16(exif: &Exif, tag: ExifTag) -> Option<u16> {
    exif_entry(exif, tag)
        .and_then(EntryValue::as_u16)
        .or_else(|| {
            exif_entry(exif, tag)
                .and_then(EntryValue::as_u16_slice)
                .and_then(|values| values.first().copied())
        })
        .or_else(|| {
            exif_entry(exif, tag)
                .and_then(EntryValue::try_as_integer)
                .and_then(|value| u16::try_from(value).ok())
        })
}

fn exif_u32(exif: &Exif, tag: ExifTag) -> Option<u32> {
    exif_entry(exif, tag)
        .and_then(EntryValue::as_u32)
        .or_else(|| {
            exif_entry(exif, tag)
                .and_then(EntryValue::as_u32_slice)
                .and_then(|values| values.first().copied())
        })
        .or_else(|| exif_u16(exif, tag).map(u32::from))
        .or_else(|| {
            exif_entry(exif, tag)
                .and_then(EntryValue::try_as_integer)
                .and_then(|value| u32::try_from(value).ok())
        })
}

fn exif_datetime(exif: &Exif, tag: ExifTag) -> Option<String> {
    match exif_entry(exif, tag)?.as_datetime()? {
        ExifDateTime::Aware(value) => Some(value.to_rfc3339()),
        ExifDateTime::Naive(value) => Some(value.format("%Y-%m-%dT%H:%M:%S").to_string()),
    }
}
