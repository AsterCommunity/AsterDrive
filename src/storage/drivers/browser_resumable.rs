//! Shared mechanics for sequential provider sessions. Provider wire parsing stays with its driver.
use aster_drive_storage::{
    BrowserUploadContext, BrowserUploadMethod, BrowserUploadRange, BrowserUploadRequest,
    BrowserUploadTransition, Result, StorageErrorKind, storage_driver_error,
};
use std::collections::BTreeMap;

pub(super) fn invalid() -> aster_drive_storage::StorageError {
    storage_driver_error(
        StorageErrorKind::Precondition,
        "invalid provider browser upload state",
    )
}

pub(super) fn ranges_offset(ranges: &[String], total: u64) -> Result<u64> {
    if ranges.is_empty() {
        return Ok(total);
    }
    ranges
        .iter()
        .map(|range| {
            let (start, end) = range.split_once('-').ok_or_else(invalid)?;
            if start.is_empty() || !start.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid());
            }
            let start: u64 = start.parse().map_err(|_| invalid())?;
            if start >= total {
                return Err(invalid());
            }
            if !end.is_empty() {
                if !end.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(invalid());
                }
                let end: u64 = end.parse().map_err(|_| invalid())?;
                if end < start || end >= total {
                    return Err(invalid());
                }
            }
            Ok(start)
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .min()
        .ok_or_else(invalid)
}

pub(super) fn transition(
    context: BrowserUploadContext<'_>,
    offset: u64,
    completed: bool,
    response_headers: Vec<String>,
) -> Result<BrowserUploadTransition> {
    if offset > context.total_size
        || completed != (offset == context.total_size)
        || context.fragment_size == 0
    {
        return Err(invalid());
    }
    let request = if completed {
        None
    } else {
        let end = offset
            .saturating_add(context.fragment_size)
            .min(context.total_size);
        Some(BrowserUploadRequest {
            url: context.upload_url.into(),
            method: BrowserUploadMethod::Put,
            headers: BTreeMap::from([
                ("Content-Type".into(), "application/octet-stream".into()),
                (
                    "Content-Range".into(),
                    format!("bytes {}-{}/{}", offset, end - 1, context.total_size),
                ),
            ]),
            body: Some(BrowserUploadRange { start: offset, end }),
            response_headers,
        })
    };
    Ok(BrowserUploadTransition {
        private_state: serde_json::Value::Null,
        confirmed_offset: offset,
        provider_completed: completed,
        request,
    })
}

pub(super) fn checked_ack(
    offset: u64,
    completed: bool,
    range: BrowserUploadRange,
    total: u64,
) -> Option<(u64, bool)> {
    (offset >= range.start
        && offset <= range.end
        && offset <= total
        && if completed {
            range.end == total && offset == total
        } else {
            offset < total
        })
    .then_some((offset, completed))
}

pub(super) fn check_response(
    receipt: Option<&aster_drive_storage::BrowserUploadReceipt<'_>>,
) -> Result<()> {
    if let Some(receipt) = receipt {
        let kind = match receipt.observation.status {
            401 => Some(StorageErrorKind::Auth),
            403 => Some(StorageErrorKind::Permission),
            400..=499 if !matches!(receipt.observation.status, 404 | 408 | 410 | 416 | 429) => {
                Some(StorageErrorKind::Precondition)
            }
            _ => None,
        };
        if let Some(kind) = kind {
            return Err(storage_driver_error(
                kind,
                "provider rejected browser upload request",
            ));
        }
    }
    Ok(())
}
