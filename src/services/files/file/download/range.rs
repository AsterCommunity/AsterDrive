use actix_web::http::header::HeaderValue;
pub use aster_forge_utils::http_range::HttpByteRange as ResolvedDownloadRange;
use aster_forge_utils::http_range::parse_single_byte_range;

use crate::errors::{AsterError, Result};
use aster_forge_utils::numbers;

pub(crate) fn parse_range_header(
    range_header: Option<&HeaderValue>,
    total_size: i64,
) -> Result<Option<ResolvedDownloadRange>> {
    let Some(range_header) = range_header else {
        return Ok(None);
    };
    let total_size = numbers::i64_to_u64(total_size, "download range total size")?;
    let raw = range_header
        .to_str()
        .map_err(|_| AsterError::validation_error("range header must be valid ASCII"))?;
    parse_single_byte_range(raw, total_size)
        .map(Some)
        .map_err(|error| AsterError::validation_error(error.to_string()))
}

#[cfg(test)]
mod tests {
    use actix_web::http::header::HeaderValue;

    use super::parse_range_header;

    fn parse(raw: &str, total_size: i64) -> (u64, u64, u64, u64) {
        let header = HeaderValue::from_str(raw).unwrap();
        let range = parse_range_header(Some(&header), total_size)
            .unwrap()
            .expect("range should be parsed");
        (
            range.start(),
            range.end(),
            range.length(),
            range.total_size(),
        )
    }

    #[test]
    fn parses_bounded_ranges() {
        assert_eq!(parse("bytes=5-9", 20), (5, 9, 5, 20));
    }

    #[test]
    fn parses_open_ended_ranges() {
        assert_eq!(parse("bytes=7-", 20), (7, 19, 13, 20));
    }

    #[test]
    fn parses_suffix_ranges() {
        assert_eq!(parse("bytes=-6", 20), (14, 19, 6, 20));
        assert_eq!(parse("bytes=-50", 20), (0, 19, 20, 20));
    }

    #[test]
    fn clamps_end_beyond_file_size() {
        assert_eq!(parse("bytes=17-99", 20), (17, 19, 3, 20));
    }

    #[test]
    fn resolved_range_exposes_public_api() {
        let range = super::ResolvedDownloadRange::new(2, 6, 10).unwrap();

        assert_eq!(range.start(), 2);
        assert_eq!(range.end(), 6);
        assert_eq!(range.length(), 5);
        assert_eq!(range.total_size(), 10);
        assert_eq!(range.content_range_header(), "bytes 2-6/10");
    }

    #[test]
    fn resolved_range_constructor_rejects_invalid_ranges() {
        assert!(super::ResolvedDownloadRange::new(0, 0, 0).is_err());
        assert!(super::ResolvedDownloadRange::new(5, 4, 10).is_err());
        assert!(super::ResolvedDownloadRange::new(5, 10, 10).is_err());
    }

    #[test]
    fn rejects_ranges_for_empty_content() {
        for raw in ["bytes=0-0", "bytes=0-", "bytes=-1"] {
            let header = HeaderValue::from_static(raw);
            assert!(
                parse_range_header(Some(&header), 0).is_err(),
                "{raw} must be unsatisfiable for empty content"
            );
        }
    }

    #[test]
    fn rejects_malformed_ranges() {
        for raw in [
            "items=0-1",
            "bytes=0-1,3-4",
            "bytes=-",
            "bytes=-0",
            "bytes=9-5",
            "bytes=20-",
        ] {
            let header = HeaderValue::from_str(raw).unwrap();
            assert!(
                parse_range_header(Some(&header), 20).is_err(),
                "{raw} should be rejected"
            );
        }
    }
}
