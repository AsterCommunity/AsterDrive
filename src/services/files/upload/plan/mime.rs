use crate::errors::{AsterError, Result};

const MAX_UPLOAD_MIME_TYPE_LEN: usize = 255;

pub(crate) fn normalize_upload_mime_type(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_UPLOAD_MIME_TYPE_LEN {
        return Err(AsterError::validation_error("invalid upload MIME type"));
    }
    let mut slash_parts = value.split('/');
    let Some(type_part) = slash_parts.next() else {
        return Err(AsterError::validation_error("invalid upload MIME type"));
    };
    let Some(subtype_part) = slash_parts.next() else {
        return Err(AsterError::validation_error("invalid upload MIME type"));
    };
    if type_part.trim().is_empty() || subtype_part.trim().is_empty() || slash_parts.next().is_some()
    {
        return Err(AsterError::validation_error("invalid upload MIME type"));
    }

    let parsed = value
        .parse::<mime::Mime>()
        .map_err(|_| AsterError::validation_error("invalid upload MIME type"))?;
    if parsed.type_() == mime::STAR || parsed.subtype() == mime::STAR {
        return Err(AsterError::validation_error(
            "upload MIME type must be concrete",
        ));
    }
    let normalized = parsed.to_string();
    if normalized.len() > MAX_UPLOAD_MIME_TYPE_LEN {
        return Err(AsterError::validation_error("upload MIME type is too long"));
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::normalize_upload_mime_type;

    #[test]
    fn normalizes_valid_mime_with_charset_parameter() {
        assert_eq!(
            normalize_upload_mime_type("Text/Plain; Charset=UTF-8").unwrap(),
            "text/plain; charset=utf-8"
        );
    }

    #[test]
    fn rejects_invalid_upload_mime_boundaries() {
        for value in [
            "text/plain\r\nx-invalid: value",
            "a/b/c",
            "/plain",
            "text/",
            "*/*",
            "text/*",
        ] {
            assert!(
                normalize_upload_mime_type(value).is_err(),
                "{value:?} must be rejected"
            );
        }
    }
}
