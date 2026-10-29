use super::GoogleDriveDriver;
use crate::storage::drivers::browser_resumable::{checked_ack, ranges_offset, transition};
use aster_drive_storage::{
    BrowserUploadContext, BrowserUploadDriver, BrowserUploadProtocol, BrowserUploadReceipt,
    BrowserUploadTransition, ProviderResumableUploadDriver, Result,
};

fn acknowledgement(receipt: &BrowserUploadReceipt<'_>, total: u64) -> Option<(u64, bool)> {
    let range = receipt.request.body?;
    match receipt.observation.status {
        200 | 201 => checked_ack(total, true, range, total),
        308 => {
            let range_header = receipt.observation.headers.get("range")?;
            let end = range_header.strip_prefix("bytes=0-")?;
            if end.is_empty() || !end.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let offset = end.parse::<u64>().ok()?.checked_add(1)?;
            checked_ack(offset, false, range, total)
        }
        _ => None,
    }
}

#[async_trait::async_trait]
impl BrowserUploadDriver for GoogleDriveDriver {
    fn protocol(&self) -> BrowserUploadProtocol {
        aster_drive_storage::BrowserUploadProtocol {
            id: "asterdrive.google_drive.resumable".into(),
            version: 1,
        }
    }
    async fn advance(
        &self,
        context: BrowserUploadContext<'_>,
        _: serde_json::Value,
        receipt: Option<BrowserUploadReceipt<'_>>,
    ) -> Result<BrowserUploadTransition> {
        crate::storage::drivers::browser_resumable::check_response(receipt.as_ref())?;
        let confirmed = receipt
            .as_ref()
            .and_then(|value| acknowledgement(value, context.total_size));
        let (offset, completed) = match confirmed {
            Some(value) => value,
            None => match self.query_upload_session(context.upload_url).await {
                Ok(status) => {
                    let offset = ranges_offset(&status.next_expected_ranges, context.total_size)?;
                    (offset, offset == context.total_size)
                }
                Err(error) if error.kind() == aster_drive_storage::StorageErrorKind::NotFound => {
                    let metadata =
                        aster_drive_storage::StorageDriver::metadata(self, context.storage_path)
                            .await?;
                    if metadata.size != context.total_size {
                        return Err(error);
                    }
                    (context.total_size, true)
                }
                Err(error) => return Err(error),
            },
        };
        transition(context, offset, completed, vec!["range".into()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_drive_storage::{
        BrowserUploadMethod, BrowserUploadObservation, BrowserUploadRange, BrowserUploadRequest,
    };
    use std::collections::BTreeMap;
    #[test]
    fn google_acknowledgements_require_a_visible_valid_contiguous_range() {
        let request = BrowserUploadRequest {
            url: "https://upload.invalid".into(),
            method: BrowserUploadMethod::Put,
            headers: BTreeMap::new(),
            body: Some(BrowserUploadRange { start: 4, end: 8 }),
            response_headers: vec!["range".into()],
        };
        for (header, expected) in [
            (Some("bytes=0-5"), Some((6, false))),
            (Some("bytes=0-3"), Some((4, false))),
            (Some("bytes=0-2"), None),
            (Some("bytes=0-8"), None),
            (Some("bytes=1-5"), None),
            (Some("bytes=0-18446744073709551615"), None),
            (Some("bytes=0-9007199254740992"), None),
            (Some("garbage"), None),
            (None, None),
        ] {
            let observation = BrowserUploadObservation {
                status: 308,
                headers: header
                    .map(|v| BTreeMap::from([("range".into(), v.into())]))
                    .unwrap_or_default(),
                body: String::new(),
                body_truncated: false,
            };
            assert_eq!(
                acknowledgement(
                    &BrowserUploadReceipt {
                        request: &request,
                        observation: &observation
                    },
                    10
                ),
                expected,
                "{header:?}"
            );
        }
        for (status, total, expected) in [
            (200, 8, Some((8, true))),
            (201, 8, Some((8, true))),
            (200, 10, None),
            (202, 8, None),
            (204, 8, None),
        ] {
            let observation = BrowserUploadObservation {
                status,
                headers: BTreeMap::new(),
                body: String::new(),
                body_truncated: false,
            };
            assert_eq!(
                acknowledgement(
                    &BrowserUploadReceipt {
                        request: &request,
                        observation: &observation
                    },
                    total
                ),
                expected
            );
        }
    }
}
