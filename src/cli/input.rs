//! Shared bounded credential input. Debug and error paths never expose input bytes.
use std::io::{IsTerminal, Read};
use std::path::Path;
use std::str::FromStr;

use secrecy::{ExposeSecret, SecretString};

use crate::errors::{AsterError, Result};

const MAX_INPUT_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct SecretArg(SecretString);

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
    prompt: bool,
    label: &str,
) -> Result<SecretString> {
    if usize::from(explicit.is_some())
        + usize::from(stdin)
        + usize::from(file.is_some())
        + usize::from(prompt)
        > 1
    {
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
    } else if prompt && std::io::stdin().is_terminal() {
        rpassword::prompt_password(label)
            .map(SecretString::from)
            .map_err(|_| AsterError::config_error("hidden terminal input failed"))?
    } else {
        return Err(AsterError::validation_error(
            "input source required: select an explicit value, stdin, a private file, or an interactive terminal",
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
