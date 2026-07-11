//! WebDAV system-file blocking rules.

use std::collections::HashSet;
use std::sync::{Arc, LazyLock};

use parking_lot::RwLock;

use crate::config::RuntimeConfig;
use crate::config::definitions::{
    DEFAULT_WEBDAV_SYSTEM_FILE_PATTERNS, WEBDAV_BLOCK_SYSTEM_FILE_PATTERNS_KEY,
    WEBDAV_BLOCK_SYSTEM_FILES_ENABLED_KEY,
};

type RuntimePatternCache = RwLock<Option<(String, Arc<[String]>)>>;

static DEFAULT_NORMALIZED_PATTERNS: LazyLock<Arc<[String]>> = LazyLock::new(|| {
    Arc::from(normalize_patterns(
        DEFAULT_WEBDAV_SYSTEM_FILE_PATTERNS
            .iter()
            .map(|pattern| (*pattern).to_string())
            .collect(),
    ))
});

static RUNTIME_PATTERN_CACHE: LazyLock<RuntimePatternCache> = LazyLock::new(|| RwLock::new(None));

#[derive(Debug, Clone)]
pub struct SystemFileBlockPolicy {
    enabled: bool,
    patterns: Arc<[String]>,
}

impl SystemFileBlockPolicy {
    pub fn from_runtime_config(runtime_config: &RuntimeConfig) -> Self {
        let enabled = runtime_config.get_bool_or(WEBDAV_BLOCK_SYSTEM_FILES_ENABLED_KEY, true);
        let patterns = if enabled {
            patterns_from_runtime_config(runtime_config)
        } else {
            Arc::from([])
        };

        Self { enabled, patterns }
    }

    pub fn is_blocked_name(&self, name: &str) -> bool {
        self.enabled && is_blocked_name_with_normalized_patterns(name, &self.patterns)
    }
}

pub fn is_blocked_by_runtime_config(runtime_config: &RuntimeConfig, name: &str) -> bool {
    SystemFileBlockPolicy::from_runtime_config(runtime_config).is_blocked_name(name)
}

pub fn is_blocked_name(name: &str, patterns: &[String]) -> bool {
    let patterns = normalize_patterns(patterns.to_vec());
    is_blocked_name_with_normalized_patterns(name, &patterns)
}

fn is_blocked_name_with_normalized_patterns(name: &str, patterns: &[String]) -> bool {
    let normalized_name = normalize_for_match(name);
    patterns
        .iter()
        .any(|pattern| simple_glob_matches(&normalized_name, pattern))
}

fn patterns_from_runtime_config(runtime_config: &RuntimeConfig) -> Arc<[String]> {
    let Some(raw) = runtime_config.get(WEBDAV_BLOCK_SYSTEM_FILE_PATTERNS_KEY) else {
        return default_system_file_patterns();
    };

    if let Some((cached_raw, patterns)) = RUNTIME_PATTERN_CACHE.read().as_ref()
        && cached_raw == &raw
    {
        return patterns.clone();
    }

    let patterns = match serde_json::from_str::<Vec<String>>(&raw) {
        Ok(patterns) => Arc::from(normalize_patterns(patterns)),
        Err(error) => {
            tracing::warn!(
                error = %error,
                key = WEBDAV_BLOCK_SYSTEM_FILE_PATTERNS_KEY,
                "invalid WebDAV system-file pattern config; using default patterns"
            );
            default_system_file_patterns()
        }
    };

    *RUNTIME_PATTERN_CACHE.write() = Some((raw, patterns.clone()));
    patterns
}

fn default_system_file_patterns() -> Arc<[String]> {
    DEFAULT_NORMALIZED_PATTERNS.clone()
}

fn normalize_patterns(patterns: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::with_capacity(patterns.len());
    let mut seen = HashSet::with_capacity(patterns.len());
    for pattern in patterns {
        let pattern = normalize_for_match(&pattern);
        if !pattern.is_empty() && seen.insert(pattern.clone()) {
            normalized.push(pattern);
        }
    }
    normalized
}

fn normalize_for_match(value: &str) -> String {
    value.trim().to_lowercase()
}

fn simple_glob_matches(name: &str, pattern: &str) -> bool {
    if !pattern.contains('*') {
        return name == pattern;
    }

    let mut rest = name;
    let parts = pattern.split('*');
    let starts_with_glob = pattern.starts_with('*');
    let ends_with_glob = pattern.ends_with('*');
    let mut first_literal = true;

    for part in parts {
        if part.is_empty() {
            continue;
        }

        if first_literal && !starts_with_glob {
            let Some(after_prefix) = rest.strip_prefix(part) else {
                return false;
            };
            rest = after_prefix;
        } else {
            let Some(index) = rest.find(part) else {
                return false;
            };
            rest = &rest[index + part.len()..];
        }
        first_literal = false;
    }

    ends_with_glob || parts_trailing_literal_matched_to_end(name, pattern)
}

fn parts_trailing_literal_matched_to_end(name: &str, pattern: &str) -> bool {
    let Some(last_literal) = pattern.rsplit('*').find(|part| !part.is_empty()) else {
        return true;
    };
    name.ends_with(last_literal)
}

#[cfg(test)]
mod tests {
    use super::{
        SystemFileBlockPolicy, default_system_file_patterns, is_blocked_by_runtime_config,
        is_blocked_name, normalize_patterns,
    };
    use crate::config::RuntimeConfig;
    use crate::config::definitions::{
        CONFIG_CATEGORY_WEBDAV, WEBDAV_BLOCK_SYSTEM_FILE_PATTERNS_KEY,
        WEBDAV_BLOCK_SYSTEM_FILES_ENABLED_KEY,
    };
    use crate::types::{ConfigSource, ConfigValueType};
    use aster_forge_db::system_config;
    use chrono::Utc;

