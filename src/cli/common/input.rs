//! Shared bounded credential input. Debug and error paths never expose input bytes.
use std::io::{IsTerminal, Read};
use std::path::Path;
use std::str::FromStr;

use inquire::validator::{StringValidator, Validation};
use inquire::{InquireError, Password, PasswordDisplayMode, Text};
use secrecy::{ExposeSecret, SecretString};

use super::error::{CliError, CliResult};
use crate::errors::{AsterError, Result};

const MAX_INPUT_BYTES: u64 = 1024 * 1024;

/// Terminal prompts use stderr so structured stdout remains machine-readable.
pub(in crate::cli) fn read_terminal_value<V>(
    sensitive: bool,
    label: &str,
    validator: V,
) -> CliResult<SecretString>
where
    V: StringValidator + 'static,
{
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(AsterError::validation_error(
            "interactive input requires terminal stdin and stderr; use a pipe or an explicit input source",
        ).into());
    }
    let limit = |value: &str| {
        Ok(if value.len() as u64 > MAX_INPUT_BYTES {
            Validation::Invalid("input exceeds 1 MiB limit".into())
        } else {
            Validation::Valid
        })
    };
    let label = label.trim().trim_end_matches(':');
    if sensitive {
        Password::new(label)
            .without_confirmation()
            .with_display_mode(PasswordDisplayMode::Masked)
            .with_validator(limit)
            .with_validator(validator)
            .prompt()
    } else {
        Text::new(label)
            .with_validator(limit)
            .with_validator(validator)
            .prompt()
    }
    .map(SecretString::from)
    .map_err(terminal_error)
}

pub(in crate::cli) fn terminal_error(error: InquireError) -> CliError {
    match error {
        InquireError::OperationCanceled | InquireError::OperationInterrupted => CliError::Canceled,
        _ => AsterError::config_error("interactive terminal input failed (values redacted)").into(),
    }
}

#[derive(Debug, Clone)]
pub struct SecretArg(SecretString);

impl From<String> for SecretArg {
    fn from(value: String) -> Self {
        Self(value.into())
    }
}

impl FromStr for SecretArg {
    type Err = std::convert::Infallible;
    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(value.to_owned().into()))
    }
}

pub fn read_secret(
    explicit: Option<&SecretArg>,
    stdin: bool,
    file: Option<&Path>,
) -> Result<SecretString> {
    if usize::from(explicit.is_some()) + usize::from(stdin) + usize::from(file.is_some()) > 1 {
        return Err(AsterError::validation_error(
            "choose exactly one input source",
        ));
    }
    let value = if let Some(value) = explicit {
        value.0.clone()
    } else if let Some(path) = file {
        let file = std::fs::File::open(path)
            .map_err(|_| AsterError::config_error("cannot open private input file"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = file
                .metadata()
                .map_err(|_| AsterError::config_error("cannot inspect private input file"))?;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(AsterError::validation_error(
                    "input file must be private (chmod 600)",
                ));
            }
        }
        read_bounded(file)?
    } else if stdin {
        read_bounded(std::io::stdin().lock())?
    } else {
        return Err(AsterError::validation_error(
            "input source required: select an explicit value, stdin, or a private file",
        ));
    };
    if value.expose_secret().len() as u64 > MAX_INPUT_BYTES {
        return Err(AsterError::validation_error("input exceeds 1 MiB limit"));
    }
    Ok(value)
}

fn read_bounded(reader: impl Read) -> Result<SecretString> {
    let mut value = String::new();
    reader
        .take(MAX_INPUT_BYTES + 1)
        .read_to_string(&mut value)
        .map_err(|_| AsterError::validation_error("input must be valid UTF-8"))?;
    if value.len() as u64 > MAX_INPUT_BYTES {
        return Err(AsterError::validation_error("input exceeds 1 MiB limit"));
    }
    if value.ends_with('\n') {
        value.pop();
        if value.ends_with('\r') {
            value.pop();
        }
    }
    Ok(value.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_input_preserves_spaces_and_multiline_and_never_debugs_secrets() {
        let value = read_bounded("  first\nsecond  \r\n".as_bytes()).unwrap();
        assert_eq!(value.expose_secret(), "  first\nsecond  ");
        let arg: SecretArg = "unique-secret-marker".parse().unwrap();
        assert!(!format!("{arg:?}").contains("unique-secret-marker"));
        assert!(
            read_bounded(vec![b'x'; usize::try_from(MAX_INPUT_BYTES + 1).unwrap()].as_slice())
                .is_err()
        );
        assert!(read_bounded(&[255][..]).is_err());
    }
}
