use std::borrow::Cow;

use crate::errors::{AsterError, Result};
use validator::{Validate, ValidationError, ValidationErrors, ValidationErrorsKind};

pub(crate) fn validate_request<T: Validate>(value: &T) -> Result<()> {
    value.validate().map_err(validation_errors_to_aster)
}

pub(crate) fn validate_name(value: &str) -> std::result::Result<(), ValidationError> {
    crate::utils::validate_name(value).map_err(aster_to_validation_error)
}

pub(crate) fn validate_uuid(value: &str) -> std::result::Result<(), ValidationError> {
    let uuid = uuid::Uuid::parse_str(value)
        .map_err(|_| message_validation_error("value must be a valid UUID"))?;
    if !uuid.hyphenated().to_string().eq_ignore_ascii_case(value) {
        return Err(message_validation_error("value must be a hyphenated UUID"));
    }
    Ok(())
}

pub(crate) fn validate_non_blank(value: &str) -> std::result::Result<(), ValidationError> {
    if value.trim().is_empty() {
        return Err(message_validation_error("value cannot be empty"));
    }
    Ok(())
}

pub(crate) fn validate_auth_username(value: &str) -> std::result::Result<(), ValidationError> {
    crate::services::auth_service::validate_username(value).map_err(aster_to_validation_error)
}

pub(crate) fn validate_auth_email(value: &str) -> std::result::Result<(), ValidationError> {
    crate::services::auth_service::validate_email(value).map_err(aster_to_validation_error)
}

pub(crate) fn validate_auth_password(value: &str) -> std::result::Result<(), ValidationError> {
    crate::services::auth_service::validate_password(value).map_err(aster_to_validation_error)
}

pub(crate) fn validate_team_name(value: &str) -> std::result::Result<(), ValidationError> {
    crate::services::team_service::validate_team_name(value)
        .map(|_| ())
        .map_err(aster_to_validation_error)
}

pub(crate) fn validate_property_namespace(value: &str) -> std::result::Result<(), ValidationError> {
    if value.trim().is_empty() {
        return Err(message_validation_error("namespace cannot be empty"));
    }
    if value.len() > 256 {
        return Err(message_validation_error("namespace too long (max 256)"));
    }
    Ok(())
}

pub(crate) fn validate_property_name(value: &str) -> std::result::Result<(), ValidationError> {
    if value.trim().is_empty() {
        return Err(message_validation_error("property name cannot be empty"));
    }
    if value.len() > 256 {
        return Err(message_validation_error("property name too long (max 256)"));
    }
    Ok(())
}

pub(crate) fn validate_property_value(value: &str) -> std::result::Result<(), ValidationError> {
    if value.len() > 65_536 {
        return Err(message_validation_error(
            "property value too long (max 64KB)",
        ));
    }
    Ok(())
}

fn aster_to_validation_error(error: AsterError) -> ValidationError {
    let mut validation_error = ValidationError::new("invalid");
    validation_error.message = Some(Cow::Owned(error.message().to_string()));
    validation_error
}

pub(crate) fn message_validation_error(message: impl Into<String>) -> ValidationError {
    let mut validation_error = ValidationError::new("invalid");
    validation_error.message = Some(Cow::Owned(message.into()));
    validation_error
}

fn validation_errors_to_aster(errors: ValidationErrors) -> AsterError {
    let mut messages = Vec::new();
    collect_validation_messages(&errors, None, &mut messages);
    messages.sort();
    AsterError::validation_error(messages.join(", "))
}

fn collect_validation_messages(
    errors: &ValidationErrors,
    prefix: Option<&str>,
    messages: &mut Vec<String>,
) {
    for (field, kind) in errors.errors() {
        let field_path = if field == "__all__" {
            prefix.map(str::to_string)
        } else {
            Some(match prefix {
                Some(prefix) => format!("{prefix}.{field}"),
                None => field.to_string(),
            })
        };

        match kind {
            ValidationErrorsKind::Field(field_errors) => {
                for error in field_errors {
                    messages.push(validation_error_message(field_path.as_deref(), error));
                }
            }
            ValidationErrorsKind::Struct(child) => {
                collect_validation_messages(child, field_path.as_deref(), messages);
            }
            ValidationErrorsKind::List(children) => {
                for (index, child) in children {
                    let list_path = match &field_path {
                        Some(path) => format!("{path}[{index}]"),
                        None => format!("[{index}]"),
                    };
                    collect_validation_messages(child, Some(&list_path), messages);
                }
            }
        }
    }
}

fn validation_error_message(field: Option<&str>, error: &ValidationError) -> String {
    error
        .message
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| match field {
            Some(field) => format!("invalid field '{field}'"),
            None => "invalid request".to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(validator::Validate)]
    struct NameReq {
        #[validate(custom(function = "crate::api::dto::validation::validate_name"))]
        name: String,
    }

    #[derive(validator::Validate)]
    struct SizeReq {
        #[validate(range(min = 1, message = "page_size must be positive"))]
        page_size: i64,
    }

    #[test]
    fn validate_request_uses_existing_name_rules() {
        let err = validate_request(&NameReq {
            name: "bad/name".to_string(),
        })
        .unwrap_err();
        assert_eq!(err.message(), "name contains forbidden character '/'");
    }

    #[test]
    fn validate_request_surfaces_range_messages() {
        let err = validate_request(&SizeReq { page_size: 0 }).unwrap_err();
        assert_eq!(err.message(), "page_size must be positive");
    }

    #[derive(validator::Validate)]
    #[validate(schema(function = "validate_same_values", skip_on_field_errors = false))]
    struct NestedReq {
        #[validate(nested)]
        items: Vec<NameReq>,
    }

    fn validate_same_values(_value: &NestedReq) -> std::result::Result<(), ValidationError> {
        Err(message_validation_error("items must be unique"))
    }

    #[test]
    fn validate_request_collects_nested_and_schema_messages() {
        let err = validate_request(&NestedReq {
            items: vec![NameReq {
                name: "bad/name".to_string(),
            }],
        })
        .unwrap_err();
        assert_eq!(
            err.message(),
            "items must be unique, name contains forbidden character '/'"
        );
    }
}
