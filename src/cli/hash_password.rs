//! Offline password hashing with strict, hidden terminal input and bounded stdin.

use std::io::{self, IsTerminal, Read, Write};

const MAX_PASSWORD_BYTES: usize = 4096;
const INVALID_INPUT: &str = "password must contain 1..4096 bytes without control characters";

/// Generate a verifier offline, keeping prompts off the hash-only stdout stream.
///
/// Terminal and pipe input share validation, so interactive editing cannot relax
/// the credential policy or require loading server configuration.
pub(super) async fn execute() -> io::Result<()> {
    let stdin = io::stdin();
    let input = if stdin.is_terminal() {
        read_hidden_terminal(stdin.lock())?
    } else {
        read_bounded(stdin.lock())?
    };
    let password = normalize_and_validate(&input)?;
    let hash = aster_forge_auth::PasswordHashRuntime::default()
        .hash_password(password)
        .await
        .map_err(|error| io::Error::other(error.to_string()))?;
    println!("{hash}");
    Ok(())
}

/// Remove one transport line ending without trimming intentional password spaces.
///
/// Reject empty, oversized, and control-containing input with a fixed diagnostic
/// that never includes credential content; the limit counts UTF-8 bytes.
fn normalize_and_validate(input: &str) -> io::Result<&str> {
    let input = input.strip_suffix('\n').unwrap_or(input);
    let input = input.strip_suffix('\r').unwrap_or(input);
    if input.is_empty() || input.len() > MAX_PASSWORD_BYTES || input.chars().any(char::is_control) {
        return Err(io::Error::other(INVALID_INPUT));
    }
    Ok(input)
}

/// Bound pipe reads while retaining enough suffix bytes to detect overflow.
///
/// EOF still terminates pipe input; a trailing CRLF is validated by the caller.
fn read_bounded(reader: impl Read) -> io::Result<String> {
    let mut input = String::new();
    // Include CRLF and one overflow byte so truncation cannot hide a long input.
    reader
        .take((MAX_PASSWORD_BYTES + 3) as u64)
        .read_to_string(&mut input)?;
    Ok(input)
}

/// Own terminal restoration across input, output, and cancellation failures.
struct HiddenTerminal {
    active: bool,
    #[cfg(windows)]
    console_mode: crossterm_winapi::ConsoleMode,
    #[cfg(windows)]
    original_mode: u32,
}

impl HiddenTerminal {
    /// Disable echo and line processing before the prompt permits user input.
    ///
    /// Save the complete Windows mode: crossterm's Windows raw-mode teardown
    /// enables default flags rather than restoring the caller's exact settings.
    fn enter() -> io::Result<Self> {
        #[cfg(windows)]
        let console_mode =
            crossterm_winapi::ConsoleMode::from(crossterm_winapi::Handle::current_in_handle()?);
        #[cfg(windows)]
        let original_mode = console_mode.mode()?;
        crossterm::terminal::enable_raw_mode()?;
        Ok(Self {
            active: true,
            #[cfg(windows)]
            console_mode,
            #[cfg(windows)]
            original_mode,
        })
    }

    /// Restore the original mode, leaving the guard armed if restoration fails.
    fn restore(&mut self) -> io::Result<()> {
        #[cfg(windows)]
        self.console_mode.set_mode(self.original_mode)?;
        #[cfg(not(windows))]
        crossterm::terminal::disable_raw_mode()?;
        self.active = false;
        Ok(())
    }
}

impl Drop for HiddenTerminal {
    /// Retry failed explicit restoration, reporting failure without input content.
    fn drop(&mut self) {
        if self.active && self.restore().is_err() {
            eprintln!("failed to restore terminal input mode");
        }
    }
}

/// Explain unavailable hidden input without exposing OS details or credentials.
///
/// Keep the failure category for callers; never fall back to echoing terminal
/// input when the controlling terminal or raw-mode setup is unavailable.
fn hidden_input_unavailable(error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        "hidden input needs a usable terminal; pipe the password through stdin instead",
    )
}

/// Read a hidden line, directing the prompt to the controlling terminal.
///
/// Restore input mode before returning even when reading or writing fails. The
/// explicit restore propagates errors; the guard retries if that restore fails.
fn read_hidden_terminal(reader: impl Read) -> io::Result<String> {
    #[cfg(unix)]
    let terminal_path = "/dev/tty";
    #[cfg(windows)]
    let terminal_path = "CONOUT$";
    let mut terminal = std::fs::OpenOptions::new()
        .write(true)
        .open(terminal_path)
        .map_err(hidden_input_unavailable)?;
    // rpassword 7.5 silently discards Tab/NUL before validation. Use only the
    // platform terminal-mode mechanism so illegal input is never reinterpreted.
    let mut hidden = HiddenTerminal::enter().map_err(hidden_input_unavailable)?;
    let result = (|| {
        terminal.write_all(b"Password: ")?;
        terminal.flush()?;
        let input = read_terminal_line(reader);
        terminal.write_all(b"\r\n")?;
        terminal.flush()?;
        input
    })();
    hidden.restore()?;
    result
}

