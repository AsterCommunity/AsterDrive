//! Shared actix extractors configuration for AsterDrive REST APIs.

use crate::errors::AsterError;
use actix_web::{Error, HttpRequest, error::JsonPayloadError, web};

pub const DEFAULT_JSON_LIMIT: usize = 1024 * 1024;
pub const DEFAULT_PAYLOAD_LIMIT: usize = 10 * 1024 * 1024;

pub fn json_config(limit: usize) -> web::JsonConfig {
    web::JsonConfig::default()
        .limit(limit)
        .error_handler(json_error_handler)
}

fn json_error_handler(error: JsonPayloadError, _req: &HttpRequest) -> Error {
    json_payload_error(error).into()
}

fn json_payload_error(error: JsonPayloadError) -> AsterError {
    match error {
        JsonPayloadError::OverflowKnownLength { length, limit } => AsterError::payload_too_large(
            format!("JSON payload is too large: {length} bytes exceeds the {limit} byte limit"),
        ),
        JsonPayloadError::Overflow { limit } => {
            AsterError::payload_too_large(format!("JSON payload exceeded the {limit} byte limit"))
        }
        JsonPayloadError::Payload(actix_web::error::PayloadError::Overflow) => {
            AsterError::payload_too_large("JSON payload exceeded the configured size limit")
        }
        JsonPayloadError::ContentType => {
            AsterError::validation_error("request content type must be application/json")
        }
        JsonPayloadError::Deserialize(error) => {
            AsterError::validation_error(format!("invalid JSON request body: {error}"))
        }
        JsonPayloadError::Payload(error) => {
            AsterError::validation_error(format!("failed to read JSON request body: {error}"))
        }
        JsonPayloadError::Serialize(error) => {
            AsterError::validation_error(format!("invalid JSON request body: {error}"))
        }
        _ => AsterError::validation_error(format!("invalid JSON request body: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::api_error_code::ApiErrorCode;

    #[test]
    fn deserialize_error_maps_to_validation_error() {
        let error = json_payload_error(JsonPayloadError::Deserialize(
            serde_json::from_str::<serde_json::Value>("{").unwrap_err(),
        ));

        assert!(matches!(error, AsterError::ValidationError(_)));
        assert_eq!(error.api_error_code(), ApiErrorCode::BadRequest);
    }

    #[test]
    fn overflow_maps_to_payload_too_large() {
        let error = json_payload_error(JsonPayloadError::OverflowKnownLength {
            length: 2,
            limit: 1,
        });

        assert!(matches!(error, AsterError::PayloadTooLarge(_)));
        assert_eq!(error.api_error_code(), ApiErrorCode::FileTooLarge);
    }

    #[test]
    fn payload_overflow_maps_to_payload_too_large() {
        let error = json_payload_error(JsonPayloadError::Payload(
            actix_web::error::PayloadError::Overflow,
        ));

        assert!(matches!(error, AsterError::PayloadTooLarge(_)));
        assert_eq!(error.api_error_code(), ApiErrorCode::FileTooLarge);
    }

    #[test]
    fn serialize_payload_error_maps_to_validation_error_in_request_config() {
        let error =
            serde_json::to_string(&std::collections::HashMap::from([(vec![1_u8], "value")]))
                .unwrap_err();
        let error = json_payload_error(JsonPayloadError::Serialize(error));

        assert!(matches!(error, AsterError::ValidationError(_)));
        assert_eq!(error.api_error_code(), ApiErrorCode::BadRequest);
    }
}