    fn config_model(key: &str, value: &str, value_type: ConfigValueType) -> system_config::Model {
        system_config::Model {
            id: 1,
            key: key.to_string(),
            value: value.to_string(),
            value_type,
            requires_restart: false,
            is_sensitive: false,
            source: ConfigSource::System,
            visibility: crate::types::ConfigVisibility::Private,
            namespace: String::new(),
            category: CONFIG_CATEGORY_WEBDAV.to_string(),
            description: "test".to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[test]
    fn exact_patterns_match_case_insensitively() {
        let patterns = default_system_file_patterns();
        assert!(is_blocked_name(".DS_Store", &patterns));
        assert!(is_blocked_name("thumbs.db", &patterns));
        assert!(is_blocked_name("DESKTOP.INI", &patterns));
        assert!(is_blocked_name("$recycle.bin", &patterns));
        assert!(is_blocked_name("system volume information", &patterns));
    }

    #[test]
    fn simple_glob_patterns_match() {
        let patterns = default_system_file_patterns();
        assert!(is_blocked_name("._photo.jpg", &patterns));
        assert!(!is_blocked_name("photo._jpg", &patterns));
        assert!(is_blocked_name(
            "blocked-file.txt",
            &[String::from("blocked-*")]
        ));
        assert!(is_blocked_name(
            "prefix-middle-suffix",
            &[String::from("prefix*suffix")]
        ));
        assert!(!is_blocked_name(
            "prefix-middle-other",
            &[String::from("prefix*suffix")]
        ));
        assert!(is_blocked_name(
            "prefix-middle-other-suffix",
            &[String::from("prefix*middle*suffix")]
        ));
        assert!(!is_blocked_name(
            "prefix-suffix-middle",
            &[String::from("prefix*middle*suffix")]
        ));
        assert!(is_blocked_name("report.tmp", &[String::from("*.tmp")]));
        assert!(!is_blocked_name(
            "report.tmp.backup",
            &[String::from("*.tmp")]
        ));
    }

    #[test]
    fn normal_file_names_do_not_match_default_patterns() {
        let patterns = default_system_file_patterns();
        assert!(!is_blocked_name("report.docx", &patterns));
        assert!(!is_blocked_name("photo.jpg", &patterns));
        assert!(!is_blocked_name("archive.zip", &patterns));
        assert!(!is_blocked_name("desktop.ini.backup", &patterns));
    }

    #[test]
    fn wildcard_pattern_can_block_all_names() {
        assert!(is_blocked_name("anything.txt", &[String::from("*")]));
        assert!(is_blocked_name(".DS_Store", &[String::from("*")]));
    }

    #[test]
    fn patterns_are_trimmed_deduplicated_and_empty_values_are_ignored() {
        assert_eq!(
            normalize_patterns(vec![
                String::new(),
                " blocked-* ".to_string(),
                "BLOCKED-*".to_string(),
                "   ".to_string(),
                "Thumbs.db".to_string(),
            ]),
            vec!["blocked-*".to_string(), "thumbs.db".to_string()]
        );
    }

    #[test]
    fn patterns_match_non_ascii_case_insensitively() {
        assert!(is_blocked_name("ä.txt", &[String::from("Ä*")]));
        assert!(is_blocked_name("Ä.txt", &[String::from("ä*")]));
    }

    #[test]
    fn disabled_runtime_policy_skips_default_matches() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(
            WEBDAV_BLOCK_SYSTEM_FILES_ENABLED_KEY,
            "false",
            ConfigValueType::Boolean,
        ));

        let policy = SystemFileBlockPolicy::from_runtime_config(&runtime_config);

        assert!(!policy.is_blocked_name(".DS_Store"));
        assert!(!is_blocked_by_runtime_config(&runtime_config, ".DS_Store"));
    }

    #[test]
    fn empty_runtime_pattern_list_blocks_nothing() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(
            WEBDAV_BLOCK_SYSTEM_FILE_PATTERNS_KEY,
            "[]",
            ConfigValueType::StringArray,
        ));

        let policy = SystemFileBlockPolicy::from_runtime_config(&runtime_config);

        assert!(!policy.is_blocked_name(".DS_Store"));
        assert!(!policy.is_blocked_name("Thumbs.db"));
    }

    #[test]
    fn invalid_runtime_pattern_list_falls_back_to_defaults() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(
            WEBDAV_BLOCK_SYSTEM_FILE_PATTERNS_KEY,
            "not json",
            ConfigValueType::StringArray,
        ));

        let policy = SystemFileBlockPolicy::from_runtime_config(&runtime_config);

        assert!(policy.is_blocked_name(".DS_Store"));
        assert!(policy.is_blocked_name("Thumbs.db"));
        assert!(!policy.is_blocked_name("report.docx"));
    }

    #[test]
    fn runtime_pattern_cache_follows_raw_config_changes() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(
            WEBDAV_BLOCK_SYSTEM_FILE_PATTERNS_KEY,
            r#"["first-*"]"#,
            ConfigValueType::StringArray,
        ));

        let first_policy = SystemFileBlockPolicy::from_runtime_config(&runtime_config);
        assert!(first_policy.is_blocked_name("first-file.txt"));
        assert!(!first_policy.is_blocked_name("second-file.txt"));

        runtime_config.apply(config_model(
            WEBDAV_BLOCK_SYSTEM_FILE_PATTERNS_KEY,
            r#"["second-*"]"#,
            ConfigValueType::StringArray,
        ));

        let second_policy = SystemFileBlockPolicy::from_runtime_config(&runtime_config);
        assert!(!second_policy.is_blocked_name("first-file.txt"));
        assert!(second_policy.is_blocked_name("second-file.txt"));
    }
}