/// Apply terminal editing while preserving all other bytes for strict validation.
///
/// Backspace removes a complete UTF-8 character; Ctrl-C and EOF keys cancel.
/// Overflow is sticky and drained through Enter with echo still disabled, so a
/// rejected long paste cannot leave its tail to be interpreted by the shell.
fn read_terminal_line(mut reader: impl Read) -> io::Result<String> {
    let mut input = Vec::new();
    let mut overflowed = false;
    loop {
        let mut byte = [0];
        reader.read_exact(&mut byte)?;
        match byte[0] {
            b'\r' | b'\n' => break,
            // These are terminal editing keys, not password content.
            8 | 127 if !overflowed => {
                if let Some(last) = input.pop()
                    && last & 0xc0 == 0x80
                {
                    while input.last().is_some_and(|byte| byte & 0xc0 == 0x80) {
                        input.pop();
                    }
                    input.pop();
                }
            }
            3 => {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "input cancelled",
                ));
            }
            4 | 26 => {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "input ended"));
            }
            byte => {
                if input.len() == MAX_PASSWORD_BYTES || overflowed {
                    // Keep echo disabled and drain the rest of a long paste to
                    // Enter, so its tail cannot become input to the caller's shell.
                    overflowed = true;
                } else {
                    input.push(byte);
                }
            }
        }
    }
    if overflowed {
        return Err(io::Error::other(INVALID_INPUT));
    }
    String::from_utf8(input)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "input must be valid UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A single transport terminator is removable; password spaces are content.
    #[test]
    fn normalizes_one_line_ending_without_trimming_spaces() {
        for input in ["abc", "abc\n", "abc\r\n", "abc\r"] {
            assert!(normalize_and_validate(input).is_ok_and(|value| value == "abc"));
        }
        assert!(normalize_and_validate("  abc  \r\n").is_ok_and(|value| value == "  abc  "));
    }

    /// Validation must reject controls and enforce the limit in bytes, not chars.
    #[test]
    fn rejects_empty_control_and_oversized_input() {
        for input in [
            "", "\n", "\r\n", "a\n\n", "a\nb", "a\tb", "a\0b", "a\x1bb", "a\x7fb",
        ] {
            assert!(normalize_and_validate(input).is_err());
        }
        assert!(normalize_and_validate(&"x".repeat(4096)).is_ok());
        assert!(normalize_and_validate(&"x".repeat(4097)).is_err());
        assert!(normalize_and_validate(&"猫".repeat(1365)).is_ok());
        assert!(normalize_and_validate(&"猫".repeat(1366)).is_err());
    }

    /// Bounded reads must not turn truncated or undecodable input into a credential.
    #[test]
    fn bounded_pipe_preserves_full_limit_and_rejects_overflow_and_invalid_utf8() {
        let input = read_bounded(format!("{}\r\n", "x".repeat(4096)).as_bytes()).unwrap();
        assert!(normalize_and_validate(&input).is_ok());
        let input = read_bounded(vec![b'x'; 10_000].as_slice()).unwrap();
        assert_eq!(input.len(), 4099);
        assert!(normalize_and_validate(&input).is_err());
        assert!(read_bounded(&[0xff][..]).is_err());
    }

    /// Hidden input errors retain their category and only expose safe guidance.
    #[test]
    fn hidden_input_failure_preserves_kind_without_exposing_source_details() {
        for kind in [
            io::ErrorKind::NotFound,
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::Unsupported,
        ] {
            let error = hidden_input_unavailable(io::Error::new(kind, "credential-marker"));
            assert_eq!(error.kind(), kind);
            assert!(
                error
                    .to_string()
                    .contains("pipe the password through stdin")
            );
            assert!(!error.to_string().contains("credential-marker"));
        }
    }

    /// Editing, cancellation, malformed UTF-8, and overflow keep strict semantics.
    #[test]
    fn terminal_preserves_invalid_controls_and_edits_whole_unicode_characters() {
        let input = read_terminal_line("abc\t\0\r".as_bytes()).unwrap();
        assert!(normalize_and_validate(&input).is_err());
        let input = read_terminal_line("猫😀\x7f\x08abc\r".as_bytes()).unwrap();
        assert!(input == "abc");
        assert!(read_terminal_line(format!("{}\r", "x".repeat(4096)).as_bytes()).is_ok());
        assert!(read_terminal_line(format!("{}\r", "x".repeat(4097)).as_bytes()).is_err());
        let mut overflow = io::Cursor::new(format!("{}tail\r", "x".repeat(4097)).into_bytes());
        assert!(read_terminal_line(&mut overflow).is_err());
        assert_eq!(overflow.position(), overflow.get_ref().len() as u64);
        assert!(read_terminal_line(&[0xff, b'\r'][..]).is_err());
        assert_eq!(
            read_terminal_line(&[3][..]).unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        for input in [&[4][..], &[26][..], &[][..]] {
            assert_eq!(
                read_terminal_line(input).unwrap_err().kind(),
                io::ErrorKind::UnexpectedEof
            );
        }
    }
}
