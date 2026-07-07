use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufReader, Read, Seek},
    path::Path,
};

use nom_exif::{
    EntryValue, Exif, ExifDateTime, ExifTag, IfdIndex, ImageFormatMetadata, MediaParser,
    MediaSource,
};
use tiff::decoder::Decoder as TiffDecoder;
use tiff::tags::Tag as TiffTag;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::types::ImageMediaMetadata;

mod tiff_fallback;

const EXIF_ARTIST_TAG_CODE: u16 = 0x013b;

pub(super) fn parse_image_metadata_from_path(path: &Path) -> Result<ImageMediaMetadata> {
    parse_image_metadata_with_reader_factory(&path.display().to_string(), || {
        File::open(path).map_aster_err_ctx(
            "open image metadata source",
            AsterError::storage_driver_error,
        )
    })
}

pub(super) fn parse_image_metadata_with_reader_factory<R, F>(
    source_label: &str,
    mut make_reader: F,
) -> Result<ImageMediaMetadata>
where
    R: Read + Seek,
    F: FnMut() -> Result<R>,
{
    let mut metadata = ImageMediaMetadata {
        width: 0,
        height: 0,
        format: None,
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
        gps_latitude: None,
        gps_longitude: None,
        gps_altitude_meters: None,
        artist: None,
        copyright: None,
        software: None,
    };

    match parse_nom_exif_image_metadata_from_reader(make_reader()?) {
        Ok(image_metadata) => enrich_image_metadata_from_nom_exif(&image_metadata, &mut metadata),
        Err(error) => {
            tracing::debug!(
                source = source_label,
                error = %error,
                "image metadata unavailable from nom-exif, falling back to image dimensions"
            );

            match tiff_fallback::enrich_image_metadata_from_reader(make_reader()?, &mut metadata) {
                Ok(()) => {}
                Err(error) => {
                    tracing::debug!(
                        source = source_label,
                        error = %error,
                        "image EXIF metadata unavailable from TIFF fallback"
                    );
                }
            }
        }
    }

    match tiff_directory_dimensions_from_reader(make_reader()?) {
        Ok(Some((width, height)))
            if dimensions_area(width, height)
                > dimensions_area(metadata.width, metadata.height) =>
        {
            metadata.width = width;
            metadata.height = height;
        }
        Ok(_) => {}
        Err(error) => {
            tracing::debug!(
                source = source_label,
                error = %error,
                "image dimensions unavailable from TIFF directory metadata"
            );
        }
    }

    if metadata.width == 0 || metadata.height == 0 {
        let (width, height, format) = image_reader_metadata_from_reader(make_reader()?)?;
        metadata.width = width;
        metadata.height = height;
        metadata.format = metadata.format.take().or(format);
    }

    Ok(metadata)
}

fn parse_nom_exif_image_metadata_from_reader<R>(reader: R) -> Result<nom_exif::ImageMetadata<Exif>>
where
    R: Read + Seek,
{
    let mut parser = MediaParser::new();
    let source = MediaSource::seekable(reader)
        .map_aster_err_ctx("open image metadata source", AsterError::validation_error)?;
    let image_metadata = parser
        .parse_image_metadata(source)
        .map_aster_err_ctx("parse image metadata", AsterError::validation_error)?;
    Ok(image_metadata.into())
}

fn image_reader_metadata_from_reader<R>(reader: R) -> Result<(u32, u32, Option<String>)>
where
    R: Read + Seek,
{
    let reader = image::ImageReader::new(BufReader::new(reader));
    let reader = reader
        .with_guessed_format()
        .map_aster_err_ctx("guess image metadata format", AsterError::validation_error)?;
    let format = reader
        .format()
        .map(|format| format.to_mime_type().to_string());
    let (width, height) = reader
        .into_dimensions()
        .map_aster_err_ctx("read image dimensions", AsterError::validation_error)?;
    Ok((width, height, format))
}

