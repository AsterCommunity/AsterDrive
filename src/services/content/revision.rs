use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::revision_repo::RevisionAppendError;
use crate::errors::{AsterError, precondition_failed_with_code};

pub(crate) fn map_append_error(error: RevisionAppendError) -> AsterError {
    match error {
        RevisionAppendError::HeadChanged => precondition_failed_with_code(
            ApiErrorCode::FileModifiedDuringWrite,
            "file revision head changed while content was being committed",
        ),
        RevisionAppendError::EtagMismatch => precondition_failed_with_code(
            ApiErrorCode::FileEtagMismatch,
            "file has been modified (ETag mismatch)",
        ),
        RevisionAppendError::Repository(error) => error,
    }
}

#[cfg(test)]
mod tests {
    use crate::api::api_error_code::ApiErrorCode;
    use crate::db::repository::revision_repo::RevisionAppendError;
    use crate::errors::AsterError;

    use super::map_append_error;

    #[test]
    fn append_conflicts_map_to_stable_precondition_codes() {
        let cases = [
            (
                RevisionAppendError::HeadChanged,
                ApiErrorCode::FileModifiedDuringWrite,
                "file revision head changed while content was being committed",
            ),
            (
                RevisionAppendError::EtagMismatch,
                ApiErrorCode::FileEtagMismatch,
                "file has been modified (ETag mismatch)",
            ),
        ];

        for (error, expected_code, expected_message) in cases {
            let mapped = map_append_error(error);
            assert_eq!(mapped.api_error_code(), expected_code);
            assert_eq!(mapped.message(), expected_message);
        }
    }

    #[test]
    fn append_repository_error_preserves_original_error() {
        let original = AsterError::record_not_found("revision fixture missing");
        let expected_code = original.code();
        let expected_api_code = original.api_error_code();
        let expected_message = original.message().to_owned();
        let mapped = map_append_error(RevisionAppendError::Repository(original));

        assert_eq!(mapped.code(), expected_code);
        assert_eq!(mapped.api_error_code(), expected_api_code);
        assert_eq!(mapped.message(), expected_message);
    }
}
