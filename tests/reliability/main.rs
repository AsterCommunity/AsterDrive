//! Process-level crash-recovery acceptance tests.

#[cfg(unix)]
mod controller;
#[cfg(unix)]
mod invariants;
#[cfg(unix)]
mod minio;
#[cfg(unix)]
mod scenarios;

#[cfg(not(unix))]
#[test]
#[ignore = "real SIGKILL/restart; requires Unix"]
fn crash_recovery() {
    panic!("reliability E2E requires Unix; no checkpoint was exercised");
}
