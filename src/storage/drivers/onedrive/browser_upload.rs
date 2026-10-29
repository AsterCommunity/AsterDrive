use super::OneDriveDriver;
use crate::storage::drivers::browser_resumable::{checked_ack, ranges_offset, transition};
use aster_drive_storage::{
    BrowserUploadContext, BrowserUploadDriver, BrowserUploadProtocol, BrowserUploadReceipt,
    BrowserUploadTransition, ProviderResumableUploadDriver, Result,
};

fn acknowledgement(receipt: &BrowserUploadReceipt<'_>, total: u64) -> Option<(u64, bool)> {
    let range = receipt.request.body?;
    let response = receipt.observation;
    match response.status {
        200 | 201 => checked_ack(total, true, range, total),
        202 if !response.body_truncated => {
            let body: serde_json::Value = serde_json::from_str(&response.body).ok()?;
            let ranges: Vec<String> =
                serde_json::from_value(body.get("nextExpectedRanges")?.clone()).ok()?;
            if ranges.is_empty() {
                return None;
            }
            checked_ack(ranges_offset(&ranges, total).ok()?, false, range, total)
        }
        _ => None,
    }
}

#[async_trait::async_trait]
impl BrowserUploadDriver for OneDriveDriver {
    fn protocol(&self) -> BrowserUploadProtocol {
        aster_drive_storage::BrowserUploadProtocol {
            id: "asterdrive.microsoft_graph.resumable".into(),
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
                    // A vanished session can also mean expiry. Only an actual object of the
                    // expected length proves that the provider committed the final request.
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
        transition(context, offset, completed, Vec::new())
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
    fn graph_acknowledgements_are_strict_and_bounded() {
        let request = BrowserUploadRequest {
            url: "https://upload.invalid".into(),
            method: BrowserUploadMethod::Put,
            headers: BTreeMap::new(),
            body: Some(BrowserUploadRange { start: 4, end: 8 }),
            response_headers: vec![],
        };
        for (status, body, expected) in [
            (202, r#"{"nextExpectedRanges":["6-"]}"#, Some((6, false))),
            (
                202,
                r#"{"nextExpectedRanges":["7-","6-6"]}"#,
                Some((6, false)),
            ),
            (202, r#"{"nextExpectedRanges":["4-"]}"#, Some((4, false))),
            (202, r#"{"nextExpectedRanges":[]}"#, None),
            (202, r#"{"nextExpectedRanges":["3-"]}"#, None),
            (202, r#"{"nextExpectedRanges":["9-"]}"#, None),
            (
                202,
                r#"{"nextExpectedRanges":["18446744073709551616-"]}"#,
                None,
            ),
            (202, r#"{"nextExpectedRanges":["6-2"]}"#, None),
            (202, "{}", None),
            (202, "", None),
            (202, "null", None),
            (200, "", None),
            (201, "", None),
            (204, "", None),
            (416, "", None),
            (0, "", None),
        ] {
            let observation = BrowserUploadObservation {
                status,
                headers: BTreeMap::new(),
                body: body.into(),
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
                "{status}: {body}"
            );
        }
        for status in [200, 201] {
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
                    8
                ),
                Some((8, true))
            );
        }
    }
}