fn enrich_image_metadata_from_nom_exif(
    image_metadata: &nom_exif::ImageMetadata<Exif>,
    metadata: &mut ImageMediaMetadata,
) {
    if let Some(exif) = image_metadata.exif.as_ref() {
        if let Some((width, height)) = exif_dimensions(exif) {
            metadata.width = width;
            metadata.height = height;
        }
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
        if let Some(gps) = exif.gps_info() {
            metadata.gps_latitude = gps.latitude_decimal().filter(|value| value.is_finite());
            metadata.gps_longitude = gps.longitude_decimal().filter(|value| value.is_finite());
            metadata.gps_altitude_meters = gps.altitude_meters().filter(|value| value.is_finite());
        }
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
}

fn exif_entry(exif: &Exif, tag: ExifTag) -> Option<&EntryValue> {
    exif.get(tag).or_else(|| {
        exif.iter()
            .find_map(|entry| (entry.tag.tag() == Some(tag)).then_some(entry.value))
    })
}

fn exif_entry_by_code(exif: &Exif, code: u16) -> Option<&EntryValue> {
    exif.get_by_code(IfdIndex::MAIN, code).or_else(|| {
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

fn tiff_directory_dimensions_from_reader<R>(
    reader: R,
) -> std::result::Result<Option<(u32, u32)>, tiff::TiffError>
where
    R: Read + Seek,
{
    let mut decoder = TiffDecoder::new(BufReader::new(reader))?;
    let mut best = Some(decoder.dimensions()?);

    if let Some(value) = decoder.find_tag(TiffTag::SubIfd)?
        && let Ok(sub_ifds) = value.into_ifd_vec()
    {
        for sub_ifd in sub_ifds {
            let directory = decoder.read_directory(sub_ifd)?;
            let mut tags = decoder.read_directory_tags(&directory);
            let width = tags.find_tag_unsigned::<u32>(TiffTag::ImageWidth)?;
            let height = tags.find_tag_unsigned::<u32>(TiffTag::ImageLength)?;
            if let Some(dimensions) = width.zip(height) {
                best = Some(larger_dimensions(best, dimensions));
            }
        }
    }

    while decoder.more_images() {
        decoder.next_image()?;
        best = Some(larger_dimensions(best, decoder.dimensions()?));
    }

    Ok(best.filter(|(width, height)| *width > 0 && *height > 0))
}

fn exif_dimensions(exif: &Exif) -> Option<(u32, u32)> {
    let mut by_ifd = BTreeMap::<IfdIndex, (Option<u32>, Option<u32>)>::new();
    for entry in exif.iter() {
        let Some(tag) = entry.tag.tag() else {
            continue;
        };
        let Some(value) = entry_u32(entry.value) else {
            continue;
        };
        match tag {
            ExifTag::ExifImageWidth | ExifTag::ImageWidth => {
                by_ifd.entry(entry.ifd).or_default().0 = Some(value);
            }
            ExifTag::ExifImageHeight | ExifTag::ImageHeight => {
                by_ifd.entry(entry.ifd).or_default().1 = Some(value);
            }
            _ => {}
        }
    }

    by_ifd
        .into_values()
        .filter_map(|(width, height)| Some((width?, height?)))
        .filter(|(width, height)| *width > 0 && *height > 0)
        .max_by_key(|(width, height)| u64::from(*width) * u64::from(*height))
}

fn larger_dimensions(current: Option<(u32, u32)>, candidate: (u32, u32)) -> (u32, u32) {
    let Some(current) = current else {
        return candidate;
    };
    if dimensions_area(candidate.0, candidate.1) > dimensions_area(current.0, current.1) {
        candidate
    } else {
        current
    }
}

fn dimensions_area(width: u32, height: u32) -> u64 {
    u64::from(width) * u64::from(height)
}

fn exif_float(exif: &Exif, tag: ExifTag) -> Option<f64> {
    let value = exif_entry(exif, tag)?.try_as_float()?;
    value.is_finite().then_some(value)
}

fn entry_u16(value: &EntryValue) -> Option<u16> {
    value
        .as_u16()
        .or_else(|| {
            value
                .as_u16_slice()
                .and_then(|values| values.first().copied())
        })
        .or_else(|| {
            value
                .try_as_integer()
                .and_then(|value| u16::try_from(value).ok())
        })
}

fn entry_u32(value: &EntryValue) -> Option<u32> {
    value
        .as_u32()
        .or_else(|| {
            value
                .as_u32_slice()
                .and_then(|values| values.first().copied())
        })
        .or_else(|| entry_u16(value).map(u32::from))
        .or_else(|| {
            value
                .try_as_integer()
                .and_then(|value| u32::try_from(value).ok())
        })
}

fn exif_u16(exif: &Exif, tag: ExifTag) -> Option<u16> {
    exif_entry(exif, tag).and_then(entry_u16)
}

fn exif_u32(exif: &Exif, tag: ExifTag) -> Option<u32> {
    exif_entry(exif, tag).and_then(entry_u32)
}

fn exif_datetime(exif: &Exif, tag: ExifTag) -> Option<String> {
    match exif_entry(exif, tag)?.as_datetime()? {
        ExifDateTime::Aware(value) => Some(value.to_rfc3339()),
        ExifDateTime::Naive(value) => Some(value.format("%Y-%m-%dT%H:%M:%S").to_string()),
    }
}
