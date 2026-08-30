//! In-memory storage placement profile resolution.
//!
//! Persisted profile/rule payloads are decoded and compiled during topology
//! reload. Upload hot paths consume only the compiled structures in this
//! module; they do not query placement tables.

use std::collections::HashSet;

use aster_forge_file_classification::{FileCategory, classify_file, normalize_extension_filter};
use rand::RngExt;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

pub const PLACEMENT_PAYLOAD_FORMAT_VERSION: u32 = 1;
pub const PLACEMENT_PAYLOAD_SCHEMA_VERSION: u32 = 1;
pub const MAX_PLACEMENT_PAYLOAD_BYTES: usize = 4_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PlacementPayloadEnvelope<T> {
    pub format_version: u32,
    pub schema_version: u32,
    pub values: T,
}

impl<T> PlacementPayloadEnvelope<T> {
    pub fn new(values: T) -> Self {
        Self {
            format_version: PLACEMENT_PAYLOAD_FORMAT_VERSION,
            schema_version: PLACEMENT_PAYLOAD_SCHEMA_VERSION,
            values,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageAdmissionConstraints {
    #[serde(default)]
    pub allowed_extensions: Vec<String>,
    #[serde(default)]
    pub denied_extensions: Vec<String>,
    #[serde(default = "default_true")]
    pub accept_extensionless: bool,
    #[serde(default)]
    pub allowed_categories: Vec<FileCategory>,
    #[serde(default)]
    pub denied_categories: Vec<FileCategory>,
    #[serde(default)]
    pub max_file_size: i64,
}

impl Default for StorageAdmissionConstraints {
    fn default() -> Self {
        Self {
            allowed_extensions: Vec::new(),
            denied_extensions: Vec::new(),
            accept_extensionless: true,
            allowed_categories: Vec::new(),
            denied_categories: Vec::new(),
            max_file_size: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PlacementMatcher {
    #[serde(default)]
    pub min_file_size: i64,
    #[serde(default)]
    pub max_file_size: i64,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub compound_extensions: Vec<String>,
    #[serde(default)]
    pub extensionless: Option<bool>,
    #[serde(default)]
    pub categories: Vec<FileCategory>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum PlacementSelectionMode {
    #[default]
    FirstAvailable,
    WeightedRandom,
}

impl PlacementSelectionMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FirstAvailable => "first_available",
            Self::WeightedRandom => "weighted_random",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum PlacementUnavailableBehavior {
    #[default]
    NextRule,
    Reject,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum UploadExecutionPreference {
    #[default]
    Automatic,
    ForceServerStream,
}

impl UploadExecutionPreference {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::ForceServerStream => "force_server_stream",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum TargetExclusionReason {
    #[serde(rename = "target_disabled")]
    Disabled,
    #[serde(rename = "target_draining")]
    Draining,
    #[serde(rename = "target_unavailable")]
    Unavailable,
    #[serde(rename = "target_incompatible")]
    Incompatible,
    #[serde(rename = "policy_max_file_size_exceeded")]
    PolicyFileSizeExceeded,
}

impl TargetExclusionReason {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Disabled => "target_disabled",
            Self::Draining => "target_draining",
            Self::Unavailable => "target_unavailable",
            Self::Incompatible => "target_incompatible",
            Self::PolicyFileSizeExceeded => "policy_max_file_size_exceeded",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoragePlacementContext {
    pub profile_id: i64,
    pub filename: String,
    pub file_size: i64,
    pub extension: String,
    pub compound_extension: Option<String>,
    pub category: FileCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePlacementSimulationInput {
    pub filename: String,
    pub file_size: i64,
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub folder_policy_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePlacementClassification {
    pub filename: String,
    pub file_size: i64,
    pub extension: String,
    pub compound_extension: Option<String>,
    pub category: FileCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePlacementSimulationResult {
    pub classification: StoragePlacementClassification,
    pub admitted: bool,
    pub decision: Option<StorageRoutingDecision>,
    pub rejection_code: Option<String>,
    pub evaluated_rules: Vec<PlacementRuleEvaluation>,
    pub excluded_targets: Vec<(i64, TargetExclusionReason)>,
}

impl StoragePlacementContext {
    pub fn from_filename(profile_id: i64, filename: &str, file_size: i64, mime_type: &str) -> Self {
        let classification = classify_file(filename, mime_type);
        Self {
            profile_id,
            filename: filename.to_string(),
            file_size,
            extension: classification.extension,
            compound_extension: classification.compound_extension,
            category: classification.category,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementTarget {
    pub id: i64,
    pub policy_id: i64,
    pub weight: u32,
    pub stable_order: u32,
    pub is_enabled: bool,
    pub accepting_new_writes: bool,
    pub policy_max_file_size: i64,
    pub exclusion: Option<TargetExclusionReason>,
}

impl PlacementTarget {
    pub fn eligible_for(&self, file_size: i64) -> Result<(), TargetExclusionReason> {
        if !self.is_enabled {
            return Err(TargetExclusionReason::Disabled);
        }
        if !self.accepting_new_writes {
            return Err(TargetExclusionReason::Draining);
        }
        if let Some(reason) = self.exclusion {
            return Err(reason);
        }
        if self.policy_max_file_size > 0 && file_size > self.policy_max_file_size {
            return Err(TargetExclusionReason::PolicyFileSizeExceeded);
        }
        if self.weight == 0 {
            return Err(TargetExclusionReason::Incompatible);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementRule {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub priority: i32,
    pub is_enabled: bool,
    pub matcher: PlacementMatcher,
    pub selection_mode: PlacementSelectionMode,
    pub unavailable_behavior: PlacementUnavailableBehavior,
    pub targets: Vec<PlacementTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPlacementProfile {
    pub id: i64,
    pub revision: i64,
    pub is_enabled: bool,
    pub admission: StorageAdmissionConstraints,
    pub execution_preference: UploadExecutionPreference,
    pub rules: Vec<PlacementRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderPlacementOverride {
    pub policy_id: i64,
    pub policy_max_file_size: i64,
    pub is_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageRoutingDecision {
    pub profile_id: i64,
    pub revision: i64,
    pub rule_id: Option<i64>,
    pub policy_id: i64,
    pub selection_mode: PlacementSelectionMode,
    pub folder_override: bool,
    pub execution_preference: UploadExecutionPreference,
    pub excluded_targets: Vec<(i64, TargetExclusionReason)>,
    pub evaluated_rules: Vec<PlacementRuleEvaluation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PlacementRuleEvaluation {
    pub rule_id: i64,
    pub matched: bool,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementRejection {
    ProfileDisabled,
    AdmissionExtensionDenied,
    AdmissionCategoryDenied,
    AdmissionExtensionlessDenied,
    AdmissionFileTooLarge,
    NoMatchingRule,
    NoEligibleTarget,
    FolderPolicyUnavailable,
    FolderPolicyFileTooLarge,
}

impl PlacementRejection {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ProfileDisabled => "placement_profile_disabled",
            Self::AdmissionExtensionDenied => "placement_extension_denied",
            Self::AdmissionCategoryDenied => "placement_category_denied",
            Self::AdmissionExtensionlessDenied => "placement_extensionless_denied",
            Self::AdmissionFileTooLarge => "placement_file_too_large",
            Self::NoMatchingRule => "placement_no_matching_rule",
            Self::NoEligibleTarget => "placement_no_eligible_target",
            Self::FolderPolicyUnavailable => "folder_policy_unavailable",
            Self::FolderPolicyFileTooLarge => "folder_policy_file_too_large",
        }
    }

    const fn rejects_admission(&self) -> bool {
        matches!(
            self,
            Self::ProfileDisabled
                | Self::AdmissionExtensionDenied
                | Self::AdmissionCategoryDenied
                | Self::AdmissionExtensionlessDenied
                | Self::AdmissionFileTooLarge
        )
    }
}

enum PlacementResolution {
    Selected(StorageRoutingDecision),
    Rejected {
        rejection: PlacementRejection,
        evaluated_rules: Vec<PlacementRuleEvaluation>,
        excluded_targets: Vec<(i64, TargetExclusionReason)>,
    },
}

pub fn compile_admission(
    admission: StorageAdmissionConstraints,
) -> Result<StorageAdmissionConstraints, String> {
    let mut compiled = admission;
    compiled.allowed_extensions = normalize_unique_extensions(&compiled.allowed_extensions)?;
    compiled.denied_extensions = normalize_unique_extensions(&compiled.denied_extensions)?;
    validate_categories(&compiled.allowed_categories)?;
    validate_categories(&compiled.denied_categories)?;
    if compiled.max_file_size < 0 {
        return Err("admission max_file_size must be non-negative".to_string());
    }
    Ok(compiled)
}

pub fn compile_matcher(matcher: PlacementMatcher) -> Result<PlacementMatcher, String> {
    let mut compiled = matcher;
    compiled.extensions = normalize_unique_extensions(&compiled.extensions)?;
    compiled.compound_extensions = normalize_unique_extensions(&compiled.compound_extensions)?;
    validate_categories(&compiled.categories)?;
    if compiled.min_file_size < 0 || compiled.max_file_size < 0 {
        return Err("matcher file sizes must be non-negative".to_string());
    }
    if compiled.max_file_size > 0 && compiled.max_file_size <= compiled.min_file_size {
        return Err("matcher max_file_size must be greater than min_file_size".to_string());
    }
    Ok(compiled)
}

pub fn resolve_placement(
    profile: &CompiledPlacementProfile,
    context: &StoragePlacementContext,
    folder_override: Option<&FolderPlacementOverride>,
) -> Result<StorageRoutingDecision, PlacementRejection> {
    resolve_placement_with_random(profile, context, folder_override, None)
}

pub fn resolve_placement_with_random(
    profile: &CompiledPlacementProfile,
    context: &StoragePlacementContext,
    folder_override: Option<&FolderPlacementOverride>,
    random_draw: Option<u64>,
) -> Result<StorageRoutingDecision, PlacementRejection> {
    match resolve_placement_internal(profile, context, folder_override, random_draw) {
        PlacementResolution::Selected(decision) => Ok(decision),
        PlacementResolution::Rejected { rejection, .. } => Err(rejection),
    }
}

pub fn simulate_placement(
    profile: &CompiledPlacementProfile,
    context: &StoragePlacementContext,
    folder_override: Option<&FolderPlacementOverride>,
) -> StoragePlacementSimulationResult {
    let classification = StoragePlacementClassification {
        filename: context.filename.clone(),
        file_size: context.file_size,
        extension: context.extension.clone(),
        compound_extension: context.compound_extension.clone(),
        category: context.category.clone(),
    };
    match resolve_placement_internal(profile, context, folder_override, None) {
        PlacementResolution::Selected(decision) => StoragePlacementSimulationResult {
            classification,
            admitted: true,
            evaluated_rules: decision.evaluated_rules.clone(),
            excluded_targets: decision.excluded_targets.clone(),
            decision: Some(decision),
            rejection_code: None,
        },
        PlacementResolution::Rejected {
            rejection,
            evaluated_rules,
            excluded_targets,
        } => StoragePlacementSimulationResult {
            classification,
            admitted: !rejection.rejects_admission(),
            decision: None,
            rejection_code: Some(rejection.code().to_string()),
            evaluated_rules,
            excluded_targets,
        },
    }
}

fn resolve_placement_internal(
    profile: &CompiledPlacementProfile,
    context: &StoragePlacementContext,
    folder_override: Option<&FolderPlacementOverride>,
    random_draw: Option<u64>,
) -> PlacementResolution {
    let rejected = |rejection| PlacementResolution::Rejected {
        rejection,
        evaluated_rules: Vec::new(),
        excluded_targets: Vec::new(),
    };
    if !profile.is_enabled {
        return rejected(PlacementRejection::ProfileDisabled);
    }
    if profile.admission.max_file_size > 0 && context.file_size > profile.admission.max_file_size {
        return rejected(PlacementRejection::AdmissionFileTooLarge);
    }
    if let Err(rejection) = apply_admission(&profile.admission, context) {
        return rejected(rejection);
    }

    if let Some(folder) = folder_override {
        if !folder.is_available {
            return rejected(PlacementRejection::FolderPolicyUnavailable);
        }
        if folder.policy_max_file_size > 0 && context.file_size > folder.policy_max_file_size {
            return rejected(PlacementRejection::FolderPolicyFileTooLarge);
        }
        return PlacementResolution::Selected(StorageRoutingDecision {
            profile_id: profile.id,
            revision: profile.revision,
            rule_id: None,
            policy_id: folder.policy_id,
            selection_mode: PlacementSelectionMode::FirstAvailable,
            folder_override: true,
            execution_preference: profile.execution_preference,
            excluded_targets: Vec::new(),
            evaluated_rules: Vec::new(),
        });
    }

    let mut excluded_targets = Vec::new();
    let mut evaluated_rules = Vec::new();
    for rule in profile.rules.iter().filter(|rule| rule.is_enabled) {
        if !matches_matcher(&rule.matcher, context) {
            evaluated_rules.push(PlacementRuleEvaluation {
                rule_id: rule.id,
                matched: false,
                reason_code: Some("matcher_mismatch".to_string()),
            });
            continue;
        }
        evaluated_rules.push(PlacementRuleEvaluation {
            rule_id: rule.id,
            matched: true,
            reason_code: None,
        });
        let mut eligible = Vec::new();
        for target in &rule.targets {
            match target.eligible_for(context.file_size) {
                Ok(()) => eligible.push(target),
                Err(reason) => excluded_targets.push((target.policy_id, reason)),
            }
        }
        if eligible.is_empty() {
            if rule.unavailable_behavior == PlacementUnavailableBehavior::Reject {
                return PlacementResolution::Rejected {
                    rejection: PlacementRejection::NoEligibleTarget,
                    evaluated_rules,
                    excluded_targets,
                };
            }
            continue;
        }
        let selected = match rule.selection_mode {
            PlacementSelectionMode::FirstAvailable => eligible
                .iter()
                .min_by_key(|target| target.stable_order)
                .expect("eligible target list is non-empty"),
            PlacementSelectionMode::WeightedRandom => {
                let total_weight: u64 = eligible.iter().map(|target| target.weight as u64).sum();
                let draw = random_draw.unwrap_or_else(|| rand::rng().random_range(0..total_weight));
                let mut cursor = draw % total_weight;
                let mut selected = eligible[eligible.len() - 1];
                for target in eligible {
                    let weight = target.weight as u64;
                    if cursor < weight {
                        selected = target;
                        break;
                    }
                    cursor -= weight;
                }
                selected
            }
        };
        return PlacementResolution::Selected(StorageRoutingDecision {
            profile_id: profile.id,
            revision: profile.revision,
            rule_id: Some(rule.id),
            policy_id: selected.policy_id,
            selection_mode: rule.selection_mode,
            folder_override: false,
            execution_preference: profile.execution_preference,
            excluded_targets,
            evaluated_rules,
        });
    }

    let rejection = if excluded_targets.is_empty() {
        PlacementRejection::NoMatchingRule
    } else {
        PlacementRejection::NoEligibleTarget
    };
    PlacementResolution::Rejected {
        rejection,
        evaluated_rules,
        excluded_targets,
    }
}

fn apply_admission(
    admission: &StorageAdmissionConstraints,
    context: &StoragePlacementContext,
) -> Result<(), PlacementRejection> {
    if context.extension.is_empty() {
        if !admission.accept_extensionless {
            return Err(PlacementRejection::AdmissionExtensionlessDenied);
        }
    } else {
        if admission
            .denied_extensions
            .iter()
            .any(|value| value == &context.extension)
            || context
                .compound_extension
                .as_ref()
                .is_some_and(|value| admission.denied_extensions.iter().any(|item| item == value))
        {
            return Err(PlacementRejection::AdmissionExtensionDenied);
        }
        if !admission.allowed_extensions.is_empty()
            && !admission.allowed_extensions.iter().any(|value| {
                value == &context.extension
                    || context
                        .compound_extension
                        .as_ref()
                        .is_some_and(|compound| value == compound)
            })
        {
            return Err(PlacementRejection::AdmissionExtensionDenied);
        }
    }
    if admission.denied_categories.contains(&context.category) {
        return Err(PlacementRejection::AdmissionCategoryDenied);
    }
    if !admission.allowed_categories.is_empty()
        && !admission.allowed_categories.contains(&context.category)
    {
        return Err(PlacementRejection::AdmissionCategoryDenied);
    }
    Ok(())
}

fn matches_matcher(matcher: &PlacementMatcher, context: &StoragePlacementContext) -> bool {
    if context.file_size < matcher.min_file_size
        || (matcher.max_file_size > 0 && context.file_size >= matcher.max_file_size)
    {
        return false;
    }
    if let Some(extensionless) = matcher.extensionless
        && extensionless != context.extension.is_empty()
    {
        return false;
    }
    if !matcher.extensions.is_empty()
        && !matcher.extensions.iter().any(|value| {
            value == &context.extension
                || context
                    .compound_extension
                    .as_ref()
                    .is_some_and(|compound| value == compound)
        })
    {
        return false;
    }
    if !matcher.compound_extensions.is_empty()
        && !context.compound_extension.as_ref().is_some_and(|compound| {
            matcher
                .compound_extensions
                .iter()
                .any(|item| item == compound)
        })
    {
        return false;
    }
    matcher.categories.is_empty() || matcher.categories.contains(&context.category)
}

fn normalize_unique_extensions(values: &[String]) -> Result<Vec<String>, String> {
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        let normalized = normalize_extension_filter(value).map_err(|error| error.to_string())?;
        if !result.contains(&normalized) {
            result.push(normalized);
        }
    }
    Ok(result)
}

fn validate_categories(categories: &[FileCategory]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for category in categories {
        if !seen.insert(category.as_str()) {
            return Err(format!("duplicate category {}", category.as_str()));
        }
    }
    Ok(())
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> CompiledPlacementProfile {
        CompiledPlacementProfile {
            id: 7,
            revision: 3,
            is_enabled: true,
            admission: StorageAdmissionConstraints::default(),
            execution_preference: UploadExecutionPreference::Automatic,
            rules: vec![PlacementRule {
                id: 11,
                name: "Rule 1".to_string(),
                description: String::new(),
                priority: 1,
                is_enabled: true,
                matcher: PlacementMatcher::default(),
                selection_mode: PlacementSelectionMode::FirstAvailable,
                unavailable_behavior: PlacementUnavailableBehavior::NextRule,
                targets: vec![PlacementTarget {
                    id: 1,
                    policy_id: 101,
                    weight: 100,
                    stable_order: 1,
                    is_enabled: true,
                    accepting_new_writes: true,
                    policy_max_file_size: 0,
                    exclusion: None,
                }],
            }],
        }
    }

    fn context(filename: &str, size: i64) -> StoragePlacementContext {
        StoragePlacementContext::from_filename(7, filename, size, "application/octet-stream")
    }

    #[test]
    fn empty_matcher_is_catch_all() {
        let context = context("file.bin", 10);
        let decision = resolve_placement(&profile(), &context, None).unwrap();
        assert_eq!(decision.policy_id, 101);
        assert_eq!(decision.rule_id, Some(11));
    }

    #[test]
    fn empty_filename_matches_default_admission_and_catch_all() {
        let context = StoragePlacementContext::from_filename(7, "", 10, "application/octet-stream");
        let decision = resolve_placement(&profile(), &context, None).unwrap();
        assert_eq!(decision.policy_id, 101);
    }

    #[test]
    fn disabled_profile_is_rejected() {
        let mut profile = profile();
        profile.is_enabled = false;
        assert_eq!(
            resolve_placement(&profile, &context("file.bin", 10), None).unwrap_err(),
            PlacementRejection::ProfileDisabled
        );
    }

    #[test]
    fn admission_deny_takes_precedence_over_allow() {
        let mut profile = profile();
        profile.admission.allowed_extensions = vec!["jpg".to_string()];
        profile.admission.denied_extensions = vec!["jpg".to_string()];
        assert_eq!(
            resolve_placement(&profile, &context("file.jpg", 10), None).unwrap_err(),
            PlacementRejection::AdmissionExtensionDenied
        );
    }

    #[test]
    fn extensionless_can_be_explicitly_rejected() {
        let mut profile = profile();
        profile.admission.accept_extensionless = false;
        assert_eq!(
            resolve_placement(&profile, &context("README", 10), None).unwrap_err(),
            PlacementRejection::AdmissionExtensionlessDenied
        );
    }

    #[test]
    fn matcher_uses_compound_extension() {
        let mut profile = profile();
        profile.rules[0].matcher.compound_extensions = vec!["tar.gz".to_string()];
        assert!(resolve_placement(&profile, &context("archive.tar.gz", 10), None).is_ok());
        assert_eq!(
            resolve_placement(&profile, &context("archive.zip", 10), None).unwrap_err(),
            PlacementRejection::NoMatchingRule
        );
    }

    #[test]
    fn folder_override_runs_after_admission() {
        let mut profile = profile();
        profile.admission.allowed_extensions = vec!["jpg".to_string()];
        let folder = FolderPlacementOverride {
            policy_id: 202,
            policy_max_file_size: 0,
            is_available: true,
        };
        let decision =
            resolve_placement(&profile, &context("photo.jpg", 10), Some(&folder)).unwrap();
        assert_eq!(decision.policy_id, 202);
        assert!(decision.folder_override);
    }

    #[test]
    fn unavailable_next_rule_falls_through() {
        let mut profile = profile();
        profile.rules[0].targets[0].accepting_new_writes = false;
        profile.rules.push(PlacementRule {
            id: 12,
            name: "Rule 2".to_string(),
            description: String::new(),
            priority: 2,
            is_enabled: true,
            matcher: PlacementMatcher::default(),
            selection_mode: PlacementSelectionMode::FirstAvailable,
            unavailable_behavior: PlacementUnavailableBehavior::NextRule,
            targets: vec![PlacementTarget {
                id: 2,
                policy_id: 102,
                weight: 100,
                stable_order: 1,
                is_enabled: true,
                accepting_new_writes: true,
                policy_max_file_size: 0,
                exclusion: None,
            }],
        });
        let decision = resolve_placement(&profile, &context("file.bin", 10), None).unwrap();
        assert_eq!(decision.policy_id, 102);
        assert_eq!(decision.rule_id, Some(12));
    }

    #[test]
    fn reject_behavior_reports_no_target() {
        let mut profile = profile();
        profile.rules[0].targets[0].is_enabled = false;
        profile.rules[0].unavailable_behavior = PlacementUnavailableBehavior::Reject;
        assert_eq!(
            resolve_placement(&profile, &context("file.bin", 10), None).unwrap_err(),
            PlacementRejection::NoEligibleTarget
        );
    }

    #[test]
    fn exhausted_next_rules_report_exclusions_and_rule_trace() {
        let mut exhausted_profile = profile();
        exhausted_profile.rules[0].targets[0].is_enabled = false;
        let error =
            resolve_placement(&exhausted_profile, &context("file.bin", 10), None).unwrap_err();
        assert_eq!(error, PlacementRejection::NoEligibleTarget);

        let mut mismatch_profile = profile();
        mismatch_profile.rules[0].matcher.extensions = vec!["jpg".to_string()];
        let error =
            resolve_placement(&mismatch_profile, &context("file.bin", 10), None).unwrap_err();
        assert_eq!(error, PlacementRejection::NoMatchingRule);
    }

    #[test]
    fn weighted_random_uses_positive_weights() {
        let mut profile = profile();
        profile.rules[0].selection_mode = PlacementSelectionMode::WeightedRandom;
        profile.rules[0].targets.push(PlacementTarget {
            id: 3,
            policy_id: 102,
            weight: 30,
            stable_order: 2,
            is_enabled: true,
            accepting_new_writes: true,
            policy_max_file_size: 0,
            exclusion: None,
        });
        assert_eq!(
            resolve_placement_with_random(&profile, &context("file.bin", 10), None, Some(0))
                .unwrap()
                .policy_id,
            101
        );
        assert_eq!(
            resolve_placement_with_random(&profile, &context("file.bin", 10), None, Some(100))
                .unwrap()
                .policy_id,
            102
        );
    }

    #[test]
    fn compile_rejects_invalid_range_and_duplicate_extensions() {
        assert!(
            compile_matcher(PlacementMatcher {
                min_file_size: 10,
                max_file_size: 10,
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            compile_admission(StorageAdmissionConstraints {
                allowed_extensions: vec!["jpg".to_string(), ".jpg".to_string()],
                ..Default::default()
            })
            .is_ok()
        );
    }

    #[test]
    fn typed_payload_and_reason_codes_are_stable() {
        let payload = PlacementPayloadEnvelope::new(StorageAdmissionConstraints::default());
        assert_eq!(payload.format_version, PLACEMENT_PAYLOAD_FORMAT_VERSION);
        assert_eq!(payload.schema_version, PLACEMENT_PAYLOAD_SCHEMA_VERSION);
        assert_eq!(UploadExecutionPreference::Automatic.as_str(), "automatic");
        assert_eq!(
            PlacementSelectionMode::FirstAvailable.as_str(),
            "first_available"
        );
        assert_eq!(
            PlacementSelectionMode::WeightedRandom.as_str(),
            "weighted_random"
        );
        assert_eq!(
            UploadExecutionPreference::ForceServerStream.as_str(),
            "force_server_stream"
        );
        assert_eq!(TargetExclusionReason::Disabled.code(), "target_disabled");
        assert_eq!(TargetExclusionReason::Draining.code(), "target_draining");
        assert_eq!(
            TargetExclusionReason::Unavailable.code(),
            "target_unavailable"
        );
        assert_eq!(
            TargetExclusionReason::Incompatible.code(),
            "target_incompatible"
        );
        assert_eq!(
            TargetExclusionReason::PolicyFileSizeExceeded.code(),
            "policy_max_file_size_exceeded"
        );
        assert_eq!(
            PlacementRejection::NoMatchingRule.code(),
            "placement_no_matching_rule"
        );
        assert_eq!(
            PlacementRejection::NoEligibleTarget.code(),
            "placement_no_eligible_target"
        );
    }

    #[test]
    fn target_eligibility_covers_each_exclusion() {
        let mut target = PlacementTarget {
            id: 1,
            policy_id: 1,
            weight: 1,
            stable_order: 1,
            is_enabled: false,
            accepting_new_writes: true,
            policy_max_file_size: 0,
            exclusion: None,
        };
        assert_eq!(target.eligible_for(1), Err(TargetExclusionReason::Disabled));
        target.is_enabled = true;
        target.accepting_new_writes = false;
        assert_eq!(target.eligible_for(1), Err(TargetExclusionReason::Draining));
        target.accepting_new_writes = true;
        target.exclusion = Some(TargetExclusionReason::Unavailable);
        assert_eq!(
            target.eligible_for(1),
            Err(TargetExclusionReason::Unavailable)
        );
        target.exclusion = None;
        target.policy_max_file_size = 1;
        assert_eq!(
            target.eligible_for(2),
            Err(TargetExclusionReason::PolicyFileSizeExceeded)
        );
        target.policy_max_file_size = 0;
        target.weight = 0;
        assert_eq!(
            target.eligible_for(1),
            Err(TargetExclusionReason::Incompatible)
        );
    }

    #[test]
    fn category_and_size_admission_boundaries_are_enforced() {
        let mut profile = profile();
        profile.admission.allowed_categories = vec![FileCategory::Image];
        profile.admission.max_file_size = 10;
        let image = context("photo.jpg", 10);
        assert!(resolve_placement(&profile, &image, None).is_ok());
        let too_large = context("photo.jpg", 11);
        assert_eq!(
            resolve_placement(&profile, &too_large, None).unwrap_err(),
            PlacementRejection::AdmissionFileTooLarge
        );
        let code = context("main.rs", 1);
        assert_eq!(
            resolve_placement(&profile, &code, None).unwrap_err(),
            PlacementRejection::AdmissionCategoryDenied
        );
    }

    #[test]
    fn folder_override_unavailable_and_too_large_are_rejected() {
        let unavailable = FolderPlacementOverride {
            policy_id: 3,
            policy_max_file_size: 0,
            is_available: false,
        };
        assert_eq!(
            resolve_placement(&profile(), &context("file.bin", 1), Some(&unavailable)).unwrap_err(),
            PlacementRejection::FolderPolicyUnavailable
        );
        let too_large = FolderPlacementOverride {
            policy_id: 3,
            policy_max_file_size: 1,
            is_available: true,
        };
        assert_eq!(
            resolve_placement(&profile(), &context("file.bin", 2), Some(&too_large)).unwrap_err(),
            PlacementRejection::FolderPolicyFileTooLarge
        );
    }

    #[test]
    fn compile_and_matcher_reject_invalid_categories_and_sizes() {
        assert!(
            compile_admission(StorageAdmissionConstraints {
                max_file_size: -1,
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            compile_admission(StorageAdmissionConstraints {
                allowed_categories: vec![FileCategory::Image, FileCategory::Image],
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            compile_matcher(PlacementMatcher {
                min_file_size: -1,
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            compile_matcher(PlacementMatcher {
                categories: vec![FileCategory::Code, FileCategory::Code],
                ..Default::default()
            })
            .is_err()
        );
    }

    #[test]
    fn matcher_extensionless_and_category_conditions_are_distinct() {
        let mut profile = profile();
        profile.rules[0].matcher.extensionless = Some(true);
        let extensionless = StoragePlacementContext {
            profile_id: 7,
            filename: "README".to_string(),
            file_size: 1,
            extension: String::new(),
            compound_extension: None,
            category: FileCategory::Other,
        };
        assert!(matches_matcher(&profile.rules[0].matcher, &extensionless));
        assert_eq!(
            resolve_placement(&profile, &context("README.txt", 1), None).unwrap_err(),
            PlacementRejection::NoMatchingRule
        );
        profile.rules[0].matcher.extensionless = None;
        profile.rules[0].matcher.categories = vec![FileCategory::Image];
        assert!(resolve_placement(&profile, &context("a.jpg", 1), None).is_ok());
        assert_eq!(
            resolve_placement(&profile, &context("main.rs", 1), None).unwrap_err(),
            PlacementRejection::NoMatchingRule
        );
    }

    #[test]
    fn simulation_preserves_rejected_routing_trace_and_classification() {
        let mut profile = profile();
        profile.rules[0].targets[0].accepting_new_writes = false;

        let result = simulate_placement(&profile, &context("archive.tar.gz", 42), None);

        assert!(result.admitted);
        assert!(result.decision.is_none());
        assert_eq!(
            result.rejection_code.as_deref(),
            Some("placement_no_eligible_target")
        );
        assert_eq!(result.classification.filename, "archive.tar.gz");
        assert_eq!(result.classification.extension, "gz");
        assert_eq!(
            result.classification.compound_extension.as_deref(),
            Some("tar.gz")
        );
        assert_eq!(
            result.evaluated_rules,
            vec![PlacementRuleEvaluation {
                rule_id: 11,
                matched: true,
                reason_code: None,
            }]
        );
        assert_eq!(
            result.excluded_targets,
            vec![(101, TargetExclusionReason::Draining)]
        );
    }

    #[test]
    fn simulation_distinguishes_admission_rejection_from_routing_rejection() {
        let mut profile = profile();
        profile.admission.denied_extensions = vec!["exe".to_string()];

        let result = simulate_placement(&profile, &context("setup.exe", 42), None);

        assert!(!result.admitted);
        assert_eq!(
            result.rejection_code.as_deref(),
            Some("placement_extension_denied")
        );
        assert!(result.evaluated_rules.is_empty());
        assert!(result.excluded_targets.is_empty());
    }

    #[test]
    fn target_exclusion_reasons_serialize_as_stable_codes() {
        assert_eq!(
            serde_json::to_string(&TargetExclusionReason::PolicyFileSizeExceeded).unwrap(),
            "\"policy_max_file_size_exceeded\""
        );
    }

    #[test]
    fn rejection_codes_cover_all_public_outcomes() {
        for rejection in [
            PlacementRejection::ProfileDisabled,
            PlacementRejection::AdmissionExtensionDenied,
            PlacementRejection::AdmissionCategoryDenied,
            PlacementRejection::AdmissionExtensionlessDenied,
            PlacementRejection::AdmissionFileTooLarge,
            PlacementRejection::NoMatchingRule,
            PlacementRejection::NoEligibleTarget,
            PlacementRejection::FolderPolicyUnavailable,
            PlacementRejection::FolderPolicyFileTooLarge,
        ] {
            assert!(!rejection.code().is_empty());
        }
    }
}
