//! Offline password hashing command; stdin is never echoed.

use std::io;

pub(super) async fn execute() -> io::Result<()> {
    use std::io::Read;
    let mut password = String::new();
    // Include CRLF and one overflow byte so a long input cannot be
    // silently accepted after truncation at a line ending.
    std::io::stdin().take(4099).read_to_string(&mut password)?;
    let password = password.strip_suffix('\n').unwrap_or(&password);
    let password = password.strip_suffix('\r').unwrap_or(password);
    if password.is_empty() || password.len() > 4096 || password.chars().any(char::is_control) {
        return Err(io::Error::other(
            "password must contain 1..4096 bytes without control characters",
        ));
    }
    let hash = aster_forge_auth::PasswordHashRuntime::default()
        .hash_password(password)
        .await
        .map_err(|error| io::Error::other(error.to_string()))?;
    println!("{hash}");
    Ok(())
}
