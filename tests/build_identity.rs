//! Run the identity unit tests embedded beside their build-script implementation.
#[expect(
    dead_code,
    reason = "the build script is included here only to run its unit tests"
)]
#[path = "../build.rs"]
mod build_script;
