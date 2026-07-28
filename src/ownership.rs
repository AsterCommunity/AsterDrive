//! Product-level ownership checks.

use crate::errors::{AsterError, Result};

pub(crate) fn verify_owner(entity_user_id: i64, user_id: i64, entity_name: &str) -> Result<()> {
    if entity_user_id != user_id {
        return Err(AsterError::auth_forbidden(format!(
            "not your {entity_name}"
        )));
    }
    Ok(())
}

pub(crate) fn verify_optional_owner(
    entity_user_id: Option<i64>,
    user_id: i64,
    entity_name: &str,
) -> Result<()> {
    verify_owner(
        entity_user_id.ok_or_else(|| {
            AsterError::auth_forbidden(format!("{entity_name} has no personal owner"))
        })?,
        user_id,
        entity_name,
    )
}

#[cfg(test)]
mod tests {
    use super::{verify_optional_owner, verify_owner};

    #[test]
    fn owner_checks_preserve_drive_forbidden_errors() {
        assert!(verify_owner(7, 7, "file").is_ok());

        let mismatch = verify_owner(7, 8, "file").expect_err("mismatched owner should fail");
        assert_eq!(mismatch.code(), "E013");
        assert_eq!(mismatch.message(), "not your file");

        let missing =
            verify_optional_owner(None, 8, "folder").expect_err("missing owner should fail");
        assert_eq!(missing.code(), "E013");
        assert_eq!(missing.message(), "folder has no personal owner");
    }
}
