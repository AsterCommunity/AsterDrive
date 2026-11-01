//! Offline verifier generation with hidden terminal input or bounded piped stdin.

use super::CliResult;
use crate::errors::AsterError;
use inquire::validator::Validation;
use secrecy::{ExposeSecret, SecretString};
use std::io::{IsTerminal, Read};

const INVALID_PASSWORD: &str = "password must contain 1..4096 bytes without control characters";

fn valid_password(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}

pub(super) async fn execute() -> CliResult<()> {
    let input: SecretString = if std::io::stdin().is_terminal() {
        super::input::read_terminal_value(true, "Password (masked)", |value: &str| {
            Ok(if valid_password(value) {
                Validation::Valid
            } else {
                Validation::Invalid(INVALID_PASSWORD.into())
            })
        })?
    } else {
        let mut password = String::new();
        // Include CRLF and one overflow byte so a long input cannot be
        // silently accepted after truncation at a line ending.
        std::io::stdin()
            .take(4099)
            .read_to_string(&mut password)
            .map_err(|_| AsterError::validation_error("input must be valid UTF-8"))?;
        let password = password.strip_suffix('\n').unwrap_or(&password);
        let password = password.strip_suffix('\r').unwrap_or(password);
        password.to_owned().into()
    };
    let password = input.expose_secret();
    if !valid_password(password) {
        return Err(AsterError::validation_error(INVALID_PASSWORD).into());
    }
    let hash = aster_forge_auth::PasswordHashRuntime::default()
        .hash_password(password)
        .await
        .map_err(|_| AsterError::internal_error("cannot generate authentication verifier"))?;
    println!("{hash}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::valid_password;

    #[test]
    fn validation_counts_utf8_bytes_and_rejects_empty_and_control_input() {
        for value in ["", "\n", "\r\n", "a\nb", "a\tb", "a\0b", "a\x1bb", "a\x7fb"] {
            assert!(!valid_password(value));
        }
        assert!(valid_password("  abc  "));
        assert!(valid_password(&"x".repeat(4096)));
        assert!(!valid_password(&"x".repeat(4097)));
        assert!(valid_password(&"猫".repeat(1365)));
        assert!(!valid_password(&"猫".repeat(1366)));
    }
}
