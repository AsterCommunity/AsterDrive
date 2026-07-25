//! Compile-time identity for the running AsterDrive binary.

pub const PRODUCT_NAME: &str = "AsterDrive";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const BUILD_TIME: &str = match option_env!("ASTER_BUILD_TIME") {
    Some(value) => value,
    None => "unknown",
};
pub const REVISION: &str = match option_env!("ASTER_BUILD_REVISION") {
    Some(value) => value,
    None => "unknown",
};
pub const PROFILE: &str = match option_env!("ASTER_BUILD_PROFILE") {
    Some(value) => value,
    None => "unknown",
};
pub const TARGET: &str = match option_env!("ASTER_BUILD_TARGET") {
    Some(value) => value,
    None => "unknown",
};
pub const VARIANT: &str = if cfg!(feature = "metrics") {
    "metrics"
} else {
    "default"
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_identity_is_available() {
        assert_eq!(PRODUCT_NAME, "AsterDrive");
        assert!(!VERSION.is_empty());
        assert!(!BUILD_TIME.is_empty());
        assert!(!REVISION.is_empty());
        assert!(!PROFILE.is_empty());
        assert!(!TARGET.is_empty());
        assert!(matches!(VARIANT, "default" | "metrics"));
    }
}
