use actix_web::http::header::HeaderValue;

use crate::errors::{AsterError, Result};
use crate::utils::numbers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DownloadRangeRequest {
    pub(crate) start: u64,
    pub(crate) length: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedDownloadRange {
    pub(crate) start: u64,
    pub(crate) end: u64,
    pub(crate) length: u64,
    pub(crate) total_size: u64,
}

impl ResolvedDownloadRange {
    pub fn new(start: u64, end: u64, total_size: u64) -> Result<Self> {
        if total_size == 0 {
            return Err(AsterError::validation_error(
                "download range total size must be greater than zero",
            ));
        }
        if start > end {
            return Err(AsterError::validation_error(
                "download range end must be greater than or equal to start",
            ));
        }
        if end >= total_size {
            return Err(AsterError::validation_error(
                "download range end must be smaller than total size",
            ));
        }
        Ok(Self {
            start,
            end,
            length: end - start + 1,
            total_size,
        })
    }

    pub fn start(&self) -> u64 {
        self.start
    }

    pub fn end(&self) -> u64 {
        self.end
    }

    pub fn length(&self) -> u64 {
        self.length
    }

    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    pub fn content_range_header(&self) -> String {
        format!("bytes {}-{}/{}", self.start, self.end, self.total_size)
    }
}

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
    let range = raw
        .strip_prefix("bytes=")
        .ok_or_else(|| AsterError::validation_error("range header must use bytes unit"))?;
    if range.contains(',') {
        return Err(AsterError::validation_error(
            "multiple range requests are not supported",
        ));
    }

    let (start_raw, end_raw) = range
        .split_once('-')
        .ok_or_else(|| AsterError::validation_error("range header is malformed"))?;
    if start_raw.is_empty() && end_raw.is_empty() {
        return Err(AsterError::validation_error("range header is malformed"));
    }
    if total_size == 0 {
        return Err(AsterError::validation_error(
            "range cannot be requested for empty file",
        ));
    }

    let requested = if start_raw.is_empty() {
        let suffix_length = parse_range_bound(end_raw, "range suffix length")?;
        if suffix_length == 0 {
            return Err(AsterError::validation_error(
                "range suffix length must be greater than zero",
            ));
        }
        let length = suffix_length.min(total_size);
        DownloadRangeRequest {
            start: total_size - length,
            length,
        }
    } else {
        let start = parse_range_bound(start_raw, "range start")?;
        if start >= total_size {
            return Err(AsterError::validation_error(
                "range offset exceeds file size",
            ));
        }
        let end = if end_raw.is_empty() {
            total_size - 1
        } else {
            parse_range_bound(end_raw, "range end")?
        };
        if end < start {
            return Err(AsterError::validation_error(
                "range end must be greater than or equal to range start",
            ));
        }
        let clamped_end = end.min(total_size - 1);
        DownloadRangeRequest {
            start,
            length: clamped_end - start + 1,
        }
    };

    ResolvedDownloadRange::new(
        requested.start,
        requested.start + requested.length - 1,
        total_size,
    )
    .map(Some)
}

fn parse_range_bound(value: &str, name: &str) -> Result<u64> {
    value
        .parse::<u64>()
        .map_err(|_| AsterError::validation_error(format!("{name} must be a valid integer")))
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
