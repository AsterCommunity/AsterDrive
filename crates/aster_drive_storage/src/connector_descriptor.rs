//! Storage connector descriptors for admin policy UI capability discovery.
//!
//! Descriptor 是 connector 对外声明的“配置/管理能力清单”。前端用它决定显示哪些
//! 字段、按钮和提示；后端服务也用它 gate 授权、连接测试、policy action 等入口。
//! 它不是 runtime driver，本文件不应该承载实际对象读写逻辑。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use aster_drive_model::types::OBJECT_MULTIPART_MIN_PART_SIZE;

use crate::ConnectorId;

use super::field_contract::{StorageDescriptorFieldKind, StorageDescriptorFieldSemantics};

/// Credential modes a connector may require.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorCredentialMode {
    /// 不需要密钥或远端绑定，例如纯本地路径。
    None,
    /// 使用 connector 自己定义字段的静态凭据。
    StaticSecret,
    /// 通过已注册 remote node 代理访问。
    RemoteNode,
    /// 需要用户完成 delegated OAuth 授权，例如 Microsoft Graph OneDrive。
    OauthDelegated,
}

/// Connector-backed policy data is visible from which primary instances.
///
/// This is a static connector capability. Deployment-specific filtering and
/// write guards must consume this field instead of maintaining a separate
/// core-owned provider allow/deny list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorDeploymentScope {
    /// Policy data lives on the primary instance itself and is not shared with
    /// other primary instances.
    InstanceLocal,
    /// Whether every primary resolves the same storage data plane is owned by
    /// the deployment. The connector is cluster-eligible when operators mount
    /// and configure it according to the product's shared-state contract.
    DeploymentManaged,
    /// Every primary instance can safely resolve the same policy reference.
    SharedAcrossPrimaryInstances,
}

impl StorageConnectorDeploymentScope {
    /// Stable wire name used in serialized descriptor payloads and diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InstanceLocal => "instance_local",
            Self::DeploymentManaged => "deployment_managed",
            Self::SharedAcrossPrimaryInstances => "shared_across_primary_instances",
        }
    }

    /// Whether this scope allows the same policy to resolve identically on
    /// every primary instance, enabling cluster-wide storage reuse.
    pub const fn is_cluster_eligible(self) -> bool {
        matches!(
            self,
            Self::DeploymentManaged | Self::SharedAcrossPrimaryInstances
        )
    }
}

/// Which persisted config domain a descriptor field writes into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorFieldScope {
    /// 写入当前 connector namespace 下的版本化 typed config envelope。
    ConnectorConfig,
    /// 写入独立的静态凭据通道，不进入 connector config JSON。
    StaticCredential,
    /// 写入授权应用配置，例如 OAuth client id/secret；不直接作为 driver 凭据。
    AuthorizationApplication,
    /// 仅作为一次 connector action 的输入，不进入 policy 或 credential 持久化。
    ActionInput,
}

/// Widget and validation class of one descriptor field in the admin form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorFieldKind {
    /// Single-line free-form text value.
    Text,
    /// Credential value; the UI masks it and the backend never echoes it back.
    Secret,
    /// One value chosen from the select contract declared on the field.
    Select,
    /// A `true`/`false` switch.
    Boolean,
    /// An integer value such as a timeout or limit.
    Number,
}

impl StorageConnectorFieldKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "string",
            Self::Secret => "secret string",
            Self::Select => "string",
            Self::Boolean => "boolean",
            Self::Number => "integer",
        }
    }
}

impl From<StorageDescriptorFieldKind> for StorageConnectorFieldKind {
    fn from(value: StorageDescriptorFieldKind) -> Self {
        match value {
            StorageDescriptorFieldKind::Text => Self::Text,
            StorageDescriptorFieldKind::Secret => Self::Secret,
            StorageDescriptorFieldKind::Select => Self::Select,
            StorageDescriptorFieldKind::Boolean => Self::Boolean,
            StorageDescriptorFieldKind::Number => Self::Number,
        }
    }
}

impl From<StorageConnectorFieldKind> for StorageDescriptorFieldKind {
    fn from(value: StorageConnectorFieldKind) -> Self {
        match value {
            StorageConnectorFieldKind::Text => Self::Text,
            StorageConnectorFieldKind::Secret => Self::Secret,
            StorageConnectorFieldKind::Select => Self::Select,
            StorageConnectorFieldKind::Boolean => Self::Boolean,
            StorageConnectorFieldKind::Number => Self::Number,
        }
    }
}

/// Scalar type submitted by a select field.
///
/// Select is a UI control, not a wire type. Keeping the value type explicit is
/// required for dynamic choices such as a numeric remote node id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorSelectValueKind {
    /// Options carry string values.
    String,
    /// Options carry integer values, for example remote node ids.
    Integer,
}

/// Scalar value carried by one select option.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorSelectOptionValue {
    /// Submitted when the select's `value_kind` is `integer`.
    Integer(i64),
    /// Submitted when the select's `value_kind` is `string`.
    String(String),
}

/// One selectable choice of a fixed-options select field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorSelectOption {
    /// Stable value submitted in the connector payload.
    pub value: StorageConnectorSelectOptionValue,
    /// Connector-owned frontend localization key.
    pub label_key: String,
    /// Optional connector-owned explanation for richer selectors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_key: Option<String>,
    /// Conditions that must all match before this option is offered by the UI.
    /// The connector still performs authoritative semantic validation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub available_when: Vec<StorageConnectorFieldCondition>,
}

/// Platform-provided option catalogs that a connector field can consume.
///
/// The platform owns loading these catalogs. The connector only opts into one
/// and declares field dependencies, so the UI never infers behavior from a
/// provider id or field name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorSelectDataSource {
    /// Registered remote nodes; integer values, no `depends_on`.
    RemoteNodes,
    /// Storage targets exposed by the remote node selected in `depends_on`;
    /// string values.
    RemoteStorageTargets,
}

/// UI contract of one select field: value type, fixed options or dynamic
/// platform catalog, and custom-value behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorSelectDescriptor {
    /// Value shape shared by every option of this select field.
    pub value_kind: StorageConnectorSelectValueKind,
    /// Connector-owned fixed choices. Mutually exclusive with `data_source`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<StorageConnectorSelectOption>,
    /// Platform catalog used to populate choices at runtime.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_source: Option<StorageConnectorSelectDataSource>,
    /// Field whose current value scopes the dynamic catalog.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<String>,
    /// Connector-owned label for a synthetic option that clears the explicit
    /// value and resumes descriptor default resolution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automatic_default_label_key: Option<String>,
    /// Whether a string select may accept a value outside its fixed presets.
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_custom_value: bool,
    /// Connector-owned label for the synthetic custom-value option.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_value_label_key: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Descriptor 可声明的 JSON 标量默认值。
///
/// Provider option 只允许标量配置；credential secret 使用独立 credential/application
/// config 通道，复杂对象也应拆成有明确字段 contract 的标量集合。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorFieldDefaultValue {
    /// Default for a boolean field.
    Boolean(bool),
    /// Default for a number (integer) field.
    Integer(i64),
    /// Default for a text, secret, or string-valued select field.
    String(String),
}

/// One connector field equals (or explicitly differs from) one scalar value.
///
/// Conditions deliberately stay small and inspectable. Multiple conditions on
/// one rule are ANDed; connectors can declare multiple ordered rules when they
/// need OR behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorFieldCondition {
    /// Omitted means the field's own scope. Static credential visibility and
    /// requiredness may explicitly depend on non-secret connector config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<StorageConnectorFieldScope>,
    /// Name of the sibling field whose value is inspected.
    pub field: String,
    /// Scalar value the referenced field must carry (or must not, when
    /// `negate` is set).
    pub value: StorageConnectorFieldDefaultValue,
    /// Require a present value different from `value`. Missing and null values never
    /// match, including negated conditions.
    #[serde(default, skip_serializing_if = "is_false")]
    pub negate: bool,
}

/// Ordered connector-owned default selected from the current field values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorFieldDefaultRule {
    /// All conditions in this rule must match for it to apply.
    pub conditions: Vec<StorageConnectorFieldCondition>,
    /// Scalar default applied when this rule wins.
    pub value: StorageConnectorFieldDefaultValue,
}

/// What happens to a field's submitted value when it is not rendered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorInactiveValueBehavior {
    /// Keep the submitted value even though the field is not rendered.
    #[default]
    Preserve,
    /// Drop the value when the field is not rendered.
    Clear,
}

impl StorageConnectorInactiveValueBehavior {
    fn is_preserve(&self) -> bool {
        *self == Self::Preserve
    }
}

/// Controls when a connector-declared field default is applied.
///
/// Missing values use the descriptor default in both modes. Empty text is
/// distinct because some connector fields model an omitted optional value,
/// while others use an empty form value to request a connector-owned root or
/// local default path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorFieldDefaultMode {
    /// Apply the default only when the field is absent.
    #[default]
    MissingOnly,
    /// Apply the default when the field is absent or an optional text field is
    /// submitted empty.
    MissingOrEmptyText,
}

impl StorageConnectorFieldDefaultMode {
    fn is_missing_only(&self) -> bool {
        *self == Self::MissingOnly
    }
}

/// Basic numeric and length constraints re-checked by the backend, usable by
/// the frontend for immediate feedback.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorFieldValidation {
    /// Lower bound accepted for a number field, inclusive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_integer: Option<i64>,
    /// Upper bound accepted for a number field, inclusive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_integer: Option<i64>,
    /// Maximum character count accepted for a text/secret field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
}

/// Stable action identity owned by one connector.
///
/// The descriptor carries the action's fields and execution contract. This
/// newtype only prevents action IDs from being mixed with connector IDs and
/// arbitrary field names while crossing registry, API, and audit boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(ToSchema),
    schema(value_type = String)
)]
/// Stable action identity owned by one connector.
pub struct StorageConnectorActionId(String);

impl StorageConnectorActionId {
    /// Wraps a connector-declared action id without validating it.
    pub fn declared(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the action id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Checks the id against [`is_valid_stable_descriptor_id`].
    pub fn validate(&self) -> Result<(), StorageConnectorActionIdError> {
        if !is_valid_stable_descriptor_id(self.as_str()) {
            return Err(StorageConnectorActionIdError);
        }
        Ok(())
    }
}

/// Returned when an action id fails stable-id validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConnectorActionIdError;

impl fmt::Display for StorageConnectorActionIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
			"action id must be 3-128 lowercase ASCII letters, digits, '.', '-' or '_' and may not start or end with punctuation",
		)
    }
}

impl std::error::Error for StorageConnectorActionIdError {}

/// Stable identity for one target-owned in-place connector promotion.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(ToSchema),
    schema(value_type = String)
)]
/// Stable identity for one target-owned in-place connector promotion.
pub struct StorageConnectorPromotionId(String);

impl StorageConnectorPromotionId {
    /// Wraps a target-declared promotion id without validating it.
    pub fn declared(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the promotion id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Checks the id against [`is_valid_stable_descriptor_id`].
    pub fn validate(&self) -> Result<(), StorageConnectorPromotionIdError> {
        if !is_valid_stable_descriptor_id(self.as_str()) {
            return Err(StorageConnectorPromotionIdError);
        }
        Ok(())
    }
}

/// Returned when a promotion id fails stable-id validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConnectorPromotionIdError;

impl fmt::Display for StorageConnectorPromotionIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "promotion id must be 3-128 lowercase ASCII letters, digits, '.', '-' or '_' and may not start or end with punctuation",
        )
    }
}

impl std::error::Error for StorageConnectorPromotionIdError {}

fn is_valid_stable_descriptor_id(value: &str) -> bool {
    (3..=128).contains(&value.len())
        && !value.starts_with(['.', '-', '_'])
        && !value.ends_with(['.', '-', '_'])
        && !value.contains("..")
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b".-_".contains(&byte)
        })
}

/// Descriptor-visible source-value matcher used for promotion discovery.
///
/// These matchers intentionally stay portable across Rust and browser URL
/// implementations. Provider-specific semantic validation still runs through
/// the target connector before any policy row is changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorPromotionValueMatcher {
    /// Matches when the source value equals the expected text.
    StringEquals {
        /// Expected source value.
        value: String,
        /// Compare case-insensitively when `false` (the serialized default).
        #[serde(default)]
        case_sensitive: bool,
    },
    /// Matches when the source value ends with the given text.
    StringSuffix {
        /// Text the source value must end with.
        suffix: String,
        /// Compare case-insensitively when `false` (the serialized default).
        #[serde(default)]
        case_sensitive: bool,
    },
    /// Matches when the source value starts with the given text.
    StringPrefix {
        /// Text the source value must start with.
        prefix: String,
        /// Compare case-insensitively when `false` (the serialized default).
        #[serde(default)]
        case_sensitive: bool,
    },
    /// Matches when the URL host ends with the given DNS label boundary,
    /// for example `.example.com`.
    UrlHostSuffix {
        /// Host suffix including its leading `.` or `-` label boundary.
        suffix: String,
    },
    /// Matches when any dot-separated DNS label of the URL host equals the
    /// given label.
    UrlHostContainsLabel {
        /// Exact DNS label searched for in the URL host.
        label: String,
    },
    /// Matches when any DNS label of the URL host equals the string value of
    /// another connector config field.
    UrlHostContainsField {
        /// Connector config field whose string value is compared against
        /// each DNS label of the URL host.
        field: String,
    },
    /// Matches when the URL host ends with any of the given DNS label
    /// boundaries.
    UrlHostSuffixAny {
        /// Acceptable host suffixes, each including its leading `.` or `-`
        /// label boundary.
        suffixes: Vec<String>,
    },
}

/// One source-condition a saved policy must satisfy before a promotion is
/// offered or executed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorPromotionRequirement {
    /// Source connector config field inspected by the generic recommendation
    /// and server-side eligibility checks.
    pub source_field: String,
    /// Condition the source field value must satisfy.
    pub matcher: StorageConnectorPromotionValueMatcher,
    /// Negate the matcher result. This keeps exclusions composable without
    /// adding a second inverse variant for every matcher kind.
    #[serde(default)]
    pub negate: bool,
}

/// One source-to-target field copy rule of a promotion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorPromotionFieldMapping {
    /// Field name in the source connector's config or credential payload.
    pub source_field: String,
    /// Field name declared by this target connector.
    pub target_field: String,
    /// Require the target connector's normalized value to remain exactly equal
    /// to the normalized source value. Use this for object-namespace fields.
    #[serde(default)]
    pub preserve_value: bool,
}

/// Target-owned contract for promoting a compatible saved policy in place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorPromotionDescriptor {
    /// Stable id of this promotion contract, unique within the target
    /// connector.
    pub promotion_id: StorageConnectorPromotionId,
    /// Connector whose saved policies are eligible for this promotion.
    pub source_connector_id: ConnectorId,
    /// Target-owned recommendation copy rendered by the generic UI.
    pub description_key: String,
    /// Target-owned confirmation copy rendered before mutating a saved policy.
    pub confirmation_key: String,
    /// Source-field conditions a saved policy must satisfy before the
    /// generic UI recommends this promotion.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<StorageConnectorPromotionRequirement>,
    /// Mapping of source connector config fields into this connector's
    /// connector-config fields; at least one mapping is required.
    pub config_mappings: Vec<StorageConnectorPromotionFieldMapping>,
    /// Mapping of source credential fields into this connector's static
    /// credential fields.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credential_mappings: Vec<StorageConnectorPromotionFieldMapping>,
}

impl StorageConnectorPromotionDescriptor {
    fn validate(&self) -> Result<(), StorageConnectorDescriptorError> {
        self.promotion_id.validate().map_err(|error| {
            StorageConnectorDescriptorError(format!(
                "promotion '{}' has invalid id: {error}",
                self.promotion_id.as_str()
            ))
        })?;
        self.source_connector_id.validate().map_err(|error| {
            StorageConnectorDescriptorError(format!(
                "promotion '{}' has invalid source connector id: {error}",
                self.promotion_id.as_str()
            ))
        })?;
        if self.description_key.trim().is_empty() || self.confirmation_key.trim().is_empty() {
            return Err(StorageConnectorDescriptorError(format!(
                "promotion '{}' requires non-empty description and confirmation keys",
                self.promotion_id.as_str()
            )));
        }
        for requirement in &self.requirements {
            if requirement.source_field.trim().is_empty() {
                return Err(StorageConnectorDescriptorError(format!(
                    "promotion '{}' requirement source field must not be empty",
                    self.promotion_id.as_str()
                )));
            }
            let matcher_value = match &requirement.matcher {
                StorageConnectorPromotionValueMatcher::StringEquals { value, .. } => value,
                StorageConnectorPromotionValueMatcher::StringSuffix { suffix, .. }
                | StorageConnectorPromotionValueMatcher::UrlHostSuffix { suffix } => suffix,
                StorageConnectorPromotionValueMatcher::UrlHostContainsLabel { label } => label,
                StorageConnectorPromotionValueMatcher::UrlHostContainsField { field } => field,
                StorageConnectorPromotionValueMatcher::UrlHostSuffixAny { suffixes } => {
                    suffixes.first().map(String::as_str).unwrap_or_default()
                }
                StorageConnectorPromotionValueMatcher::StringPrefix { prefix, .. } => prefix,
            };
            if matcher_value.trim().is_empty() {
                return Err(StorageConnectorDescriptorError(format!(
                    "promotion '{}' requirement matcher must not be empty",
                    self.promotion_id.as_str()
                )));
            }
            if let StorageConnectorPromotionValueMatcher::UrlHostSuffix { suffix } =
                &requirement.matcher
                && (!(suffix.starts_with('.') || suffix.starts_with('-')) || suffix.len() == 1)
            {
                return Err(StorageConnectorDescriptorError(format!(
                    "promotion '{}' URL host suffix matcher must start with a DNS label boundary marker '.' or '-'",
                    self.promotion_id.as_str()
                )));
            }
            if let StorageConnectorPromotionValueMatcher::UrlHostSuffixAny { suffixes } =
                &requirement.matcher
                && (suffixes.is_empty()
                    || suffixes.iter().any(|suffix| {
                        suffix.len() == 1 || !(suffix.starts_with('.') || suffix.starts_with('-'))
                    }))
            {
                return Err(StorageConnectorDescriptorError(format!(
                    "promotion '{}' URL host suffix matchers must use DNS label boundary markers '.' or '-'",
                    self.promotion_id.as_str()
                )));
            }
        }
        if self.config_mappings.is_empty() {
            return Err(StorageConnectorDescriptorError(format!(
                "promotion '{}' must declare at least one config mapping",
                self.promotion_id.as_str()
            )));
        }
        validate_promotion_mappings(self.promotion_id.as_str(), &self.config_mappings)?;
        validate_promotion_mappings(self.promotion_id.as_str(), &self.credential_mappings)?;
        Ok(())
    }
}

fn validate_promotion_mappings(
    promotion_id: &str,
    mappings: &[StorageConnectorPromotionFieldMapping],
) -> Result<(), StorageConnectorDescriptorError> {
    let mut target_fields = HashSet::with_capacity(mappings.len());
    for mapping in mappings {
        if mapping.source_field.trim().is_empty() || mapping.target_field.trim().is_empty() {
            return Err(StorageConnectorDescriptorError(format!(
                "promotion '{promotion_id}' field mappings must use non-empty names"
            )));
        }
        if !target_fields.insert(mapping.target_field.as_str()) {
            return Err(StorageConnectorDescriptorError(format!(
                "promotion '{promotion_id}' maps target field '{}' more than once",
                mapping.target_field
            )));
        }
    }
    Ok(())
}

/// Which backend entry this action routes through, and which endpoint kinds
/// it may declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorActionKind {
    /// Connector/plugin 自定义动作，可能修改远端状态。
    Custom,
    /// 授权流程入口。
    Authorization,
    /// 已授权 credential 校验入口。
    CredentialValidation,
    /// 连接测试入口。
    ConnectionTest,
}

/// Backend endpoint through which an action can be invoked. Each action kind
/// accepts only the endpoints matched by [`action_kind_accepts_endpoint`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorActionEndpoint {
    /// Execute a custom action on unsaved draft policy parameters.
    ExecuteDraftStoragePolicyAction,
    /// Execute a custom action on an already saved policy.
    ExecuteSavedStoragePolicyAction,
    /// Launch the delegated authorization flow for a saved policy.
    StartStorageAuthorization,
    /// Validate the stored credential of a saved policy.
    ValidateStoragePolicyCredential,
    /// Test connectivity using draft policy parameters.
    TestPolicyParams,
    /// Test connectivity of an already saved policy.
    TestPolicyConnection,
}

/// Wire shape of one structured action output field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorActionOutputValueKind {
    /// A string value rendered as text.
    Text,
    /// A numeric value.
    Number,
    /// A boolean rendered via `true_key`/`false_key` labels.
    Boolean,
    /// A list of strings rendered as multiple lines or chips.
    StringList,
}

/// Presentation contract for one structured action output field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorActionOutputFieldDescriptor {
    /// Top-level key read from `StoragePolicyActionResult.output`.
    pub name: String,
    /// Connector-owned localization key used as the field label.
    pub label_key: String,
    /// Wire value shape accepted by the generic presenter.
    pub value_kind: StorageConnectorActionOutputValueKind,
    /// Connector-owned label used when a boolean value is true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub true_key: Option<String>,
    /// Connector-owned label used when a boolean value is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub false_key: Option<String>,
}

impl StorageConnectorActionOutputFieldDescriptor {
    fn validate(&self) -> Result<(), StorageConnectorActionDescriptorError> {
        if self.name.trim().is_empty() {
            return Err(StorageConnectorActionDescriptorError(
                "action output field name must not be empty".to_string(),
            ));
        }
        if self.label_key.trim().is_empty() {
            return Err(StorageConnectorActionDescriptorError(format!(
                "action output field '{}' label_key must not be empty",
                self.name
            )));
        }
        match self.value_kind {
            StorageConnectorActionOutputValueKind::Boolean => {
                for (name, message_id) in [
                    ("true_key", self.true_key.as_deref()),
                    ("false_key", self.false_key.as_deref()),
                ] {
                    if message_id.is_none_or(|message_id| message_id.trim().is_empty()) {
                        return Err(StorageConnectorActionDescriptorError(format!(
                            "boolean action output field '{}' requires non-empty {name}",
                            self.name
                        )));
                    }
                }
            }
            _ if self.true_key.is_some() || self.false_key.is_some() => {
                return Err(StorageConnectorActionDescriptorError(format!(
                    "non-boolean action output field '{}' must not declare boolean labels",
                    self.name
                )));
            }
            _ => {}
        }
        Ok(())
    }
}

/// One executable connector action: id, localization, routing endpoints,
/// input/output schema, and execution preconditions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorActionDescriptor {
    /// Connector 内唯一且稳定的 action ID。
    pub action_id: StorageConnectorActionId,
    /// 前端本地化 label key。
    pub label_key: String,
    /// 前端本地化说明 key。
    pub description_key: String,
    /// 用于把 action 归类到授权、连接测试、policy action 等入口。
    pub kind: StorageConnectorActionKind,
    /// 该 action 可通过哪些后端 endpoint 执行。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoints: Vec<StorageConnectorActionEndpoint>,
    /// Action-owned input schema. Values are never persisted into the policy.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<StorageConnectorFieldDescriptor>,
    /// Connector-owned presentation schema for successful structured output.
    ///
    /// Action output remains transient and is never copied into policy config.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_fields: Vec<StorageConnectorActionOutputFieldDescriptor>,
    /// true 表示必须先保存 policy，draft 参数不能执行。
    pub requires_saved_policy: bool,
    /// true 表示执行前必须存在可用授权凭据。
    pub requires_authorization: bool,
    /// true 表示该动作会修改 provider 远端状态。
    pub mutates_remote_state: bool,
    /// true 表示 UI 在执行前应展示明确确认步骤。
    pub requires_confirmation: bool,
}

impl StorageConnectorActionDescriptor {
    /// Validate the connector-owned action schema before it enters a catalog.
    ///
    /// Registration-time validation prevents a plugin from advertising a UI
    /// contract that the generic dispatcher cannot route or normalize.
    pub fn validate(&self) -> Result<(), StorageConnectorActionDescriptorError> {
        self.action_id
            .validate()
            .map_err(|error| StorageConnectorActionDescriptorError(error.to_string()))?;
        if self.label_key.trim().is_empty() {
            return Err(StorageConnectorActionDescriptorError(
                "action label_key must not be empty".to_string(),
            ));
        }
        if self.description_key.trim().is_empty() {
            return Err(StorageConnectorActionDescriptorError(
                "action description_key must not be empty".to_string(),
            ));
        }
        if self.endpoints.is_empty() {
            return Err(StorageConnectorActionDescriptorError(
                "action must declare at least one endpoint".to_string(),
            ));
        }
        let mut endpoints = Vec::with_capacity(self.endpoints.len());
        for endpoint in &self.endpoints {
            if endpoints.contains(endpoint) {
                return Err(StorageConnectorActionDescriptorError(
                    "action must not declare the same endpoint more than once".to_string(),
                ));
            }
            endpoints.push(*endpoint);
            if !action_kind_accepts_endpoint(self.kind, *endpoint) {
                return Err(StorageConnectorActionDescriptorError(format!(
                    "action kind {:?} does not accept endpoint {:?}",
                    self.kind, endpoint
                )));
            }
        }

        let has_draft_endpoint = self.endpoints.iter().any(|endpoint| {
            matches!(
                endpoint,
                StorageConnectorActionEndpoint::ExecuteDraftStoragePolicyAction
                    | StorageConnectorActionEndpoint::TestPolicyParams
            )
        });
        if self.requires_saved_policy == has_draft_endpoint {
            return Err(StorageConnectorActionDescriptorError(
                "requires_saved_policy must be false exactly when a draft endpoint is declared"
                    .to_string(),
            ));
        }

        let mut field_names = HashSet::with_capacity(self.fields.len());
        for field in &self.fields {
            field.validate().map_err(|error| {
                StorageConnectorActionDescriptorError(format!(
                    "action field '{}' is invalid: {error}",
                    field.name
                ))
            })?;
            if field.scope != StorageConnectorFieldScope::ActionInput {
                return Err(StorageConnectorActionDescriptorError(format!(
                    "action field '{}' must use action_input scope",
                    field.name
                )));
            }
            if !field_names.insert(field.name.as_str()) {
                return Err(StorageConnectorActionDescriptorError(format!(
                    "action field '{}' is declared more than once",
                    field.name
                )));
            }
        }
        let mut output_field_names = HashSet::with_capacity(self.output_fields.len());
        for field in &self.output_fields {
            field.validate()?;
            if !output_field_names.insert(field.name.as_str()) {
                return Err(StorageConnectorActionDescriptorError(format!(
                    "action output field '{}' is declared more than once",
                    field.name
                )));
            }
        }
        Ok(())
    }
}

fn action_kind_accepts_endpoint(
    kind: StorageConnectorActionKind,
    endpoint: StorageConnectorActionEndpoint,
) -> bool {
    matches!(
        (kind, endpoint),
        (
            StorageConnectorActionKind::Custom,
            StorageConnectorActionEndpoint::ExecuteDraftStoragePolicyAction
                | StorageConnectorActionEndpoint::ExecuteSavedStoragePolicyAction
        ) | (
            StorageConnectorActionKind::Authorization,
            StorageConnectorActionEndpoint::StartStorageAuthorization
        ) | (
            StorageConnectorActionKind::CredentialValidation,
            StorageConnectorActionEndpoint::ValidateStoragePolicyCredential
        ) | (
            StorageConnectorActionKind::ConnectionTest,
            StorageConnectorActionEndpoint::TestPolicyParams
                | StorageConnectorActionEndpoint::TestPolicyConnection
        )
    )
}

/// Returned when an action descriptor fails catalog registration validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConnectorActionDescriptorError(String);

impl fmt::Display for StorageConnectorActionDescriptorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for StorageConnectorActionDescriptorError {}

/// Connector 声明支持的全部上传工作流开关与逐项语义。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorUploadWorkflows {
    /// 后端/客户端可以用单请求写入小对象。
    pub simple_upload: bool,
    /// 单请求上传的静态语义。实际是否走 direct 仍受 policy chunk_size 限制。
    pub simple_upload_capabilities: StorageConnectorSimpleUploadCapabilities,
    /// 后端可以通过 `StreamUploadDriver` 把 reader 写入 provider。
    pub stream_upload: bool,
    /// 支持对象存储 multipart/block upload 语义。
    pub object_multipart_upload: bool,
    /// 对象存储 multipart/block upload 的具体语义。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_multipart_upload_capabilities:
        Option<StorageConnectorObjectMultipartUploadCapabilities>,
    /// 支持 provider-native resumable/session upload。
    pub provider_resumable_upload: bool,
    /// 支持浏览器/客户端使用 presigned URL 直传。
    pub presigned_upload: bool,
    /// 是否允许前端直接拿 provider-native session 上传。
    pub frontend_direct_provider_resumable_upload: bool,
    /// Provider-native resumable/session upload 的具体语义。
    ///
    /// 该字段只描述 provider 自己的 session/range 协议，例如 Microsoft Graph
    /// upload session。S3-compatible multipart/block upload 不应填这里。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_resumable_upload_capabilities:
        Option<StorageConnectorProviderResumableUploadCapabilities>,
}

/// 单请求 simple upload 的具体语义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorSimpleUploadCapabilities {
    /// true 表示浏览器把对象发给 AsterDrive，由后端 relay 到 provider。
    pub server_side_relay: bool,
    /// true 表示单请求 direct/relay 上限由具体 policy chunk_size 决定。
    pub policy_limited: bool,
    /// Provider 自身单请求 API 的最大对象大小；None 表示当前 connector 不声明静态上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_provider_single_request_size: Option<u64>,
}

/// 对象存储 multipart/block upload 的逐项约束。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorObjectMultipartUploadCapabilities {
    /// Provider 最小非 final part 大小。
    pub min_part_size: i64,
    /// true 表示实际 part size 由 policy chunk_size 决定，但会被 min_part_size 修正。
    pub policy_limited_part_size: bool,
    /// AsterDrive 服务端是否可以 relay 上传 part。
    pub relay_part_upload: bool,
    /// 浏览器是否可以通过 presigned URL 直传 part。
    pub presigned_part_upload: bool,
    /// 浏览器直传 part 后是否必须从响应读取 ETag。
    ///
    /// Azure block upload 通过 URL 中的 blockid 作为 completion token，因此不要求
    /// 浏览器能读 ETag；S3-compatible multipart 通常需要 ETag。
    pub presigned_part_etag_required: bool,
    /// complete 阶段是否需要显式提交 part 列表。
    pub explicit_complete_required: bool,
    /// 是否支持清理未完成的 provider multipart/block upload。
    pub abort_supported: bool,
    /// 是否支持查询 provider 已接收的 part/block 列表。
    pub list_parts_supported: bool,
}

/// Provider-native resumable/session upload 的逐项参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorProviderResumableUploadCapabilities {
    /// Backend-coordinated browser execution protocol; opaque to the frontend.
    pub browser_upload_protocol: Option<crate::BrowserUploadProtocol>,
    /// Provider 标识，例如 `microsoft_graph`。
    pub provider: String,
    /// 面向 UI/诊断的 session 名称，例如 `Microsoft Graph upload session`。
    pub session_label: String,
    /// Provider 接受的最小分片大小。
    pub min_fragment_size: usize,
    /// 后端默认使用的分片大小。
    pub default_fragment_size: usize,
    /// Provider 或当前实现允许的最大分片大小。
    pub max_fragment_size: usize,
    /// 分片边界对齐要求。
    pub fragment_alignment: usize,
    /// 小文件可绕过 resumable session 的大小上限。
    pub max_simple_upload_size: Option<u64>,
    /// 是否允许浏览器直接拿 provider session 上传。
    pub frontend_direct_upload: bool,
    /// Provider 是否在最后一个 range/fragment 接收后隐式完成 session。
    pub implicit_completion: bool,
    /// 当前实现是否向上层暴露 provider-native abort。
    pub abort_supported: bool,
    /// 当前实现是否向上层暴露 provider-native status/query。
    pub status_query_supported: bool,
}

impl From<crate::ProviderResumableUploadCapabilities>
    for StorageConnectorProviderResumableUploadCapabilities
{
    fn from(value: crate::ProviderResumableUploadCapabilities) -> Self {
        Self {
            browser_upload_protocol: value.browser_upload_protocol,
            provider: value.provider.into(),
            session_label: value.session_label.into(),
            min_fragment_size: value.min_fragment_size,
            default_fragment_size: value.default_fragment_size,
            max_fragment_size: value.max_fragment_size,
            fragment_alignment: value.fragment_alignment,
            max_simple_upload_size: value.max_simple_upload_size,
            frontend_direct_upload: value.frontend_direct_upload,
            implicit_completion: value.implicit_completion,
            abort_supported: value.abort_supported,
            status_query_supported: value.status_query_supported,
        }
    }
}

/// Connector 对底层对象内容的读写能力开关。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorCapabilities {
    /// 是否支持高效 range read。
    pub efficient_range: bool,
    /// 是否支持容量观测。
    pub capacity: bool,
    /// 是否支持底层对象路径列举。
    pub list: bool,
    /// 是否支持 presigned download。
    pub presigned_download: bool,
    /// 是否允许 connector config 提供自定义下载交付根 URL。
    pub custom_download_base_url: bool,
    /// 是否支持 provider/storage-native thumbnail。
    pub storage_native_thumbnail: bool,
    /// 是否支持 provider/storage-native media metadata。
    pub storage_native_media_metadata: bool,
    /// 是否需要或支持 remote node 绑定。
    pub remote_node_binding: bool,
    /// 是否暴露对象存储 upload/download strategy 选项。
    pub object_storage_transfer_strategy: bool,
    /// 底层对象路径采用 opaque UUID，还是保留原文件名作为 provider item name。
    pub object_naming: StorageConnectorObjectNamingMode,
}

/// How uploaded objects are named on the provider side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorObjectNamingMode {
    /// Objects are stored under opaque UUID names.
    OpaqueUuid,
    /// Objects keep the original filename as the provider item name.
    OriginalFilename,
}

/// 编辑已有 policy 时字段值如何处理。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorFieldUpdateBehavior {
    /// The field can be freely changed on edit.
    #[default]
    Mutable,
    /// The field is ignored on edit and keeps its creation-time value.
    CreateOnly,
    /// The field can only be filled once if it was empty at creation.
    SetOnce,
}

impl StorageConnectorFieldUpdateBehavior {
    fn is_mutable(&self) -> bool {
        matches!(self, Self::Mutable)
    }
}

/// 管理端表单里一个字段的完整 contract：payload 名、scope、控件类型、
/// 本地化、默认值与条件行为。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorFieldDescriptor {
    /// 提交 payload 中的字段名。
    pub name: String,
    /// 字段进入哪个配置域。
    pub scope: StorageConnectorFieldScope,
    /// 前端可用的基础控件类型。
    pub kind: StorageConnectorFieldKind,
    /// 前端本地化 label key。默认通常等于 `name`。
    pub label_key: String,
    /// 可选 placeholder，本地化策略由前端决定。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// 可选 help 文案 key。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_key: Option<String>,
    /// 字段必填校验失败时的前端文案 key。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_message_key: Option<String>,
    /// endpoint 协议不合法时的前端文案 key。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalid_protocol_message_key: Option<String>,
    /// endpoint 允许的 URL protocol，取值与浏览器 `URL.protocol` 一致，例如 `https:`。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_endpoint_protocols: Vec<String>,
    /// true 表示 endpoint 可以省略 URL scheme，由 connector 在后端补齐或解释。
    #[serde(default)]
    pub allow_endpoint_without_protocol: bool,
    /// true 表示该字段失焦时前端可以安全 trim。
    #[serde(default)]
    pub trim_on_blur: bool,
    /// 是否必填。复杂条件校验仍由 connector/service 做最终裁决。
    pub required: bool,
    /// 是否是敏感字段，前端应按 secret input 处理，后端不应明文回显。
    pub secret: bool,
    /// 编辑已有策略时字段是否可改。`set_once` 只允许给历史空值补值一次。
    #[serde(
        default,
        skip_serializing_if = "StorageConnectorFieldUpdateBehavior::is_mutable"
    )]
    pub update_behavior: StorageConnectorFieldUpdateBehavior,
    /// Select control contract. Present exactly when `kind` is `select`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub select: Option<StorageConnectorSelectDescriptor>,
    /// Connector schema 定义的默认值。省略表示该字段没有隐式默认值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<StorageConnectorFieldDefaultValue>,
    /// Connector-owned rule deciding whether an empty optional text field also
    /// resolves to `default_value`.
    #[serde(
        default,
        skip_serializing_if = "StorageConnectorFieldDefaultMode::is_missing_only"
    )]
    pub default_mode: StorageConnectorFieldDefaultMode,
    /// Ordered conditional defaults. The first matching rule wins; when none
    /// matches, `default_value` remains the fallback.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_rules: Vec<StorageConnectorFieldDefaultRule>,
    /// Conditions that must all match for the field to be rendered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub visible_when: Vec<StorageConnectorFieldCondition>,
    /// Conditions that make an otherwise optional field required in the UI.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_when: Vec<StorageConnectorFieldCondition>,
    /// What the form/payload normalizer does when `visible_when` is false.
    #[serde(
        default,
        skip_serializing_if = "StorageConnectorInactiveValueBehavior::is_preserve"
    )]
    pub inactive_value_behavior: StorageConnectorInactiveValueBehavior,
    /// Connector localization key for a collapsed advanced field group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advanced_group_key: Option<String>,
    /// 可被前端用于即时反馈、且必须由后端再次执行的基础约束。
    #[serde(default)]
    pub validation: StorageConnectorFieldValidation,
}

impl StorageConnectorFieldDescriptor {
    /// Checks field-level descriptor invariants: non-empty name/label keys,
    /// endpoint protocol rules requiring their message key, default-mode
    /// applicability, and select contract consistency.
    pub fn validate(&self) -> Result<(), StorageConnectorFieldDescriptorError> {
        if self.name.trim().is_empty() {
            return Err(StorageConnectorFieldDescriptorError(
                "field name must not be empty".to_string(),
            ));
        }
        if self.label_key.trim().is_empty() {
            return Err(StorageConnectorFieldDescriptorError(format!(
                "field '{}' label_key must not be empty",
                self.name
            )));
        }
        if (!self.allowed_endpoint_protocols.is_empty() || self.allow_endpoint_without_protocol)
            && self
                .invalid_protocol_message_key
                .as_deref()
                .is_none_or(|key| key.trim().is_empty())
        {
            return Err(StorageConnectorFieldDescriptorError(format!(
                "field '{}' declares endpoint protocol rules without invalid_protocol_message_key",
                self.name
            )));
        }

        if self.default_mode == StorageConnectorFieldDefaultMode::MissingOrEmptyText {
            if self.default_value.is_none() {
                return Err(StorageConnectorFieldDescriptorError(format!(
                    "field '{}' cannot apply an empty-text default without default_value",
                    self.name
                )));
            }
            if self.kind != StorageConnectorFieldKind::Text || self.required {
                return Err(StorageConnectorFieldDescriptorError(format!(
                    "field '{}' may use missing_or_empty_text only for optional text",
                    self.name
                )));
            }
        }
        if self
            .advanced_group_key
            .as_deref()
            .is_some_and(|key| key.trim().is_empty())
        {
            return Err(StorageConnectorFieldDescriptorError(format!(
                "field '{}' advanced_group_key must not be empty",
                self.name
            )));
        }
        for rule in &self.default_rules {
            if rule.conditions.is_empty() {
                return Err(StorageConnectorFieldDescriptorError(format!(
                    "field '{}' default rule must declare at least one condition",
                    self.name
                )));
            }
            if !field_value_matches_kind(self, &rule.value) {
                return Err(StorageConnectorFieldDescriptorError(format!(
                    "field '{}' conditional default value does not match field kind",
                    self.name
                )));
            }
        }

        match (self.kind, &self.select) {
            (StorageConnectorFieldKind::Select, Some(select)) => {
                let has_static_options = !select.options.is_empty();
                let has_data_source = select.data_source.is_some();
                if has_static_options == has_data_source {
                    return Err(StorageConnectorFieldDescriptorError(format!(
                        "select field '{}' must declare exactly one of options or data_source",
                        self.name
                    )));
                }
                if select.depends_on.is_some() && !has_data_source {
                    return Err(StorageConnectorFieldDescriptorError(format!(
                        "select field '{}' may declare depends_on only with data_source",
                        self.name
                    )));
                }
                if select
                    .automatic_default_label_key
                    .as_deref()
                    .is_some_and(|key| key.trim().is_empty())
                {
                    return Err(StorageConnectorFieldDescriptorError(format!(
                        "select field '{}' automatic_default_label_key must not be empty",
                        self.name
                    )));
                }
                if select.automatic_default_label_key.is_some()
                    && self.default_value.is_none()
                    && self.default_rules.is_empty()
                {
                    return Err(StorageConnectorFieldDescriptorError(format!(
                        "select field '{}' automatic default option requires a default value or default rule",
                        self.name
                    )));
                }
                if select.allow_custom_value
                    && (select.value_kind != StorageConnectorSelectValueKind::String
                        || !has_static_options)
                {
                    return Err(StorageConnectorFieldDescriptorError(format!(
                        "select field '{}' custom values require fixed string options",
                        self.name
                    )));
                }
                match (
                    select.allow_custom_value,
                    select.custom_value_label_key.as_deref(),
                ) {
                    (true, Some(key)) if !key.trim().is_empty() => {}
                    (true, _) => {
                        return Err(StorageConnectorFieldDescriptorError(format!(
                            "select field '{}' custom values require custom_value_label_key",
                            self.name
                        )));
                    }
                    (false, Some(_)) => {
                        return Err(StorageConnectorFieldDescriptorError(format!(
                            "select field '{}' custom_value_label_key requires custom values",
                            self.name
                        )));
                    }
                    (false, None) => {}
                }
                match select.data_source {
                    Some(StorageConnectorSelectDataSource::RemoteNodes)
                        if select.value_kind != StorageConnectorSelectValueKind::Integer
                            || select.depends_on.is_some() =>
                    {
                        return Err(StorageConnectorFieldDescriptorError(format!(
                            "select field '{}' remote_nodes source requires integer values and no dependency",
                            self.name
                        )));
                    }
                    Some(StorageConnectorSelectDataSource::RemoteStorageTargets)
                        if select.value_kind != StorageConnectorSelectValueKind::String
                            || select.depends_on.is_none() =>
                    {
                        return Err(StorageConnectorFieldDescriptorError(format!(
                            "select field '{}' remote_storage_targets source requires string values and one dependency",
                            self.name
                        )));
                    }
                    _ => {}
                }

                let mut values = HashSet::with_capacity(select.options.len());
                for option in &select.options {
                    if option.label_key.trim().is_empty() {
                        return Err(StorageConnectorFieldDescriptorError(format!(
                            "select field '{}' option label_key must not be empty",
                            self.name
                        )));
                    }
                    let value_kind_matches = matches!(
                        (select.value_kind, &option.value),
                        (
                            StorageConnectorSelectValueKind::String,
                            StorageConnectorSelectOptionValue::String(_)
                        ) | (
                            StorageConnectorSelectValueKind::Integer,
                            StorageConnectorSelectOptionValue::Integer(_)
                        )
                    );
                    if !value_kind_matches {
                        return Err(StorageConnectorFieldDescriptorError(format!(
                            "select field '{}' option value does not match value_kind",
                            self.name
                        )));
                    }
                    if !values.insert(option.value.clone()) {
                        return Err(StorageConnectorFieldDescriptorError(format!(
                            "select field '{}' declares duplicate option values",
                            self.name
                        )));
                    }
                }
                if let Some(default) = &self.default_value {
                    let default_option = match default {
                        StorageConnectorFieldDefaultValue::String(value) => {
                            StorageConnectorSelectOptionValue::String(value.clone())
                        }
                        StorageConnectorFieldDefaultValue::Integer(value) => {
                            StorageConnectorSelectOptionValue::Integer(*value)
                        }
                        StorageConnectorFieldDefaultValue::Boolean(_) => {
                            return Err(StorageConnectorFieldDescriptorError(format!(
                                "select field '{}' default value does not match value_kind",
                                self.name
                            )));
                        }
                    };
                    let default_kind_matches = matches!(
                        (select.value_kind, &default_option),
                        (
                            StorageConnectorSelectValueKind::String,
                            StorageConnectorSelectOptionValue::String(_)
                        ) | (
                            StorageConnectorSelectValueKind::Integer,
                            StorageConnectorSelectOptionValue::Integer(_)
                        )
                    );
                    if !default_kind_matches
                        || (!select.options.is_empty()
                            && !select
                                .options
                                .iter()
                                .any(|option| option.value == default_option))
                    {
                        return Err(StorageConnectorFieldDescriptorError(format!(
                            "select field '{}' default value is not a declared option",
                            self.name
                        )));
                    }
                }
                for rule in &self.default_rules {
                    let default_option = match &rule.value {
                        StorageConnectorFieldDefaultValue::String(value) => {
                            StorageConnectorSelectOptionValue::String(value.clone())
                        }
                        StorageConnectorFieldDefaultValue::Integer(value) => {
                            StorageConnectorSelectOptionValue::Integer(*value)
                        }
                        StorageConnectorFieldDefaultValue::Boolean(_) => {
                            return Err(StorageConnectorFieldDescriptorError(format!(
                                "select field '{}' conditional default value does not match value_kind",
                                self.name
                            )));
                        }
                    };
                    if !select.options.is_empty()
                        && !select
                            .options
                            .iter()
                            .any(|option| option.value == default_option)
                    {
                        return Err(StorageConnectorFieldDescriptorError(format!(
                            "select field '{}' conditional default is not a declared option",
                            self.name
                        )));
                    }
                }
                for option in &select.options {
                    for condition in &option.available_when {
                        if condition.field.trim().is_empty() {
                            return Err(StorageConnectorFieldDescriptorError(format!(
                                "select field '{}' option condition field must not be empty",
                                self.name
                            )));
                        }
                    }
                }
            }
            (StorageConnectorFieldKind::Select, None) => {
                return Err(StorageConnectorFieldDescriptorError(format!(
                    "select field '{}' is missing its select contract",
                    self.name
                )));
            }
            (_, Some(_)) => {
                return Err(StorageConnectorFieldDescriptorError(format!(
                    "non-select field '{}' must not declare a select contract",
                    self.name
                )));
            }
            (_, None) => {}
        }
        Ok(())
    }
}

fn field_value_matches_kind(
    field: &StorageConnectorFieldDescriptor,
    value: &StorageConnectorFieldDefaultValue,
) -> bool {
    matches!(
        (field.kind, value),
        (
            StorageConnectorFieldKind::Text | StorageConnectorFieldKind::Secret,
            StorageConnectorFieldDefaultValue::String(_)
        ) | (
            StorageConnectorFieldKind::Boolean,
            StorageConnectorFieldDefaultValue::Boolean(_)
        ) | (
            StorageConnectorFieldKind::Number,
            StorageConnectorFieldDefaultValue::Integer(_)
        )
    ) || matches!(
        (
            field.kind,
            field.select.as_ref().map(|select| select.value_kind),
            value,
        ),
        (
            StorageConnectorFieldKind::Select,
            Some(StorageConnectorSelectValueKind::String),
            StorageConnectorFieldDefaultValue::String(_)
        ) | (
            StorageConnectorFieldKind::Select,
            Some(StorageConnectorSelectValueKind::Integer),
            StorageConnectorFieldDefaultValue::Integer(_)
        )
    )
}

/// Returned when one field descriptor violates its own contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConnectorFieldDescriptorError(String);

impl fmt::Display for StorageConnectorFieldDescriptorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for StorageConnectorFieldDescriptorError {}

/// Typed connector configuration that is also the source of truth for the
/// admin form field contract.
///
/// Implementations are normally generated by [`crate::storage_connector_config`]
/// so persisted serde fields and descriptor fields cannot drift independently.
pub trait StorageConnectorConfigSchema {
    /// Descriptor fields written into the versioned connector config
    /// envelope, in declaration order.
    fn connector_config_fields() -> Vec<StorageConnectorFieldDescriptor>;

    /// Credential mode this config type requires.
    fn credential_mode() -> StorageConnectorCredentialMode;

    /// Descriptor fields written into the separate static credential
    /// channel, in declaration order.
    fn credential_fields() -> Vec<StorageConnectorFieldDescriptor>;

    /// Config fields followed by credential fields; the combined form field
    /// list used by admin validation and UI rendering.
    fn descriptor_fields() -> Vec<StorageConnectorFieldDescriptor> {
        let mut fields = Self::connector_config_fields();
        fields.extend(Self::credential_fields());
        fields
    }
}

/// Typed action input that is also the source of truth for its UI field schema.
///
/// Implementations are normally generated by
/// [`crate::storage_connector_action_schema`]. Connector code deserializes the
/// validated action values into this type instead of manually assembling JSON.
pub trait StorageConnectorActionSchema {
    /// UI field schema for this action's input, all with `action_input`
    /// scope.
    fn action_fields() -> Vec<StorageConnectorFieldDescriptor>;
}

/// Failure modes when normalizing connector config or action input against a
/// descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageConnectorOptionsValidationError {
    /// Payload envelope uses a different `format_version` than the platform
    /// currently accepts.
    FormatVersionMismatch {
        /// Envelope format version required by the platform.
        expected: u32,
        /// Format version carried by the rejected payload.
        actual: u32,
    },
    /// Payload namespace does not match the descriptor being applied.
    NamespaceMismatch {
        /// Connector id the descriptor expects.
        expected: String,
        /// Connector id carried by the payload.
        actual: String,
    },
    /// Payload schema version is not the version the descriptor resolves.
    SchemaVersionMismatch {
        /// Schema version declared by the descriptor.
        expected: u32,
        /// Schema version carried by the payload.
        actual: u32,
    },
    /// A declared field descriptor itself failed structural validation.
    InvalidFieldDescriptor {
        /// Name of the offending field.
        field: String,
        /// Human-readable explanation of the descriptor violation.
        reason: String,
    },
    /// The same field name is declared more than once in one scope.
    DuplicateField(String),
    /// Payload carries a field the descriptor does not declare.
    UnknownField(String),
    /// A required field is absent (or null) in the payload.
    MissingRequiredField(String),
    /// A secret/secret field was submitted through the connector config
    /// channel instead of credential storage.
    SecretField(String),
    /// Value shape does not match the field kind.
    TypeMismatch {
        /// Name of the offending field.
        field: String,
        /// Field kind the value must satisfy.
        expected: StorageConnectorFieldKind,
    },
    /// A select value is not among the declared options and the field does
    /// not accept custom values.
    InvalidOption {
        /// Name of the offending field.
        field: String,
        /// Rejected value as submitted.
        value: String,
    },
    /// A number field is below its declared minimum.
    IntegerBelowMinimum {
        /// Name of the offending field.
        field: String,
        /// Inclusive lower bound from the field validation.
        minimum: i64,
    },
    /// A number field is above its declared maximum.
    IntegerAboveMaximum {
        /// Name of the offending field.
        field: String,
        /// Inclusive upper bound from the field validation.
        maximum: i64,
    },
    /// A text field exceeds its declared maximum length.
    StringTooLong {
        /// Name of the offending field.
        field: String,
        /// Maximum character count from the field validation.
        maximum: u32,
    },
}

impl fmt::Display for StorageConnectorOptionsValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FormatVersionMismatch { expected, actual } => write!(
                formatter,
                "connector config format version mismatch: expected {expected}, got {actual}"
            ),
            Self::NamespaceMismatch { expected, actual } => write!(
                formatter,
                "provider options namespace mismatch: expected '{expected}', got '{actual}'"
            ),
            Self::SchemaVersionMismatch { expected, actual } => write!(
                formatter,
                "provider options schema version mismatch: expected {expected}, got {actual}"
            ),
            Self::InvalidFieldDescriptor { field, reason } => write!(
                formatter,
                "provider option field '{field}' has an invalid descriptor: {reason}"
            ),
            Self::DuplicateField(field) => {
                write!(
                    formatter,
                    "provider option field '{field}' is declared more than once"
                )
            }
            Self::UnknownField(field) => {
                write!(formatter, "unknown provider option field '{field}'")
            }
            Self::MissingRequiredField(field) => {
                write!(
                    formatter,
                    "required provider option field '{field}' is missing"
                )
            }
            Self::SecretField(field) => write!(
                formatter,
                "provider option field '{field}' is secret and must use credential storage"
            ),
            Self::TypeMismatch { field, expected } => write!(
                formatter,
                "provider option field '{field}' must be a {}",
                expected.as_str()
            ),
            Self::InvalidOption { field, value } => write!(
                formatter,
                "provider option field '{field}' has unsupported value '{value}'"
            ),
            Self::IntegerBelowMinimum { field, minimum } => write!(
                formatter,
                "provider option field '{field}' must be at least {minimum}"
            ),
            Self::IntegerAboveMaximum { field, maximum } => write!(
                formatter,
                "provider option field '{field}' must be at most {maximum}"
            ),
            Self::StringTooLong { field, maximum } => write!(
                formatter,
                "provider option field '{field}' exceeds maximum length {maximum}"
            ),
        }
    }
}

impl std::error::Error for StorageConnectorOptionsValidationError {}

/// 按 connector descriptor 归一化并校验一个 provider option namespace。
///
/// 调用方必须传入 payload 顶层 namespace，避免 service 先丢掉 namespace 后让错误
/// connector 接收数据。返回值只包含 descriptor 声明的字段及补齐后的默认值。
pub fn normalize_storage_connector_config(
    descriptor: &StorageConnectorDescriptor,
    input: &crate::ConnectorConfigEnvelope,
) -> Result<crate::ConnectorConfigEnvelope, StorageConnectorOptionsValidationError> {
    if input.format_version != crate::CONNECTOR_CONFIG_FORMAT_VERSION {
        return Err(
            StorageConnectorOptionsValidationError::FormatVersionMismatch {
                expected: crate::CONNECTOR_CONFIG_FORMAT_VERSION,
                actual: input.format_version,
            },
        );
    }
    if descriptor.connector_id != input.connector_id {
        return Err(StorageConnectorOptionsValidationError::NamespaceMismatch {
            expected: descriptor.connector_id.as_str().to_string(),
            actual: input.connector_id.as_str().to_string(),
        });
    }
    if descriptor.config_schema_version != input.schema_version {
        return Err(
            StorageConnectorOptionsValidationError::SchemaVersionMismatch {
                expected: descriptor.config_schema_version,
                actual: input.schema_version,
            },
        );
    }

    let normalized = normalize_storage_connector_field_values(
        descriptor
            .fields
            .iter()
            .filter(|field| field.scope == StorageConnectorFieldScope::ConnectorConfig),
        &input.values,
        true,
    )?;

    Ok(crate::ConnectorConfigEnvelope {
        format_version: crate::CONNECTOR_CONFIG_FORMAT_VERSION,
        connector_id: input.connector_id.clone(),
        schema_version: descriptor.config_schema_version,
        values: normalized,
    })
}

/// Normalize one action invocation against the connector-owned action schema.
///
/// Unlike persisted connector config, action input may contain secret fields:
/// they are delivered only to the connector invocation and are never written
/// into the policy envelope.
pub fn normalize_storage_connector_action_input(
    descriptor: &StorageConnectorActionDescriptor,
    input: &BTreeMap<String, serde_json::Value>,
) -> Result<BTreeMap<String, serde_json::Value>, StorageConnectorOptionsValidationError> {
    normalize_storage_connector_field_values(
        descriptor
            .fields
            .iter()
            .filter(|field| field.scope == StorageConnectorFieldScope::ActionInput),
        input,
        false,
    )
}

/// Resolve one custom action endpoint and normalize its connector-owned input.
///
/// This shared boundary knows only connector metadata. Provider-specific
/// action IDs and typed input structs remain inside the connector.
pub fn normalize_storage_connector_custom_action_invocation(
    connector: &StorageConnectorDescriptor,
    action_id: &StorageConnectorActionId,
    endpoint: StorageConnectorActionEndpoint,
    input: &BTreeMap<String, serde_json::Value>,
) -> Result<BTreeMap<String, serde_json::Value>, StorageConnectorActionInvocationError> {
    let action = connector
        .actions
        .iter()
        .find(|action| {
            action.kind == StorageConnectorActionKind::Custom
                && &action.action_id == action_id
                && action.endpoints.contains(&endpoint)
        })
        .ok_or_else(|| StorageConnectorActionInvocationError::Unsupported {
            connector_id: connector.connector_id.clone(),
            action_id: action_id.clone(),
            endpoint,
        })?;
    normalize_storage_connector_action_input(action, input)
        .map_err(StorageConnectorActionInvocationError::InvalidInput)
}

/// Failure to resolve or normalize one connector action invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageConnectorActionInvocationError {
    /// The connector does not declare this custom action for the requested
    /// endpoint.
    Unsupported {
        /// Connector the invocation was addressed to.
        connector_id: ConnectorId,
        /// Action id that could not be resolved.
        action_id: StorageConnectorActionId,
        /// Endpoint the action was not declared on.
        endpoint: StorageConnectorActionEndpoint,
    },
    /// The action resolved but its input failed schema normalization.
    InvalidInput(StorageConnectorOptionsValidationError),
}

impl fmt::Display for StorageConnectorActionInvocationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported {
                connector_id,
                action_id,
                endpoint,
            } => write!(
                formatter,
                "storage connector action '{}' is not available through endpoint {:?} for connector '{}'",
                action_id.as_str(),
                endpoint,
                connector_id.as_str()
            ),
            Self::InvalidInput(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for StorageConnectorActionInvocationError {}

fn normalize_storage_connector_field_values<'a>(
    declared_fields: impl IntoIterator<Item = &'a StorageConnectorFieldDescriptor>,
    input: &BTreeMap<String, serde_json::Value>,
    reject_secrets: bool,
) -> Result<BTreeMap<String, serde_json::Value>, StorageConnectorOptionsValidationError> {
    let mut fields = HashMap::new();
    for field in declared_fields {
        if fields.insert(field.name.as_str(), field).is_some() {
            return Err(StorageConnectorOptionsValidationError::DuplicateField(
                field.name.clone(),
            ));
        }
    }

    for name in input.keys() {
        if !fields.contains_key(name.as_str()) {
            return Err(StorageConnectorOptionsValidationError::UnknownField(
                name.clone(),
            ));
        }
    }

    // Resolve connector-owned defaults to a fixed point. Re-evaluating values
    // that were not explicitly supplied lets one default depend on another
    // without making descriptor field order observable.
    let mut resolved_input = input.clone();
    for _ in 0..=fields.len() {
        let mut changed = false;
        for field in fields.values() {
            if supplied_connector_field_value(input, &field.name).is_some() {
                continue;
            }
            let next = resolved_field_default(field, &resolved_input).map(default_value_to_json);
            match next {
                Some(value) if resolved_input.get(&field.name) != Some(&value) => {
                    resolved_input.insert(field.name.clone(), value);
                    changed = true;
                }
                None if resolved_input.remove(&field.name).is_some() => changed = true,
                _ => {}
            }
        }
        if !changed {
            break;
        }
    }

    let mut normalized = BTreeMap::new();
    for field in fields.values() {
        if reject_secrets && (field.secret || field.kind == StorageConnectorFieldKind::Secret) {
            if supplied_connector_field_value(input, &field.name).is_some() {
                return Err(StorageConnectorOptionsValidationError::SecretField(
                    field.name.clone(),
                ));
            }
            continue;
        }

        let supplied = supplied_connector_field_value(input, &field.name).cloned();
        let value = supplied_connector_field_value(&resolved_input, &field.name).cloned();
        let Some(mut value) = value else {
            if field.required {
                return Err(
                    StorageConnectorOptionsValidationError::MissingRequiredField(
                        field.name.clone(),
                    ),
                );
            }
            continue;
        };

        normalize_and_validate_connector_field_value(field, &mut value)?;
        if value.as_str().is_some_and(str::is_empty) && !field.required {
            if supplied.is_some()
                && field.default_mode == StorageConnectorFieldDefaultMode::MissingOrEmptyText
            {
                let default_value = field.default_value.as_ref().ok_or_else(|| {
                    StorageConnectorOptionsValidationError::InvalidFieldDescriptor {
                        field: field.name.clone(),
                        reason: "missing_or_empty_text requires default_value".to_string(),
                    }
                })?;
                value = default_value_to_json(default_value);
                normalize_and_validate_connector_field_value(field, &mut value)?;
            } else if supplied.is_some() {
                continue;
            }
        }
        normalized.insert(field.name.clone(), value);
    }
    Ok(normalized)
}

fn supplied_connector_field_value<'a>(
    values: &'a BTreeMap<String, serde_json::Value>,
    name: &str,
) -> Option<&'a serde_json::Value> {
    values.get(name).filter(|value| !value.is_null())
}

fn resolved_field_default<'a>(
    field: &'a StorageConnectorFieldDescriptor,
    values: &BTreeMap<String, serde_json::Value>,
) -> Option<&'a StorageConnectorFieldDefaultValue> {
    field
        .default_rules
        .iter()
        .find(|rule| {
            rule.conditions.iter().all(|condition| {
                values
                    .get(&condition.field)
                    .filter(|value| !value.is_null())
                    .is_some_and(|value| {
                        (value == &default_value_to_json(&condition.value)) != condition.negate
                    })
            })
        })
        .map(|rule| &rule.value)
        .or(field.default_value.as_ref())
}

fn default_value_to_json(value: &StorageConnectorFieldDefaultValue) -> serde_json::Value {
    match value {
        StorageConnectorFieldDefaultValue::Boolean(value) => serde_json::Value::Bool(*value),
        StorageConnectorFieldDefaultValue::Integer(value) => {
            serde_json::Value::Number((*value).into())
        }
        StorageConnectorFieldDefaultValue::String(value) => {
            serde_json::Value::String(value.clone())
        }
    }
}

fn normalize_and_validate_connector_field_value(
    field: &StorageConnectorFieldDescriptor,
    value: &mut serde_json::Value,
) -> Result<(), StorageConnectorOptionsValidationError> {
    match field.kind {
        StorageConnectorFieldKind::Text | StorageConnectorFieldKind::Secret => {
            let Some(text) = value.as_str() else {
                return Err(StorageConnectorOptionsValidationError::TypeMismatch {
                    field: field.name.clone(),
                    expected: field.kind,
                });
            };
            let normalized = if field.trim_on_blur {
                text.trim()
            } else {
                text
            };
            if field.required && normalized.is_empty() {
                return Err(
                    StorageConnectorOptionsValidationError::MissingRequiredField(
                        field.name.clone(),
                    ),
                );
            }
            if let Some(maximum) = field.validation.max_length
                && normalized.chars().count() > maximum as usize
            {
                return Err(StorageConnectorOptionsValidationError::StringTooLong {
                    field: field.name.clone(),
                    maximum,
                });
            }
            *value = serde_json::Value::String(normalized.to_string());
        }
        StorageConnectorFieldKind::Select => {
            let Some(select) = field.select.as_ref() else {
                return Err(StorageConnectorOptionsValidationError::TypeMismatch {
                    field: field.name.clone(),
                    expected: field.kind,
                });
            };
            let option_value = match select.value_kind {
                StorageConnectorSelectValueKind::String => {
                    let Some(text) = value.as_str() else {
                        return Err(StorageConnectorOptionsValidationError::TypeMismatch {
                            field: field.name.clone(),
                            expected: field.kind,
                        });
                    };
                    let normalized = if field.trim_on_blur {
                        text.trim()
                    } else {
                        text
                    }
                    .to_string();
                    if field.required && normalized.is_empty() {
                        return Err(
                            StorageConnectorOptionsValidationError::MissingRequiredField(
                                field.name.clone(),
                            ),
                        );
                    }
                    if let Some(maximum) = field.validation.max_length
                        && normalized.chars().count() > maximum as usize
                    {
                        return Err(StorageConnectorOptionsValidationError::StringTooLong {
                            field: field.name.clone(),
                            maximum,
                        });
                    }
                    *value = serde_json::Value::String(normalized.clone());
                    StorageConnectorSelectOptionValue::String(normalized)
                }
                StorageConnectorSelectValueKind::Integer => {
                    let Some(integer) = value.as_i64() else {
                        return Err(StorageConnectorOptionsValidationError::TypeMismatch {
                            field: field.name.clone(),
                            expected: field.kind,
                        });
                    };
                    StorageConnectorSelectOptionValue::Integer(integer)
                }
            };
            if !select.options.is_empty()
                && !select.allow_custom_value
                && !select
                    .options
                    .iter()
                    .any(|option| option.value == option_value)
            {
                return Err(StorageConnectorOptionsValidationError::InvalidOption {
                    field: field.name.clone(),
                    value: match option_value {
                        StorageConnectorSelectOptionValue::String(value) => value,
                        StorageConnectorSelectOptionValue::Integer(value) => value.to_string(),
                    },
                });
            }
        }
        StorageConnectorFieldKind::Boolean => {
            if !value.is_boolean() {
                return Err(StorageConnectorOptionsValidationError::TypeMismatch {
                    field: field.name.clone(),
                    expected: field.kind,
                });
            }
        }
        StorageConnectorFieldKind::Number => {
            let Some(integer) = value.as_i64() else {
                return Err(StorageConnectorOptionsValidationError::TypeMismatch {
                    field: field.name.clone(),
                    expected: field.kind,
                });
            };
            if let Some(minimum) = field.validation.min_integer
                && integer < minimum
            {
                return Err(
                    StorageConnectorOptionsValidationError::IntegerBelowMinimum {
                        field: field.name.clone(),
                        minimum,
                    },
                );
            }
            if let Some(maximum) = field.validation.max_integer
                && integer > maximum
            {
                return Err(
                    StorageConnectorOptionsValidationError::IntegerAboveMaximum {
                        field: field.name.clone(),
                        maximum,
                    },
                );
            }
        }
    }
    Ok(())
}

/// Styling tone applied by the generic credential panel to a status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorCredentialStatusTone {
    /// Ordinary informational status.
    Neutral,
    /// Healthy credential state.
    Success,
    /// Degraded state that still works.
    Warning,
    /// Broken state requiring administrator action.
    Danger,
}

/// One connector-owned rewrite rule for a sanitized credential status reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorCredentialReasonRule {
    /// Case-insensitive fragments matched against the sanitized status reason.
    pub contains_any: Vec<String>,
    /// Connector-owned localization key presented instead of the wire reason.
    pub message_key: String,
}

impl StorageConnectorCredentialReasonRule {
    fn validate(&self) -> Result<(), StorageConnectorDescriptorError> {
        if self.contains_any.is_empty()
            || self
                .contains_any
                .iter()
                .any(|fragment| fragment.trim().is_empty())
        {
            return Err(StorageConnectorDescriptorError(
                "credential status reason rule must declare non-empty fragments".to_string(),
            ));
        }
        if self.message_key.trim().is_empty() {
            return Err(StorageConnectorDescriptorError(
                "credential status reason rule message_key must not be empty".to_string(),
            ));
        }
        Ok(())
    }
}

/// Connector-owned presentation for one credential status wire value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorCredentialStatusPresentation {
    /// Connector-owned localized status label.
    pub label_key: String,
    /// Semantic styling consumed by the generic credential panel.
    pub tone: StorageConnectorCredentialStatusTone,
    /// Optional explanatory copy for this status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_key: Option<String>,
    /// Optional alert heading for states that require administrator attention.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attention_title_key: Option<String>,
    /// Optional guidance shown below the normalized reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attention_guidance_key: Option<String>,
    /// Connector-owned normalization rules for sanitized wire reasons.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reason_rules: Vec<StorageConnectorCredentialReasonRule>,
    /// Localized fallback used when no reason rule matches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_fallback_key: Option<String>,
}

impl StorageConnectorCredentialStatusPresentation {
    fn validate(&self) -> Result<(), StorageConnectorDescriptorError> {
        if self.label_key.trim().is_empty() {
            return Err(StorageConnectorDescriptorError(
                "credential status presentation label_key must not be empty".to_string(),
            ));
        }
        for (name, message_id) in [
            ("description_key", self.description_key.as_deref()),
            ("attention_title_key", self.attention_title_key.as_deref()),
            (
                "attention_guidance_key",
                self.attention_guidance_key.as_deref(),
            ),
            ("reason_fallback_key", self.reason_fallback_key.as_deref()),
        ] {
            if message_id.is_some_and(|message_id| message_id.trim().is_empty()) {
                return Err(StorageConnectorDescriptorError(format!(
                    "credential status presentation {name} must not be empty when present"
                )));
            }
        }
        for rule in &self.reason_rules {
            rule.validate()?;
        }
        Ok(())
    }

    fn localization_message_ids<'a>(&'a self, message_ids: &mut BTreeSet<&'a str>) {
        message_ids.insert(self.label_key.as_str());
        message_ids.extend(
            [
                self.description_key.as_deref(),
                self.attention_title_key.as_deref(),
                self.attention_guidance_key.as_deref(),
                self.reason_fallback_key.as_deref(),
            ]
            .into_iter()
            .flatten(),
        );
        message_ids.extend(
            self.reason_rules
                .iter()
                .map(|rule| rule.message_key.as_str()),
        );
    }
}

/// Connector-owned copy slots for the generic credential management panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorCredentialManagementDescriptor {
    /// Credential panel heading.
    pub title_key: String,
    /// Status text shown while the credential snapshot is loading.
    pub loading_key: String,
    /// Credential status wire value to connector-owned presentation metadata.
    ///
    /// The map keeps the UI independent from provider-specific status copy and
    /// lets future connector credential contracts add status values without a
    /// frontend provider matrix.
    pub status_presentations: BTreeMap<String, StorageConnectorCredentialStatusPresentation>,
    /// Label used when an existing credential starts authorization again.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reauthorize_action_key: Option<String>,
    /// Localized lifecycle label for the original authorization time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorized_at_key: Option<String>,
    /// Localized lifecycle label for the latest token refresh time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refreshed_at_key: Option<String>,
    /// Localized lifecycle label for the latest validation time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validated_at_key: Option<String>,
    /// Label for an authorization redirect URI exposed by the platform.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri_key: Option<String>,
    /// Redirect URI registration guidance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri_help_key: Option<String>,
    /// Accessible label for the redirect URI copy action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri_copy_key: Option<String>,
    /// Message shown when authorization is requested with unsaved policy data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save_before_authorize_key: Option<String>,
    /// Message shown after the authorization window is opened.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_started_key: Option<String>,
    /// Message shown when credential validation is requested with unsaved data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save_before_validate_key: Option<String>,
    /// Message shown after a credential validates successfully.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_success_key: Option<String>,
    /// Optional success detail rendered with connector-provided parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_success_detail_key: Option<String>,
    /// Message shown after creating a policy that still needs authorization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_authorize_next_key: Option<String>,
}

impl StorageConnectorCredentialManagementDescriptor {
    fn validate(&self) -> Result<(), StorageConnectorDescriptorError> {
        for (name, message_id) in [
            ("title_key", self.title_key.as_str()),
            ("loading_key", self.loading_key.as_str()),
        ] {
            if message_id.trim().is_empty() {
                return Err(StorageConnectorDescriptorError(format!(
                    "credential_management {name} must not be empty"
                )));
            }
        }
        if self.status_presentations.is_empty() {
            return Err(StorageConnectorDescriptorError(
                "credential_management status_presentations must not be empty".to_string(),
            ));
        }
        if !self.status_presentations.contains_key("missing") {
            return Err(StorageConnectorDescriptorError(
                "credential_management status_presentations must declare 'missing'".to_string(),
            ));
        }
        for (status, presentation) in &self.status_presentations {
            if status.trim().is_empty() {
                return Err(StorageConnectorDescriptorError(
                    "credential_management status keys must not be empty".to_string(),
                ));
            }
            presentation.validate()?;
        }
        for (name, message_id) in [
            (
                "reauthorize_action_key",
                self.reauthorize_action_key.as_deref(),
            ),
            ("authorized_at_key", self.authorized_at_key.as_deref()),
            ("refreshed_at_key", self.refreshed_at_key.as_deref()),
            ("validated_at_key", self.validated_at_key.as_deref()),
            ("redirect_uri_key", self.redirect_uri_key.as_deref()),
            (
                "redirect_uri_help_key",
                self.redirect_uri_help_key.as_deref(),
            ),
            (
                "redirect_uri_copy_key",
                self.redirect_uri_copy_key.as_deref(),
            ),
            (
                "save_before_authorize_key",
                self.save_before_authorize_key.as_deref(),
            ),
            (
                "authorization_started_key",
                self.authorization_started_key.as_deref(),
            ),
            (
                "save_before_validate_key",
                self.save_before_validate_key.as_deref(),
            ),
            (
                "validation_success_key",
                self.validation_success_key.as_deref(),
            ),
            (
                "validation_success_detail_key",
                self.validation_success_detail_key.as_deref(),
            ),
            (
                "created_authorize_next_key",
                self.created_authorize_next_key.as_deref(),
            ),
        ] {
            if message_id.is_some_and(|message_id| message_id.trim().is_empty()) {
                return Err(StorageConnectorDescriptorError(format!(
                    "credential_management {name} must not be empty when present"
                )));
            }
        }
        if (self.redirect_uri_help_key.is_some() || self.redirect_uri_copy_key.is_some())
            && self.redirect_uri_key.is_none()
        {
            return Err(StorageConnectorDescriptorError(
                "credential_management redirect URI help/copy metadata requires redirect_uri_key"
                    .to_string(),
            ));
        }
        Ok(())
    }

    fn localization_message_ids<'a>(&'a self, message_ids: &mut BTreeSet<&'a str>) {
        message_ids.insert(self.title_key.as_str());
        message_ids.insert(self.loading_key.as_str());
        for presentation in self.status_presentations.values() {
            presentation.localization_message_ids(message_ids);
        }
        message_ids.extend(
            [
                self.reauthorize_action_key.as_deref(),
                self.authorized_at_key.as_deref(),
                self.refreshed_at_key.as_deref(),
                self.validated_at_key.as_deref(),
                self.redirect_uri_key.as_deref(),
                self.redirect_uri_help_key.as_deref(),
                self.redirect_uri_copy_key.as_deref(),
                self.save_before_authorize_key.as_deref(),
                self.authorization_started_key.as_deref(),
                self.save_before_validate_key.as_deref(),
                self.validation_success_key.as_deref(),
                self.validation_success_detail_key.as_deref(),
                self.created_authorize_next_key.as_deref(),
            ]
            .into_iter()
            .flatten(),
        );
    }
}

/// Connector 对管理端与后端服务声明的完整能力清单：凭据模式、部署范围、
/// 能力开关、表单字段和可执行动作。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorDescriptor {
    /// 持久化到 policy 的稳定 connector/plugin id。
    pub connector_id: ConnectorId,
    /// 人类可读名称。
    pub label: String,
    /// 人类可读说明。
    pub description: String,
    /// 管理端展示元数据。
    ///
    /// 这类 label/icon/helper 虽然最终由前端渲染，但语义上属于 connector：
    /// 新 connector 不应该要求前端再维护一份 driver 展示矩阵。
    pub ui: StorageConnectorUiDescriptor,
    /// connector 的主要凭据模式。
    pub credential_mode: StorageConnectorCredentialMode,
    /// policy 数据相对于多个 Primary 的可见范围。
    pub deployment_scope: StorageConnectorDeploymentScope,
    /// 是否能在首次系统初始化中直接创建一个可用的默认 policy。
    ///
    /// 需要先保存 policy、再跳转授权或完成其他后置配置的 connector 应设为 false。
    pub supports_initial_setup: bool,
    /// 是否需要额外授权才能成为可用 policy。
    pub requires_authorization: bool,
    /// 授权 provider，例如 `microsoft_graph`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_provider: Option<String>,
    /// Connector-owned credential management presentation and lifecycle copy.
    ///
    /// Generic admin code consumes these semantic slots without knowing the
    /// connector id or provider-specific localization keys.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_management: Option<StorageConnectorCredentialManagementDescriptor>,
    /// 存储对象能力。
    pub capabilities: StorageConnectorCapabilities,
    /// 上传工作流能力。
    pub upload_workflows: StorageConnectorUploadWorkflows,
    /// 管理端配置字段声明。
    pub fields: Vec<StorageConnectorFieldDescriptor>,
    /// 当前 connector 能解析并输出的配置 schema 版本。
    pub config_schema_version: u32,
    /// 凭据 payload 的独立 schema 版本。配置字段演进不应使已保存凭据失效。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_schema_version: Option<u32>,
    /// 管理端/服务端可执行动作声明。
    pub actions: Vec<StorageConnectorActionDescriptor>,
    /// Target-owned in-place promotion contracts from compatible connectors.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub promotions: Vec<StorageConnectorPromotionDescriptor>,
    /// 用于开发追踪的相关 issue 编号，不参与业务逻辑。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_issues: Vec<u16>,
}

impl StorageConnectorDescriptor {
    /// Validate the complete plugin-owned catalog contract before registration.
    pub fn validate(&self) -> Result<(), StorageConnectorDescriptorError> {
        self.connector_id
            .validate()
            .map_err(|error| StorageConnectorDescriptorError(error.to_string()))?;
        if self.config_schema_version == 0 {
            return Err(StorageConnectorDescriptorError(
                "config_schema_version must be greater than zero".to_string(),
            ));
        }
        match (self.credential_mode, self.credential_schema_version) {
            (StorageConnectorCredentialMode::None, None) => {}
            (StorageConnectorCredentialMode::None, Some(_)) => {
                return Err(StorageConnectorDescriptorError(
                    "credential_schema_version must be omitted when credentials are disabled"
                        .to_string(),
                ));
            }
            (_, Some(version)) if version > 0 => {}
            _ => {
                return Err(StorageConnectorDescriptorError(
                    "credential_schema_version must be a positive integer when credentials are enabled"
                        .to_string(),
                ));
            }
        }

        let mut field_names = HashSet::with_capacity(self.fields.len());
        for field in &self.fields {
            field.validate().map_err(|error| {
                StorageConnectorDescriptorError(format!(
                    "field '{}' is invalid: {error}",
                    field.name
                ))
            })?;
            if !field_names.insert((field.scope, field.name.as_str())) {
                return Err(StorageConnectorDescriptorError(format!(
                    "field '{}' is declared more than once in scope {:?}",
                    field.name, field.scope
                )));
            }
        }
        let download_base_url_field = self.fields.iter().find(|field| {
            field.scope == StorageConnectorFieldScope::ConnectorConfig
                && field.name == "download_base_url"
        });
        match (
            self.capabilities.custom_download_base_url,
            download_base_url_field,
        ) {
            (true, Some(field))
                if field.kind == StorageConnectorFieldKind::Text
                    && !field.required
                    && !field.secret => {}
            (true, Some(_)) => {
                return Err(StorageConnectorDescriptorError(
                    "custom download base URL capability requires an optional non-secret text connector-config field named 'download_base_url'"
                        .to_string(),
                ));
            }
            (true, None) => {
                return Err(StorageConnectorDescriptorError(
                    "custom download base URL capability requires connector-config field 'download_base_url'"
                        .to_string(),
                ));
            }
            (false, Some(_)) => {
                return Err(StorageConnectorDescriptorError(
                    "connector-config field 'download_base_url' requires custom download base URL capability"
                        .to_string(),
                ));
            }
            (false, None) => {}
        }
        for field in &self.fields {
            let mut visiting = HashSet::new();
            if has_conditional_field_cycle(&self.fields, field, &mut visiting) {
                return Err(StorageConnectorDescriptorError(format!(
                    "field '{}' participates in a conditional field cycle",
                    field.name
                )));
            }
        }
        for field in &self.fields {
            let conditions = field
                .visible_when
                .iter()
                .chain(field.required_when.iter())
                .chain(
                    field
                        .default_rules
                        .iter()
                        .flat_map(|rule| rule.conditions.iter()),
                )
                .chain(
                    field
                        .select
                        .iter()
                        .flat_map(|select| select.options.iter())
                        .flat_map(|option| option.available_when.iter()),
                );
            for condition in conditions {
                let dependency_scope = condition.scope.unwrap_or(field.scope);
                if dependency_scope != field.scope
                    && !(field.scope == StorageConnectorFieldScope::StaticCredential
                        && dependency_scope == StorageConnectorFieldScope::ConnectorConfig
                        && !field
                            .default_rules
                            .iter()
                            .any(|rule| rule.conditions.contains(condition))
                        && !field
                            .select
                            .iter()
                            .flat_map(|select| &select.options)
                            .any(|option| option.available_when.contains(condition)))
                {
                    return Err(StorageConnectorDescriptorError(format!(
                        "field '{}' declares an unsupported cross-scope condition",
                        field.name
                    )));
                }
                let Some(dependency) = self.fields.iter().find(|candidate| {
                    candidate.scope == dependency_scope && candidate.name == condition.field
                }) else {
                    return Err(StorageConnectorDescriptorError(format!(
                        "field '{}' condition references undeclared field '{}' in scope {:?}",
                        field.name, condition.field, field.scope
                    )));
                };
                if dependency.secret || dependency.kind == StorageConnectorFieldKind::Secret {
                    return Err(StorageConnectorDescriptorError(format!(
                        "field '{}' condition must not depend on a secret",
                        field.name
                    )));
                }
                if dependency.scope == field.scope && dependency.name == field.name {
                    return Err(StorageConnectorDescriptorError(format!(
                        "field '{}' condition must not reference itself",
                        field.name
                    )));
                }
                if !field_value_matches_kind(dependency, &condition.value) {
                    return Err(StorageConnectorDescriptorError(format!(
                        "field '{}' condition value does not match referenced field '{}' kind",
                        field.name, condition.field
                    )));
                }
            }
        }
        for field in &self.fields {
            if let Some(dependency) = field
                .select
                .as_ref()
                .and_then(|select| select.depends_on.as_deref())
            {
                if dependency == field.name {
                    return Err(StorageConnectorDescriptorError(format!(
                        "field '{}' must not depend on itself",
                        field.name
                    )));
                }
                if !field_names.contains(&(field.scope, dependency)) {
                    return Err(StorageConnectorDescriptorError(format!(
                        "field '{}' depends on undeclared field '{}' in scope {:?}",
                        field.name, dependency, field.scope
                    )));
                }
            }
        }
        for field in &self.fields {
            let mut visited = HashSet::new();
            let mut current = field;
            while let Some(dependency) = current
                .select
                .as_ref()
                .and_then(|select| select.depends_on.as_deref())
            {
                if !visited.insert((current.scope, current.name.as_str())) {
                    return Err(StorageConnectorDescriptorError(format!(
                        "field '{}' participates in a dependency cycle",
                        field.name
                    )));
                }
                let Some(next) = self.fields.iter().find(|candidate| {
                    candidate.scope == current.scope && candidate.name == dependency
                }) else {
                    break;
                };
                current = next;
            }
        }

        let mut action_ids = HashSet::with_capacity(self.actions.len());
        for action in &self.actions {
            action.validate().map_err(|error| {
                StorageConnectorDescriptorError(format!(
                    "action '{}' is invalid: {error}",
                    action.action_id.as_str()
                ))
            })?;
            if !action_ids.insert(action.action_id.clone()) {
                return Err(StorageConnectorDescriptorError(format!(
                    "action '{}' is declared more than once",
                    action.action_id.as_str()
                )));
            }
        }
        let mut promotion_ids = HashSet::with_capacity(self.promotions.len());
        for promotion in &self.promotions {
            promotion.validate()?;
            if promotion.source_connector_id == self.connector_id {
                return Err(StorageConnectorDescriptorError(format!(
                    "promotion '{}' source connector must differ from target connector",
                    promotion.promotion_id.as_str()
                )));
            }
            if !promotion_ids.insert(promotion.promotion_id.clone()) {
                return Err(StorageConnectorDescriptorError(format!(
                    "promotion '{}' is declared more than once",
                    promotion.promotion_id.as_str()
                )));
            }
            for mapping in &promotion.config_mappings {
                if !self.fields.iter().any(|field| {
                    field.scope == StorageConnectorFieldScope::ConnectorConfig
                        && field.name == mapping.target_field
                }) {
                    return Err(StorageConnectorDescriptorError(format!(
                        "promotion '{}' maps undeclared target config field '{}'",
                        promotion.promotion_id.as_str(),
                        mapping.target_field
                    )));
                }
            }
            for mapping in &promotion.credential_mappings {
                if !self.fields.iter().any(|field| {
                    field.scope == StorageConnectorFieldScope::StaticCredential
                        && field.name == mapping.target_field
                }) {
                    return Err(StorageConnectorDescriptorError(format!(
                        "promotion '{}' maps undeclared target credential field '{}'",
                        promotion.promotion_id.as_str(),
                        mapping.target_field
                    )));
                }
            }
        }
        if let Some(credential_management) = &self.credential_management {
            credential_management.validate()?;
        }
        let has_credential_lifecycle_action = self.actions.iter().any(|action| {
            matches!(
                action.kind,
                StorageConnectorActionKind::Authorization
                    | StorageConnectorActionKind::CredentialValidation
            )
        });
        if (has_credential_lifecycle_action
            || self.credential_mode == StorageConnectorCredentialMode::OauthDelegated)
            && self.credential_management.is_none()
        {
            return Err(StorageConnectorDescriptorError(
                "authorization and credential validation actions require credential_management"
                    .to_string(),
            ));
        }
        Ok(())
    }

    /// Message ids that must be supplied by this connector's localization
    /// provider. Generic product fallbacks used only when an optional message
    /// is absent are deliberately not included.
    pub fn localization_message_ids(&self) -> BTreeSet<&str> {
        let mut message_ids = BTreeSet::from([
            self.ui.label_key.as_str(),
            self.ui.description_key.as_str(),
            self.ui.helper_key.as_str(),
            self.ui.config_step_title_key.as_str(),
            self.ui.config_step_description_key.as_str(),
            self.ui.edit_context_key.as_str(),
        ]);
        for field in &self.fields {
            collect_field_localization_message_ids(field, &mut message_ids);
        }
        for action in &self.actions {
            message_ids.insert(action.label_key.as_str());
            message_ids.insert(action.description_key.as_str());
            for field in &action.fields {
                collect_field_localization_message_ids(field, &mut message_ids);
            }
            for field in &action.output_fields {
                message_ids.insert(field.label_key.as_str());
                message_ids.extend(
                    [field.true_key.as_deref(), field.false_key.as_deref()]
                        .into_iter()
                        .flatten(),
                );
            }
        }
        for promotion in &self.promotions {
            message_ids.insert(promotion.description_key.as_str());
            message_ids.insert(promotion.confirmation_key.as_str());
        }
        if let Some(credential_management) = &self.credential_management {
            credential_management.localization_message_ids(&mut message_ids);
        }
        message_ids
    }
}

fn has_conditional_field_cycle<'a>(
    fields: &'a [StorageConnectorFieldDescriptor],
    field: &'a StorageConnectorFieldDescriptor,
    visiting: &mut HashSet<(StorageConnectorFieldScope, &'a str)>,
) -> bool {
    let key = (field.scope, field.name.as_str());
    if !visiting.insert(key) {
        return true;
    }
    let conditions = field
        .visible_when
        .iter()
        .chain(field.required_when.iter())
        .chain(
            field
                .default_rules
                .iter()
                .flat_map(|rule| rule.conditions.iter()),
        )
        .chain(
            field
                .select
                .iter()
                .flat_map(|select| select.options.iter())
                .flat_map(|option| option.available_when.iter()),
        );
    let has_cycle = conditions
        .filter_map(|condition| {
            fields.iter().find(|candidate| {
                candidate.scope == condition.scope.unwrap_or(field.scope)
                    && candidate.name == condition.field
            })
        })
        .any(|dependency| has_conditional_field_cycle(fields, dependency, visiting));
    visiting.remove(&key);
    has_cycle
}

fn collect_field_localization_message_ids<'a>(
    field: &'a StorageConnectorFieldDescriptor,
    message_ids: &mut BTreeSet<&'a str>,
) {
    message_ids.insert(field.label_key.as_str());
    if let Some(message_id) = field.help_key.as_deref() {
        message_ids.insert(message_id);
    }
    if let Some(message_id) = field.required_message_key.as_deref() {
        message_ids.insert(message_id);
    }
    if let Some(message_id) = field.invalid_protocol_message_key.as_deref() {
        message_ids.insert(message_id);
    }
    if let Some(message_id) = field.advanced_group_key.as_deref() {
        message_ids.insert(message_id);
    }
    if let Some(select) = &field.select {
        message_ids.extend(
            [
                select.automatic_default_label_key.as_deref(),
                select.custom_value_label_key.as_deref(),
            ]
            .into_iter()
            .flatten(),
        );
    }
    for option in field
        .select
        .as_ref()
        .into_iter()
        .flat_map(|select| &select.options)
    {
        message_ids.insert(option.label_key.as_str());
        if let Some(message_id) = option.description_key.as_deref() {
            message_ids.insert(message_id);
        }
    }
}

/// Returned when a connector descriptor fails registration-time validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConnectorDescriptorError(String);

impl fmt::Display for StorageConnectorDescriptorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for StorageConnectorDescriptorError {}

/// 管理端 connector 展示与文案槽位。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorUiDescriptor {
    /// 前端 i18n label key。
    pub label_key: String,
    /// 前端 i18n description key。
    pub description_key: String,
    /// Connector-owned browser icon metadata. URL is filled by the serving
    /// application after resolving the connector asset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<StorageConnectorIconDescriptor>,
    /// Connector-owned badge accent color.
    ///
    /// Keeping the color as structured RGB data lets external connectors pick
    /// their own presentation without extending a core-owned color enum or
    /// sending executable CSS through the descriptor API.
    pub badge_rgb: StorageConnectorBadgeRgb,
    /// 创建向导右侧 helper 文案 key。
    pub helper_key: String,
    /// 创建向导配置步骤标题 key。
    pub config_step_title_key: String,
    /// 创建向导配置步骤说明 key。
    pub config_step_description_key: String,
    /// 编辑页上下文说明 key。
    pub edit_context_key: String,
    /// base_path 为空时展示的 fallback 文案。
    pub base_path_empty_display: String,
    /// base_path input placeholder。
    pub base_path_placeholder: String,
}

/// Connector 图标资产的定位与缓存信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorIconDescriptor {
    /// Serving path of the icon asset; the application resolves it to a full
    /// URL.
    pub url: String,
    /// MIME type of the icon asset, for example `image/svg+xml`.
    pub content_type: String,
    /// Cache-busting revision string changed whenever the icon changes.
    pub revision: String,
}

/// 徽标 accent 颜色的结构化 RGB 通道。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorBadgeRgb {
    /// Red channel, 0-255.
    pub red: u8,
    /// Green channel, 0-255.
    pub green: u8,
    /// Blue channel, 0-255.
    pub blue: u8,
}

impl StorageConnectorBadgeRgb {
    /// Builds a badge color from the three channel values.
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }
}

/// Constructor input for [`object_storage_connector_descriptor`]; all fields
/// map one-to-one onto the resulting [`StorageConnectorDescriptor`].
pub struct ObjectStorageConnectorDescriptorInput {
    /// Stable persisted connector id.
    pub connector_id: ConnectorId,
    /// Human-readable connector name.
    pub label: &'static str,
    /// Human-readable connector description.
    pub description: &'static str,
    /// Admin UI presentation metadata.
    pub ui: StorageConnectorUiDescriptorInput,
    /// Visibility of this connector's policy data across primary instances.
    pub deployment_scope: StorageConnectorDeploymentScope,
    /// Whether a usable default policy can be created during first-time
    /// system setup.
    pub supports_initial_setup: bool,
    /// Primary credential mode of the connector.
    pub credential_mode: StorageConnectorCredentialMode,
    /// Connector-config and static credential field declarations.
    pub fields: Vec<StorageConnectorFieldDescriptor>,
    /// Whether browser presigned part uploads must read the ETag from the
    /// response, as required by S3-compatible multipart.
    pub presigned_part_etag_required: bool,
    /// Whether the provider offers storage-native thumbnail and media
    /// metadata extraction.
    pub storage_native_processing: bool,
    /// Version of the connector config schema this descriptor resolves.
    pub config_schema_version: u32,
    /// Independent schema version of the credential payload.
    pub credential_schema_version: Option<u32>,
    /// Development-tracking issue numbers, not used by business logic.
    pub related_issues: Vec<u16>,
}

/// Builds the descriptor for an S3-compatible object storage family
/// connector, presetting the shared capability and upload-workflow profile.
///
/// The constructor fixes `requires_authorization` to `false`, enables
/// efficient range reads, listing, presigned download, and multipart upload,
/// and selects [`StorageConnectorObjectNamingMode::OpaqueUuid`]; caller inputs
/// only customize per-provider flags such as native media processing or
/// presigned part ETag requirements.
pub fn object_storage_connector_descriptor(
    input: ObjectStorageConnectorDescriptorInput,
) -> StorageConnectorDescriptor {
    StorageConnectorDescriptor {
        connector_id: input.connector_id,
        label: input.label.to_string(),
        description: input.description.to_string(),
        ui: storage_connector_ui_descriptor(input.ui),
        credential_mode: input.credential_mode,
        deployment_scope: input.deployment_scope,
        supports_initial_setup: input.supports_initial_setup,
        requires_authorization: false,
        authorization_provider: None,
        credential_management: None,
        capabilities: StorageConnectorCapabilities {
            efficient_range: true,
            // S3-compatible, OSS, COS, and Azure Blob data-plane APIs do not
            // expose one portable remaining-capacity contract. Policy capacity
            // probes therefore return `unsupported` for this shared family.
            capacity: false,
            list: true,
            presigned_download: true,
            custom_download_base_url: false,
            storage_native_thumbnail: input.storage_native_processing,
            storage_native_media_metadata: input.storage_native_processing,
            remote_node_binding: false,
            object_storage_transfer_strategy: true,
            object_naming: StorageConnectorObjectNamingMode::OpaqueUuid,
        },
        upload_workflows: StorageConnectorUploadWorkflows {
            simple_upload: true,
            simple_upload_capabilities: server_relay_simple_upload_capabilities(None),
            stream_upload: true,
            object_multipart_upload: true,
            object_multipart_upload_capabilities: Some(object_multipart_upload_capabilities(
                ObjectMultipartUploadCapabilitiesInput {
                    presigned_part_etag_required: input.presigned_part_etag_required,
                },
            )),
            provider_resumable_upload: false,
            presigned_upload: true,
            frontend_direct_provider_resumable_upload: false,
            provider_resumable_upload_capabilities: None,
        },
        fields: input.fields,
        config_schema_version: input.config_schema_version,
        credential_schema_version: input.credential_schema_version,
        actions: vec![
            draft_connection_test_action_descriptor(),
            saved_connection_test_action_descriptor(false),
        ],
        promotions: Vec::new(),
        related_issues: input.related_issues,
    }
}

/// Builds simple-upload capabilities for the server-relay profile: the
/// browser sends the object to AsterDrive, which relays it to the provider,
/// with the per-request limit owned by policy `chunk_size`.
pub fn server_relay_simple_upload_capabilities(
    max_provider_single_request_size: Option<u64>,
) -> StorageConnectorSimpleUploadCapabilities {
    StorageConnectorSimpleUploadCapabilities {
        server_side_relay: true,
        policy_limited: true,
        max_provider_single_request_size,
    }
}

/// Caller-tunable knobs for the shared object multipart upload capability
/// profile.
pub struct ObjectMultipartUploadCapabilitiesInput {
    /// Whether browser presigned part uploads must read the response ETag.
    pub presigned_part_etag_required: bool,
}

/// Builds the shared multipart/block upload capability profile: relay and
/// presigned part uploads enabled, explicit complete required, abort and
/// part listing supported, with [`OBJECT_MULTIPART_MIN_PART_SIZE`] as the
/// floor.
pub fn object_multipart_upload_capabilities(
    input: ObjectMultipartUploadCapabilitiesInput,
) -> StorageConnectorObjectMultipartUploadCapabilities {
    StorageConnectorObjectMultipartUploadCapabilities {
        min_part_size: OBJECT_MULTIPART_MIN_PART_SIZE,
        policy_limited_part_size: true,
        relay_part_upload: true,
        presigned_part_upload: true,
        presigned_part_etag_required: input.presigned_part_etag_required,
        explicit_complete_required: true,
        abort_supported: true,
        list_parts_supported: true,
    }
}

/// Constructor input for [`custom_action_descriptor`]; endpoint flags are
/// mapped onto `endpoints` and `requires_saved_policy` by the constructor.
pub struct StorageConnectorCustomActionDescriptorInput {
    /// Connector-unique, stable action id.
    pub action_id: StorageConnectorActionId,
    /// Frontend localization label key.
    pub label_key: &'static str,
    /// Frontend localization description key.
    pub description_key: &'static str,
    /// Action input schema; values are never persisted into the policy.
    pub fields: Vec<StorageConnectorFieldDescriptor>,
    /// Presentation schema for successful structured output.
    pub output_fields: Vec<StorageConnectorActionOutputFieldDescriptor>,
    /// Whether the action can run against unsaved draft policy parameters.
    pub supports_draft: bool,
    /// Whether the action can run against an already saved policy.
    pub supports_saved: bool,
    /// Whether a usable authorization credential must exist before running.
    pub requires_authorization: bool,
    /// Whether the action modifies provider remote state.
    pub mutates_remote_state: bool,
    /// Whether the UI should show an explicit confirmation step.
    pub requires_confirmation: bool,
}

/// Builds a `custom`-kind action descriptor, mapping `supports_draft` and
/// `supports_saved` onto the matching execution endpoints and deriving
/// `requires_saved_policy` from the absence of a draft endpoint.
pub fn custom_action_descriptor(
    input: StorageConnectorCustomActionDescriptorInput,
) -> StorageConnectorActionDescriptor {
    let mut endpoints = Vec::new();
    if input.supports_draft {
        endpoints.push(StorageConnectorActionEndpoint::ExecuteDraftStoragePolicyAction);
    }
    if input.supports_saved {
        endpoints.push(StorageConnectorActionEndpoint::ExecuteSavedStoragePolicyAction);
    }
    StorageConnectorActionDescriptor {
        action_id: input.action_id,
        label_key: input.label_key.to_string(),
        description_key: input.description_key.to_string(),
        kind: StorageConnectorActionKind::Custom,
        endpoints,
        fields: input.fields,
        output_fields: input.output_fields,
        requires_saved_policy: !input.supports_draft,
        requires_authorization: input.requires_authorization,
        mutates_remote_state: input.mutates_remote_state,
        requires_confirmation: input.requires_confirmation,
    }
}

/// Builds the shared `start_authorization` action that launches the delegated
/// authorization flow for a saved policy.
pub fn start_authorization_action_descriptor() -> StorageConnectorActionDescriptor {
    StorageConnectorActionDescriptor {
        action_id: StorageConnectorActionId::declared("start_authorization"),
        label_key: "policy_connector_start_authorization".to_string(),
        description_key: "policy_connector_start_authorization_desc".to_string(),
        kind: StorageConnectorActionKind::Authorization,
        endpoints: vec![StorageConnectorActionEndpoint::StartStorageAuthorization],
        fields: Vec::new(),
        output_fields: Vec::new(),
        requires_saved_policy: true,
        requires_authorization: false,
        mutates_remote_state: false,
        requires_confirmation: false,
    }
}

/// Builds the shared `validate_credential` action that re-validates the
/// stored credential of a saved policy.
pub fn validate_credential_action_descriptor() -> StorageConnectorActionDescriptor {
    StorageConnectorActionDescriptor {
        action_id: StorageConnectorActionId::declared("validate_credential"),
        label_key: "policy_connector_validate_credential".to_string(),
        description_key: "policy_connector_validate_credential_desc".to_string(),
        kind: StorageConnectorActionKind::CredentialValidation,
        endpoints: vec![StorageConnectorActionEndpoint::ValidateStoragePolicyCredential],
        fields: Vec::new(),
        output_fields: Vec::new(),
        requires_saved_policy: true,
        requires_authorization: true,
        mutates_remote_state: false,
        requires_confirmation: false,
    }
}

/// Builds the shared `test_draft_connection` action that probes connectivity
/// using unsaved draft policy parameters.
pub fn draft_connection_test_action_descriptor() -> StorageConnectorActionDescriptor {
    StorageConnectorActionDescriptor {
        action_id: StorageConnectorActionId::declared("test_draft_connection"),
        label_key: "test_connection".to_string(),
        description_key: "policy_wizard_step_connection_desc".to_string(),
        kind: StorageConnectorActionKind::ConnectionTest,
        endpoints: vec![StorageConnectorActionEndpoint::TestPolicyParams],
        fields: Vec::new(),
        output_fields: Vec::new(),
        requires_saved_policy: false,
        requires_authorization: false,
        mutates_remote_state: false,
        requires_confirmation: false,
    }
}

/// Builds the shared `test_saved_connection` action that probes connectivity
/// of an already saved policy, optionally requiring an authorized credential.
pub fn saved_connection_test_action_descriptor(
    requires_authorization: bool,
) -> StorageConnectorActionDescriptor {
    StorageConnectorActionDescriptor {
        action_id: StorageConnectorActionId::declared("test_saved_connection"),
        label_key: "test_connection".to_string(),
        description_key: "policy_wizard_step_connection_desc".to_string(),
        kind: StorageConnectorActionKind::ConnectionTest,
        endpoints: vec![StorageConnectorActionEndpoint::TestPolicyConnection],
        fields: Vec::new(),
        output_fields: Vec::new(),
        requires_saved_policy: true,
        requires_authorization,
        mutates_remote_state: false,
        requires_confirmation: false,
    }
}

/// Builds a minimal field descriptor with the given name, scope, kind,
/// requiredness, and secret flag; all presentation and default slots stay
/// empty.
pub fn storage_connector_field(
    name: &str,
    scope: StorageConnectorFieldScope,
    kind: StorageConnectorFieldKind,
    required: bool,
    secret: bool,
) -> StorageConnectorFieldDescriptor {
    storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
        name,
        scope,
        kind,
        required,
        secret,
        label_key: name,
        placeholder: None,
        help_key: None,
        required_message_key: None,
        invalid_protocol_message_key: None,
        allowed_endpoint_protocols: Vec::new(),
        allow_endpoint_without_protocol: false,
        trim_on_blur: false,
    })
}

/// Constructor input for [`storage_connector_field_with_display`], carrying
/// the display-oriented slots a plain field builder leaves empty.
pub struct StorageConnectorFieldDisplayInput<'a> {
    /// Field name in the submitted payload.
    pub name: &'a str,
    /// Config domain the field writes into.
    pub scope: StorageConnectorFieldScope,
    /// Base control type available to the frontend.
    pub kind: StorageConnectorFieldKind,
    /// Whether the field must be non-empty to save.
    pub required: bool,
    /// Whether the field holds credential material.
    pub secret: bool,
    /// Frontend localization label key.
    pub label_key: &'a str,
    /// Optional placeholder text.
    pub placeholder: Option<&'a str>,
    /// Optional help copy key.
    pub help_key: Option<&'a str>,
    /// Frontend copy key for the required-validation failure.
    pub required_message_key: Option<&'a str>,
    /// Frontend copy key for an invalid endpoint protocol.
    pub invalid_protocol_message_key: Option<&'a str>,
    /// URL protocols the endpoint accepts, matching browser
    /// `URL.protocol` spelling (for example `https:`).
    pub allowed_endpoint_protocols: Vec<&'a str>,
    /// Whether the endpoint may omit the URL scheme.
    pub allow_endpoint_without_protocol: bool,
    /// Whether the frontend may safely trim the value on blur.
    pub trim_on_blur: bool,
}

/// Builds a field descriptor from explicit display inputs, resolving the
/// required/secret bits through [`StorageDescriptorFieldSemantics`]; select
/// and default slots stay empty.
pub fn storage_connector_field_with_display(
    input: StorageConnectorFieldDisplayInput<'_>,
) -> StorageConnectorFieldDescriptor {
    let semantics = StorageDescriptorFieldSemantics::from_descriptor_bits(
        input.kind.into(),
        input.required,
        input.secret,
    );
    StorageConnectorFieldDescriptor {
        name: input.name.to_string(),
        scope: input.scope,
        kind: semantics.kind.into(),
        label_key: input.label_key.to_string(),
        placeholder: input.placeholder.map(ToOwned::to_owned),
        help_key: input.help_key.map(ToOwned::to_owned),
        required_message_key: input.required_message_key.map(ToOwned::to_owned),
        invalid_protocol_message_key: input.invalid_protocol_message_key.map(ToOwned::to_owned),
        allowed_endpoint_protocols: input
            .allowed_endpoint_protocols
            .into_iter()
            .map(ToOwned::to_owned)
            .collect(),
        allow_endpoint_without_protocol: input.allow_endpoint_without_protocol,
        trim_on_blur: input.trim_on_blur,
        required: semantics.required,
        secret: semantics.secret,
        update_behavior: StorageConnectorFieldUpdateBehavior::Mutable,
        select: None,
        default_value: None,
        default_mode: StorageConnectorFieldDefaultMode::MissingOnly,
        default_rules: Vec::new(),
        visible_when: Vec::new(),
        required_when: Vec::new(),
        inactive_value_behavior: StorageConnectorInactiveValueBehavior::Preserve,
        advanced_group_key: None,
        validation: StorageConnectorFieldValidation::default(),
    }
}

/// Builds a string-valued select field whose fixed options use each value as
/// its own localization key.
pub fn storage_connector_field_with_options(
    name: &str,
    scope: StorageConnectorFieldScope,
    kind: StorageConnectorFieldKind,
    required: bool,
    secret: bool,
    options: Vec<&str>,
) -> StorageConnectorFieldDescriptor {
    StorageConnectorFieldDescriptor {
        select: Some(StorageConnectorSelectDescriptor {
            value_kind: StorageConnectorSelectValueKind::String,
            options: options
                .into_iter()
                .map(|value| StorageConnectorSelectOption {
                    value: StorageConnectorSelectOptionValue::String(value.to_string()),
                    label_key: value.to_string(),
                    description_key: None,
                    available_when: Vec::new(),
                })
                .collect(),
            data_source: None,
            depends_on: None,
            automatic_default_label_key: None,
            allow_custom_value: false,
            custom_value_label_key: None,
        }),
        ..storage_connector_field(name, scope, kind, required, secret)
    }
}

/// One fixed select option as passed to [`storage_connector_select_field`].
pub struct StorageConnectorSelectOptionInput<'a> {
    /// Stable value submitted in the connector payload.
    pub value: &'a str,
    /// Connector-owned localization key for the option label.
    pub label_key: &'a str,
    /// Optional connector-owned explanation key for richer selectors.
    pub description_key: Option<&'a str>,
}

/// Builds a required-string select field with fixed options and no dynamic
/// data source.
pub fn storage_connector_select_field(
    name: &str,
    scope: StorageConnectorFieldScope,
    required: bool,
    options: Vec<StorageConnectorSelectOptionInput<'_>>,
) -> StorageConnectorFieldDescriptor {
    StorageConnectorFieldDescriptor {
        select: Some(StorageConnectorSelectDescriptor {
            value_kind: StorageConnectorSelectValueKind::String,
            options: options
                .into_iter()
                .map(|option| StorageConnectorSelectOption {
                    value: StorageConnectorSelectOptionValue::String(option.value.to_string()),
                    label_key: option.label_key.to_string(),
                    description_key: option.description_key.map(ToOwned::to_owned),
                    available_when: Vec::new(),
                })
                .collect(),
            data_source: None,
            depends_on: None,
            automatic_default_label_key: None,
            allow_custom_value: false,
            custom_value_label_key: None,
        }),
        ..storage_connector_field(
            name,
            scope,
            StorageConnectorFieldKind::Select,
            required,
            false,
        )
    }
}

/// Builds a select field whose options are populated at runtime from a
/// platform-provided catalog instead of fixed connector options.
pub fn storage_connector_dynamic_select_field(
    name: &str,
    scope: StorageConnectorFieldScope,
    required: bool,
    value_kind: StorageConnectorSelectValueKind,
    data_source: StorageConnectorSelectDataSource,
    depends_on: Option<&str>,
) -> StorageConnectorFieldDescriptor {
    StorageConnectorFieldDescriptor {
        select: Some(StorageConnectorSelectDescriptor {
            value_kind,
            options: Vec::new(),
            data_source: Some(data_source),
            depends_on: depends_on.map(ToOwned::to_owned),
            automatic_default_label_key: None,
            allow_custom_value: false,
            custom_value_label_key: None,
        }),
        ..storage_connector_field(
            name,
            scope,
            StorageConnectorFieldKind::Select,
            required,
            false,
        )
    }
}

/// Constructor input for [`storage_connector_ui_descriptor`], carrying the
/// localization keys and badge color of the admin presentation.
pub struct StorageConnectorUiDescriptorInput {
    /// Frontend i18n label key.
    pub label_key: &'static str,
    /// Frontend i18n description key.
    pub description_key: &'static str,
    /// Connector-owned badge accent color.
    pub badge_rgb: StorageConnectorBadgeRgb,
    /// Create-wizard helper copy key.
    pub helper_key: &'static str,
    /// Create-wizard config step title key.
    pub config_step_title_key: &'static str,
    /// Create-wizard config step description key.
    pub config_step_description_key: &'static str,
    /// Edit page context copy key.
    pub edit_context_key: &'static str,
    /// Fallback copy shown when `base_path` is empty.
    pub base_path_empty_display: &'static str,
    /// `base_path` input placeholder.
    pub base_path_placeholder: &'static str,
}

/// Builds UI presentation metadata from static input keys; the icon slot
/// starts empty and providers attach it separately when they serve one.
pub fn storage_connector_ui_descriptor(
    input: StorageConnectorUiDescriptorInput,
) -> StorageConnectorUiDescriptor {
    StorageConnectorUiDescriptor {
        label_key: input.label_key.to_string(),
        description_key: input.description_key.to_string(),
        icon: None,
        badge_rgb: input.badge_rgb,
        helper_key: input.helper_key.to_string(),
        config_step_title_key: input.config_step_title_key.to_string(),
        config_step_description_key: input.config_step_description_key.to_string(),
        edit_context_key: input.edit_context_key.to_string(),
        base_path_empty_display: input.base_path_empty_display.to_string(),
        base_path_placeholder: input.base_path_placeholder.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        ObjectStorageConnectorDescriptorInput, StorageConnectorActionEndpoint,
        StorageConnectorActionId, StorageConnectorActionInvocationError,
        StorageConnectorActionKind, StorageConnectorActionOutputFieldDescriptor,
        StorageConnectorActionOutputValueKind, StorageConnectorBadgeRgb,
        StorageConnectorCredentialManagementDescriptor, StorageConnectorCredentialMode,
        StorageConnectorCredentialReasonRule, StorageConnectorCredentialStatusPresentation,
        StorageConnectorCredentialStatusTone, StorageConnectorCustomActionDescriptorInput,
        StorageConnectorDeploymentScope, StorageConnectorFieldCondition,
        StorageConnectorFieldDefaultMode, StorageConnectorFieldDefaultRule,
        StorageConnectorFieldDefaultValue, StorageConnectorFieldDisplayInput,
        StorageConnectorFieldKind, StorageConnectorFieldScope,
        StorageConnectorOptionsValidationError, StorageConnectorPromotionDescriptor,
        StorageConnectorPromotionFieldMapping, StorageConnectorPromotionId,
        StorageConnectorPromotionRequirement, StorageConnectorPromotionValueMatcher,
        StorageConnectorSelectDataSource, StorageConnectorSelectOption,
        StorageConnectorSelectOptionInput, StorageConnectorSelectOptionValue,
        StorageConnectorSelectValueKind, StorageConnectorUiDescriptorInput,
        custom_action_descriptor, normalize_storage_connector_action_input,
        normalize_storage_connector_config, normalize_storage_connector_custom_action_invocation,
        normalize_storage_connector_field_values, object_storage_connector_descriptor,
        start_authorization_action_descriptor, storage_connector_dynamic_select_field,
        storage_connector_field, storage_connector_field_with_display,
        storage_connector_field_with_options, storage_connector_select_field,
    };
    use crate::{
        CONNECTOR_CONFIG_FORMAT_VERSION, ConnectorConfigEnvelope, ConnectorId,
        StorageConnectorActionSchema,
    };

    #[test]
    fn deployment_scope_distinguishes_rejected_and_operator_managed_storage() {
        assert!(!StorageConnectorDeploymentScope::InstanceLocal.is_cluster_eligible());
        assert!(StorageConnectorDeploymentScope::DeploymentManaged.is_cluster_eligible());
        assert!(
            StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances.is_cluster_eligible()
        );
        assert_eq!(
            StorageConnectorDeploymentScope::DeploymentManaged.as_str(),
            "deployment_managed"
        );
    }

    crate::storage_connector_action_schema! {
        struct TestPluginActionInput {
            path: String => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "path",
                scope: StorageConnectorFieldScope::ActionInput,
                kind: StorageConnectorFieldKind::Text,
                required: true,
                secret: false,
                label_key: "path",
                placeholder: None,
                help_key: None,
                required_message_key: None,
                invalid_protocol_message_key: None,
                allowed_endpoint_protocols: Vec::new(),
                allow_endpoint_without_protocol: false,
                trim_on_blur: true,
            }),
            mode: String => storage_connector_field_with_options(
                "mode",
                StorageConnectorFieldScope::ActionInput,
                StorageConnectorFieldKind::Select,
                true,
                false,
                vec!["check", "apply"],
            ),
            token: String => storage_connector_field(
                "token",
                StorageConnectorFieldScope::ActionInput,
                StorageConnectorFieldKind::Secret,
                true,
                true,
            ),
            force: bool => {
                let mut field = storage_connector_field(
                    "force",
                    StorageConnectorFieldScope::ActionInput,
                    StorageConnectorFieldKind::Boolean,
                    false,
                    false,
                );
                field.default_value = Some(StorageConnectorFieldDefaultValue::Boolean(false));
                field
            },
        }
    }

    fn plugin_action_descriptor() -> super::StorageConnectorActionDescriptor {
        custom_action_descriptor(StorageConnectorCustomActionDescriptorInput {
            action_id: StorageConnectorActionId::declared("plugin.validate_path"),
            label_key: "plugin_validate_path",
            description_key: "plugin_validate_path_desc",
            fields: TestPluginActionInput::action_fields(),
            output_fields: vec![StorageConnectorActionOutputFieldDescriptor {
                name: "request_id".to_string(),
                label_key: "plugin_request_id".to_string(),
                value_kind: StorageConnectorActionOutputValueKind::Text,
                true_key: None,
                false_key: None,
            }],
            supports_draft: true,
            supports_saved: false,
            requires_authorization: true,
            mutates_remote_state: false,
            requires_confirmation: true,
        })
    }

    fn s3_descriptor() -> super::StorageConnectorDescriptor {
        let mut path_style = storage_connector_field(
            "s3_path_style",
            StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Boolean,
            false,
            false,
        );
        path_style.default_value = Some(StorageConnectorFieldDefaultValue::Boolean(true));

        let mut region = storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
            name: "s3_region",
            scope: StorageConnectorFieldScope::ConnectorConfig,
            kind: StorageConnectorFieldKind::Text,
            required: false,
            secret: false,
            label_key: "s3_region",
            placeholder: Some("auto"),
            help_key: None,
            required_message_key: None,
            invalid_protocol_message_key: None,
            allowed_endpoint_protocols: Vec::new(),
            allow_endpoint_without_protocol: false,
            trim_on_blur: true,
        });
        region.default_value = Some(StorageConnectorFieldDefaultValue::String(
            "auto".to_string(),
        ));

        let timeout_field = |name: &str, default_value: i64| {
            let mut field = storage_connector_field(
                name,
                StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Number,
                false,
                false,
            );
            field.default_value = Some(StorageConnectorFieldDefaultValue::Integer(default_value));
            field.validation.min_integer = Some(1);
            field
        };

        object_storage_connector_descriptor(ObjectStorageConnectorDescriptorInput {
            connector_id: ConnectorId::declared("asterdrive.storage.s3"),
            label: "S3",
            description: "test",
            ui: StorageConnectorUiDescriptorInput {
                label_key: "s3",
                description_key: "s3_desc",
                badge_rgb: StorageConnectorBadgeRgb::new(59, 130, 246),
                helper_key: "helper",
                config_step_title_key: "title",
                config_step_description_key: "description",
                edit_context_key: "edit",
                base_path_empty_display: "root",
                base_path_placeholder: "prefix",
            },
            deployment_scope: StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances,
            supports_initial_setup: true,
            credential_mode: StorageConnectorCredentialMode::StaticSecret,
            fields: vec![
                path_style,
                region,
                timeout_field("s3_connect_timeout_secs", 5),
                timeout_field("s3_read_timeout_secs", 30),
                timeout_field("s3_operation_timeout_secs", 3_600),
            ],
            presigned_part_etag_required: true,
            storage_native_processing: false,
            config_schema_version: 3,
            credential_schema_version: Some(1),
            related_issues: Vec::new(),
        })
    }

    fn promotion_descriptor() -> StorageConnectorPromotionDescriptor {
        StorageConnectorPromotionDescriptor {
            promotion_id: StorageConnectorPromotionId::declared("promote_from_source"),
            source_connector_id: ConnectorId::declared("com.example.source"),
            description_key: "promotion_desc".to_string(),
            confirmation_key: "promotion_confirm".to_string(),
            requirements: vec![StorageConnectorPromotionRequirement {
                source_field: "endpoint".to_string(),
                matcher: StorageConnectorPromotionValueMatcher::UrlHostSuffix {
                    suffix: ".example.com".to_string(),
                },
                negate: false,
            }],
            config_mappings: vec![StorageConnectorPromotionFieldMapping {
                source_field: "region".to_string(),
                target_field: "s3_region".to_string(),
                preserve_value: false,
            }],
            credential_mappings: Vec::new(),
        }
    }

    #[test]
    fn badge_rgb_uses_structured_channels_and_rejects_out_of_range_values() {
        let color = StorageConnectorBadgeRgb::new(16, 185, 129);
        assert_eq!(
            serde_json::to_value(color).unwrap(),
            serde_json::json!({ "red": 16, "green": 185, "blue": 129 })
        );
        assert_eq!(
            serde_json::from_value::<StorageConnectorBadgeRgb>(serde_json::json!({
                "red": 0,
                "green": 255,
                "blue": 128
            }))
            .unwrap(),
            StorageConnectorBadgeRgb::new(0, 255, 128)
        );
        assert!(
            serde_json::from_value::<StorageConnectorBadgeRgb>(serde_json::json!({
                "red": 256,
                "green": 0,
                "blue": 0
            }))
            .is_err()
        );
    }

    #[test]
    fn promotion_descriptor_rejects_invalid_local_contracts() {
        let mut valid = s3_descriptor();
        valid.promotions.push(promotion_descriptor());
        valid.validate().expect("valid promotion descriptor");
        assert!(
            valid
                .localization_message_ids()
                .contains("promotion_confirm")
        );

        let mut self_promotion = valid.clone();
        self_promotion.promotions[0].source_connector_id = self_promotion.connector_id.clone();
        assert!(
            self_promotion
                .validate()
                .expect_err("self promotion must fail")
                .to_string()
                .contains("must differ")
        );

        let mut duplicate_target = valid.clone();
        duplicate_target.promotions[0].config_mappings.push(
            StorageConnectorPromotionFieldMapping {
                source_field: "other_region".to_string(),
                target_field: "s3_region".to_string(),
                preserve_value: false,
            },
        );
        assert!(
            duplicate_target
                .validate()
                .expect_err("duplicate target mapping must fail")
                .to_string()
                .contains("more than once")
        );

        let mut missing_target = valid.clone();
        missing_target.promotions[0].config_mappings[0].target_field = "missing".to_string();
        assert!(
            missing_target
                .validate()
                .expect_err("missing target field must fail")
                .to_string()
                .contains("undeclared target config field")
        );

        let mut empty_matcher = valid.clone();
        empty_matcher.promotions[0].requirements[0].matcher =
            StorageConnectorPromotionValueMatcher::StringSuffix {
                suffix: " ".to_string(),
                case_sensitive: false,
            };
        assert!(
            empty_matcher
                .validate()
                .expect_err("empty matcher must fail")
                .to_string()
                .contains("matcher must not be empty")
        );

        let mut bare_url_host_suffix = valid.clone();
        bare_url_host_suffix.promotions[0].requirements[0].matcher =
            StorageConnectorPromotionValueMatcher::UrlHostSuffix {
                suffix: "example.com".to_string(),
            };
        assert!(
            bare_url_host_suffix
                .validate()
                .expect_err("URL host suffix without a label boundary must fail")
                .to_string()
                .contains("DNS label boundary marker")
        );

        let mut invalid_source_id = valid.clone();
        invalid_source_id.promotions[0].source_connector_id = ConnectorId::declared("INVALID ID");
        assert!(
            invalid_source_id
                .validate()
                .expect_err("invalid source connector id must fail")
                .to_string()
                .contains("invalid source connector id")
        );

        for clear_description in [true, false] {
            let mut empty_copy = valid.clone();
            if clear_description {
                empty_copy.promotions[0].description_key = " ".to_string();
            } else {
                empty_copy.promotions[0].confirmation_key = String::new();
            }
            assert!(
                empty_copy
                    .validate()
                    .expect_err("empty promotion copy key must fail")
                    .to_string()
                    .contains("non-empty description and confirmation")
            );
        }

        let mut empty_requirement_field = valid.clone();
        empty_requirement_field.promotions[0].requirements[0].source_field = " ".to_string();
        assert!(
            empty_requirement_field
                .validate()
                .expect_err("empty requirement source field must fail")
                .to_string()
                .contains("source field must not be empty")
        );

        let mut no_config_mapping = valid.clone();
        no_config_mapping.promotions[0].config_mappings.clear();
        assert!(
            no_config_mapping
                .validate()
                .expect_err("promotion without config mapping must fail")
                .to_string()
                .contains("at least one config mapping")
        );

        let mut empty_mapping_name = valid.clone();
        empty_mapping_name.promotions[0].config_mappings[0].source_field = String::new();
        assert!(
            empty_mapping_name
                .validate()
                .expect_err("empty mapping field name must fail")
                .to_string()
                .contains("non-empty names")
        );

        let mut duplicate_promotion = valid.clone();
        duplicate_promotion
            .promotions
            .push(duplicate_promotion.promotions[0].clone());
        assert!(
            duplicate_promotion
                .validate()
                .expect_err("duplicate promotion id must fail")
                .to_string()
                .contains("declared more than once")
        );

        let mut missing_credential_target = valid.clone();
        missing_credential_target.promotions[0]
            .credential_mappings
            .push(StorageConnectorPromotionFieldMapping {
                source_field: "source_secret".to_string(),
                target_field: "missing_secret".to_string(),
                preserve_value: false,
            });
        assert!(
            missing_credential_target
                .validate()
                .expect_err("undeclared target credential field must fail")
                .to_string()
                .contains("undeclared target credential field")
        );

        let mut invalid_id = valid;
        invalid_id.promotions[0].promotion_id = StorageConnectorPromotionId::declared("INVALID ID");
        assert!(
            invalid_id
                .validate()
                .expect_err("invalid promotion id must fail")
                .to_string()
                .contains("invalid id")
        );
    }

    #[test]
    fn connector_options_apply_defaults_and_normalize_text() {
        let descriptor = s3_descriptor();
        let input = ConnectorConfigEnvelope {
            format_version: CONNECTOR_CONFIG_FORMAT_VERSION,
            connector_id: ConnectorId::declared("asterdrive.storage.s3"),
            schema_version: 3,
            values: BTreeMap::from([(
                "s3_region".to_string(),
                serde_json::json!("  cn-beijing  "),
            )]),
        };

        let normalized = normalize_storage_connector_config(&descriptor, &input).unwrap();

        assert_eq!(normalized.values["s3_region"], "cn-beijing");
        assert_eq!(normalized.values["s3_path_style"], true);
        assert_eq!(normalized.values["s3_connect_timeout_secs"], 5);
        assert_eq!(normalized.values["s3_read_timeout_secs"], 30);
        assert_eq!(normalized.values["s3_operation_timeout_secs"], 3_600);
    }

    #[test]
    fn credential_lifecycle_actions_require_valid_connector_owned_presentation() {
        let mut descriptor = s3_descriptor();
        descriptor.actions = vec![start_authorization_action_descriptor()];

        let error = descriptor
            .validate()
            .expect_err("authorization action without credential presentation must fail");
        assert!(error.to_string().contains("require credential_management"));

        descriptor.credential_management = Some(StorageConnectorCredentialManagementDescriptor {
            title_key: "credential_title".to_string(),
            loading_key: "credential_loading".to_string(),
            status_presentations: BTreeMap::new(),
            reauthorize_action_key: None,
            authorized_at_key: None,
            refreshed_at_key: None,
            validated_at_key: None,
            redirect_uri_key: None,
            redirect_uri_help_key: None,
            redirect_uri_copy_key: None,
            save_before_authorize_key: None,
            authorization_started_key: None,
            save_before_validate_key: None,
            validation_success_key: None,
            validation_success_detail_key: None,
            created_authorize_next_key: None,
        });
        let error = descriptor
            .validate()
            .expect_err("empty credential status map must fail");
        assert!(
            error
                .to_string()
                .contains("status_presentations must not be empty")
        );

        descriptor
            .credential_management
            .as_mut()
            .expect("credential presentation")
            .status_presentations
            .insert(
                "authorized".to_string(),
                StorageConnectorCredentialStatusPresentation {
                    label_key: "credential_authorized".to_string(),
                    tone: StorageConnectorCredentialStatusTone::Success,
                    description_key: None,
                    attention_title_key: None,
                    attention_guidance_key: None,
                    reason_rules: Vec::new(),
                    reason_fallback_key: None,
                },
            );
        let error = descriptor
            .validate()
            .expect_err("credential presentation must declare missing status copy");
        assert!(error.to_string().contains("must declare 'missing'"));

        descriptor
            .credential_management
            .as_mut()
            .expect("credential presentation")
            .status_presentations
            .insert(
                "missing".to_string(),
                StorageConnectorCredentialStatusPresentation {
                    label_key: "credential_missing".to_string(),
                    tone: StorageConnectorCredentialStatusTone::Neutral,
                    description_key: None,
                    attention_title_key: None,
                    attention_guidance_key: None,
                    reason_rules: Vec::new(),
                    reason_fallback_key: None,
                },
            );
        descriptor
            .validate()
            .expect("valid credential presentation");
        assert!(
            descriptor
                .localization_message_ids()
                .contains("credential_authorized")
        );

        for field in ["redirect_uri_help_key", "redirect_uri_copy_key"] {
            let mut invalid = descriptor.clone();
            let management = invalid
                .credential_management
                .as_mut()
                .expect("credential presentation");
            match field {
                "redirect_uri_help_key" => {
                    management.redirect_uri_help_key = Some("credential_redirect_help".to_string());
                }
                "redirect_uri_copy_key" => {
                    management.redirect_uri_copy_key = Some("credential_redirect_copy".to_string());
                }
                _ => unreachable!("fixture only declares redirect URI metadata fields"),
            }
            let error = invalid
                .validate()
                .expect_err("redirect URI metadata without redirect_uri_key must fail");
            assert!(error.to_string().contains("requires redirect_uri_key"));
        }

        for (contains_any, message_key, expected) in [
            (
                Vec::<String>::new(),
                "credential_reason".to_string(),
                "non-empty fragments",
            ),
            (
                vec!["  ".to_string()],
                "credential_reason".to_string(),
                "non-empty fragments",
            ),
            (
                vec!["invalid_grant".to_string()],
                "  ".to_string(),
                "message_key must not be empty",
            ),
        ] {
            let mut invalid = descriptor.clone();
            invalid
                .credential_management
                .as_mut()
                .expect("credential presentation")
                .status_presentations
                .get_mut("missing")
                .expect("missing presentation")
                .reason_rules = vec![StorageConnectorCredentialReasonRule {
                contains_any,
                message_key,
            }];
            let error = invalid
                .validate()
                .expect_err("invalid credential reason rule must fail");
            assert!(error.to_string().contains(expected));
        }
    }

    #[test]
    fn connector_default_mode_distinguishes_missing_and_empty_text() {
        let mut descriptor = s3_descriptor();
        let mut base_path = storage_connector_field(
            "base_path",
            StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Text,
            false,
            false,
        );
        base_path.default_value = Some(StorageConnectorFieldDefaultValue::String(
            "connector-root".to_string(),
        ));
        base_path.default_mode = StorageConnectorFieldDefaultMode::MissingOrEmptyText;
        base_path.validate().unwrap();
        descriptor.fields.push(base_path.clone());

        for values in [
            BTreeMap::new(),
            BTreeMap::from([("base_path".to_string(), serde_json::Value::Null)]),
            BTreeMap::from([("base_path".to_string(), serde_json::json!(""))]),
        ] {
            let normalized = normalize_storage_connector_config(
                &descriptor,
                &ConnectorConfigEnvelope {
                    format_version: CONNECTOR_CONFIG_FORMAT_VERSION,
                    connector_id: ConnectorId::declared("asterdrive.storage.s3"),
                    schema_version: 3,
                    values,
                },
            )
            .unwrap();
            assert_eq!(normalized.values["base_path"], "connector-root");
        }

        let normalized = normalize_storage_connector_config(
            &descriptor,
            &ConnectorConfigEnvelope {
                format_version: CONNECTOR_CONFIG_FORMAT_VERSION,
                connector_id: ConnectorId::declared("asterdrive.storage.s3"),
                schema_version: 3,
                values: BTreeMap::from([("base_path".to_string(), serde_json::json!("tenant-a"))]),
            },
        )
        .unwrap();
        assert_eq!(normalized.values["base_path"], "tenant-a");

        base_path.default_mode = StorageConnectorFieldDefaultMode::MissingOnly;
        descriptor.fields.pop();
        descriptor.fields.push(base_path);
        let normalized = normalize_storage_connector_config(
            &descriptor,
            &ConnectorConfigEnvelope {
                format_version: CONNECTOR_CONFIG_FORMAT_VERSION,
                connector_id: ConnectorId::declared("asterdrive.storage.s3"),
                schema_version: 3,
                values: BTreeMap::from([("base_path".to_string(), serde_json::json!(""))]),
            },
        )
        .unwrap();
        assert!(!normalized.values.contains_key("base_path"));
    }

    #[test]
    fn null_secret_is_treated_as_not_supplied() {
        let secret = storage_connector_field(
            "token",
            StorageConnectorFieldScope::StaticCredential,
            StorageConnectorFieldKind::Secret,
            true,
            true,
        );
        let normalized = normalize_storage_connector_field_values(
            [&secret],
            &BTreeMap::from([("token".to_string(), serde_json::Value::Null)]),
            true,
        )
        .expect("null secret should be treated as omitted");
        assert!(normalized.is_empty());

        assert!(matches!(
            normalize_storage_connector_field_values(
                [&secret],
                &BTreeMap::from([("token".to_string(), serde_json::json!("secret"))]),
                true,
            ),
            Err(StorageConnectorOptionsValidationError::SecretField(field)) if field == "token"
        ));
    }

    #[test]
    fn empty_text_default_mode_rejects_invalid_descriptor_combinations() {
        let mut field = storage_connector_field(
            "base_path",
            StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Text,
            false,
            false,
        );
        field.default_mode = StorageConnectorFieldDefaultMode::MissingOrEmptyText;
        assert!(field.validate().is_err());
        assert!(matches!(
            normalize_storage_connector_field_values(
                [&field],
                &BTreeMap::from([("base_path".to_string(), serde_json::json!(""))]),
                false,
            ),
            Err(StorageConnectorOptionsValidationError::InvalidFieldDescriptor { .. })
        ));

        field.default_value = Some(StorageConnectorFieldDefaultValue::String(String::new()));
        field.required = true;
        assert!(field.validate().is_err());

        field.required = false;
        field.kind = StorageConnectorFieldKind::Boolean;
        field.default_value = Some(StorageConnectorFieldDefaultValue::Boolean(false));
        assert!(field.validate().is_err());
    }

    #[test]
    fn connector_options_reject_namespace_version_unknown_type_and_range_errors() {
        let descriptor = s3_descriptor();
        let namespace = ConnectorConfigEnvelope {
            format_version: CONNECTOR_CONFIG_FORMAT_VERSION,
            connector_id: ConnectorId::declared("asterdrive.storage.s3"),
            schema_version: 3,
            values: BTreeMap::new(),
        };
        assert!(matches!(
            normalize_storage_connector_config(
                &descriptor,
                &ConnectorConfigEnvelope {
                    connector_id: ConnectorId::declared("wrong.namespace"),
                    ..namespace.clone()
                }
            ),
            Err(StorageConnectorOptionsValidationError::NamespaceMismatch { .. })
        ));

        let mut wrong_version = namespace.clone();
        wrong_version.schema_version = 2;
        assert!(matches!(
            normalize_storage_connector_config(&descriptor, &wrong_version),
            Err(StorageConnectorOptionsValidationError::SchemaVersionMismatch { .. })
        ));

        let mut unknown = namespace.clone();
        unknown
            .values
            .insert("secret_backdoor".to_string(), serde_json::json!(true));
        assert!(matches!(
            normalize_storage_connector_config(&descriptor, &unknown),
            Err(StorageConnectorOptionsValidationError::UnknownField(_))
        ));

        let mut wrong_type = namespace.clone();
        wrong_type
            .values
            .insert("s3_path_style".to_string(), serde_json::json!("false"));
        assert!(matches!(
            normalize_storage_connector_config(&descriptor, &wrong_type),
            Err(StorageConnectorOptionsValidationError::TypeMismatch { .. })
        ));

        let mut below_minimum = namespace;
        below_minimum
            .values
            .insert("s3_connect_timeout_secs".to_string(), serde_json::json!(0));
        assert!(matches!(
            normalize_storage_connector_config(&descriptor, &below_minimum),
            Err(StorageConnectorOptionsValidationError::IntegerBelowMinimum { .. })
        ));
    }

    #[test]
    fn connector_option_defaults_match_declared_scalar_types() {
        let descriptor = s3_descriptor();
        let path_style = descriptor
            .fields
            .iter()
            .find(|field| field.name == "s3_path_style")
            .unwrap();
        assert_eq!(
            path_style.default_value,
            Some(StorageConnectorFieldDefaultValue::Boolean(true))
        );
    }

    #[test]
    fn select_contract_preserves_connector_owned_labels_and_accepts_dynamic_integer_values() {
        let static_field = storage_connector_select_field(
            "mode",
            StorageConnectorFieldScope::ConnectorConfig,
            true,
            vec![StorageConnectorSelectOptionInput {
                value: "check",
                label_key: "plugin_mode_check",
                description_key: Some("plugin_mode_check_desc"),
            }],
        );
        let select = static_field.select.as_ref().unwrap();
        assert_eq!(select.value_kind, StorageConnectorSelectValueKind::String);
        assert_eq!(
            select.options,
            vec![StorageConnectorSelectOption {
                value: StorageConnectorSelectOptionValue::String("check".to_string()),
                label_key: "plugin_mode_check".to_string(),
                description_key: Some("plugin_mode_check_desc".to_string()),
                available_when: Vec::new(),
            }]
        );
        static_field.validate().unwrap();

        let remote_node = storage_connector_dynamic_select_field(
            "remote_node_id",
            StorageConnectorFieldScope::ConnectorConfig,
            true,
            StorageConnectorSelectValueKind::Integer,
            StorageConnectorSelectDataSource::RemoteNodes,
            None,
        );
        remote_node.validate().unwrap();

        let mut descriptor = s3_descriptor();
        descriptor.fields = vec![remote_node];
        let valid = ConnectorConfigEnvelope {
            format_version: CONNECTOR_CONFIG_FORMAT_VERSION,
            connector_id: descriptor.connector_id.clone(),
            schema_version: descriptor.config_schema_version,
            values: BTreeMap::from([("remote_node_id".to_string(), serde_json::json!(7))]),
        };
        let normalized = normalize_storage_connector_config(&descriptor, &valid).unwrap();
        assert_eq!(normalized.values["remote_node_id"], 7);

        let mut wrong_type = valid;
        wrong_type
            .values
            .insert("remote_node_id".to_string(), serde_json::json!("7"));
        assert!(matches!(
            normalize_storage_connector_config(&descriptor, &wrong_type),
            Err(StorageConnectorOptionsValidationError::TypeMismatch { field, .. })
                if field == "remote_node_id"
        ));
    }

    #[test]
    fn negated_default_condition_matches_present_unequal_values_only() {
        let mut value = storage_connector_field(
            "target",
            StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Text,
            false,
            false,
        );
        value.default_rules = vec![super::StorageConnectorFieldDefaultRule {
            conditions: vec![StorageConnectorFieldCondition {
                scope: None,
                field: "mode".into(),
                value: StorageConnectorFieldDefaultValue::String("origin".into()),
                negate: true,
            }],
            value: StorageConnectorFieldDefaultValue::String("custom".into()),
        }];
        for (mode, expected) in [
            (None, None),
            (Some("origin"), None),
            (Some("custom_origin"), Some("custom")),
            (Some("cdn"), Some("custom")),
        ] {
            let values = mode
                .map(|m| BTreeMap::from([("mode".into(), serde_json::json!(m))]))
                .unwrap_or_default();
            assert_eq!(
                super::resolved_field_default(&value, &values).map(super::default_value_to_json),
                expected.map(serde_json::Value::from)
            );
        }
        assert!(
            super::resolved_field_default(
                &value,
                &BTreeMap::from([("mode".into(), serde_json::Value::Null)])
            )
            .is_none()
        );
        let encoded = serde_json::json!({"scope":null,"field":"mode","value":"origin"});
        let condition: StorageConnectorFieldCondition = serde_json::from_value(encoded).unwrap();
        assert!(!condition.negate);
        assert!(
            serde_json::to_value(condition)
                .unwrap()
                .get("negate")
                .is_none()
        );
    }

    #[test]
    fn string_select_accepts_trimmed_custom_values_when_declared() {
        let mut tenant = storage_connector_select_field(
            "tenant",
            StorageConnectorFieldScope::ConnectorConfig,
            true,
            vec![StorageConnectorSelectOptionInput {
                value: "common",
                label_key: "tenant_common",
                description_key: None,
            }],
        );
        tenant.default_value = Some(StorageConnectorFieldDefaultValue::String(
            "common".to_string(),
        ));
        tenant.trim_on_blur = true;
        tenant.validation.max_length = Some(256);
        let select = tenant.select.as_mut().unwrap();
        select.automatic_default_label_key = Some("tenant_auto".to_string());
        select.allow_custom_value = true;
        select.custom_value_label_key = Some("tenant_custom".to_string());
        tenant.validate().unwrap();

        let mut descriptor = s3_descriptor();
        descriptor.fields = vec![tenant];
        let input = ConnectorConfigEnvelope {
            format_version: CONNECTOR_CONFIG_FORMAT_VERSION,
            connector_id: descriptor.connector_id.clone(),
            schema_version: descriptor.config_schema_version,
            values: BTreeMap::from([(
                "tenant".to_string(),
                serde_json::json!(" contoso.onmicrosoft.com "),
            )]),
        };

        let normalized = normalize_storage_connector_config(&descriptor, &input).unwrap();
        assert_eq!(normalized.values["tenant"], "contoso.onmicrosoft.com");
    }

    #[test]
    fn select_field_validation_rejects_ambiguous_and_incoherent_contracts() {
        let valid = storage_connector_select_field(
            "mode",
            StorageConnectorFieldScope::ConnectorConfig,
            true,
            vec![StorageConnectorSelectOptionInput {
                value: "check",
                label_key: "plugin_mode_check",
                description_key: None,
            }],
        );
        let mut cases = Vec::new();

        let mut missing_contract = valid.clone();
        missing_contract.select = None;
        cases.push((missing_contract, "missing its select contract"));

        let mut non_select = valid.clone();
        non_select.kind = StorageConnectorFieldKind::Text;
        cases.push((non_select, "non-select field"));

        let mut both_sources = valid.clone();
        both_sources.select.as_mut().unwrap().data_source =
            Some(StorageConnectorSelectDataSource::RemoteNodes);
        cases.push((both_sources, "exactly one"));

        let mut duplicate_option = valid.clone();
        let first_option = duplicate_option.select.as_ref().unwrap().options[0].clone();
        duplicate_option
            .select
            .as_mut()
            .unwrap()
            .options
            .push(first_option);
        cases.push((duplicate_option, "duplicate option"));

        let mut wrong_option_type = valid.clone();
        wrong_option_type.select.as_mut().unwrap().options[0].value =
            StorageConnectorSelectOptionValue::Integer(1);
        cases.push((wrong_option_type, "does not match value_kind"));

        let mut invalid_default = valid.clone();
        invalid_default.default_value = Some(StorageConnectorFieldDefaultValue::String(
            "missing".to_string(),
        ));
        cases.push((invalid_default, "default value is not a declared option"));

        let mut invalid_conditional_default = valid.clone();
        invalid_conditional_default.default_rules = vec![StorageConnectorFieldDefaultRule {
            conditions: vec![StorageConnectorFieldCondition {
                negate: false,
                scope: None,
                field: "other".to_string(),
                value: StorageConnectorFieldDefaultValue::String("value".to_string()),
            }],
            value: StorageConnectorFieldDefaultValue::String("missing".to_string()),
        }];
        cases.push((
            invalid_conditional_default,
            "conditional default is not a declared option",
        ));

        let mut automatic_without_default = valid.clone();
        automatic_without_default
            .select
            .as_mut()
            .unwrap()
            .automatic_default_label_key = Some("automatic".to_string());
        cases.push((
            automatic_without_default,
            "automatic default option requires a default value or default rule",
        ));

        let mut custom_without_label = valid.clone();
        custom_without_label
            .select
            .as_mut()
            .unwrap()
            .allow_custom_value = true;
        cases.push((
            custom_without_label,
            "custom values require custom_value_label_key",
        ));

        let wrong_dynamic_type = storage_connector_dynamic_select_field(
            "remote_node_id",
            StorageConnectorFieldScope::ConnectorConfig,
            true,
            StorageConnectorSelectValueKind::String,
            StorageConnectorSelectDataSource::RemoteNodes,
            None,
        );
        cases.push((wrong_dynamic_type, "remote_nodes source requires integer"));

        let missing_dynamic_dependency = storage_connector_dynamic_select_field(
            "remote_storage_target_key",
            StorageConnectorFieldScope::ConnectorConfig,
            true,
            StorageConnectorSelectValueKind::String,
            StorageConnectorSelectDataSource::RemoteStorageTargets,
            None,
        );
        cases.push((
            missing_dynamic_dependency,
            "remote_storage_targets source requires string values and one dependency",
        ));

        for (field, expected_message) in cases {
            let error = field
                .validate()
                .expect_err("invalid select contract must be rejected");
            assert!(
                error.to_string().contains(expected_message),
                "expected '{expected_message}' in '{error}'"
            );
        }
    }

    #[test]
    fn connector_descriptor_validation_rejects_missing_and_cyclic_dependencies() {
        let target_field = |name: &str, dependency: &str| {
            storage_connector_dynamic_select_field(
                name,
                StorageConnectorFieldScope::ConnectorConfig,
                true,
                StorageConnectorSelectValueKind::String,
                StorageConnectorSelectDataSource::RemoteStorageTargets,
                Some(dependency),
            )
        };

        let mut missing = s3_descriptor();
        missing.fields = vec![target_field("target", "remote_node_id")];
        assert!(
            missing
                .validate()
                .unwrap_err()
                .to_string()
                .contains("depends on undeclared field")
        );

        let mut cyclic = s3_descriptor();
        cyclic.fields = vec![
            target_field("target_a", "target_b"),
            target_field("target_b", "target_a"),
        ];
        assert!(
            cyclic
                .validate()
                .unwrap_err()
                .to_string()
                .contains("dependency cycle")
        );

        let mut missing_condition = s3_descriptor();
        let mut conditioned = storage_connector_field(
            "conditioned",
            StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Text,
            false,
            false,
        );
        conditioned.visible_when = vec![StorageConnectorFieldCondition {
            negate: false,
            scope: None,
            field: "missing".to_string(),
            value: StorageConnectorFieldDefaultValue::String("value".to_string()),
        }];
        missing_condition.fields = vec![conditioned];
        assert!(
            missing_condition
                .validate()
                .unwrap_err()
                .to_string()
                .contains("condition references undeclared field")
        );

        let conditional_default = |name: &str, dependency: &str| {
            let mut field = storage_connector_field(
                name,
                StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Text,
                false,
                false,
            );
            field.default_rules = vec![StorageConnectorFieldDefaultRule {
                conditions: vec![StorageConnectorFieldCondition {
                    negate: false,
                    scope: None,
                    field: dependency.to_string(),
                    value: StorageConnectorFieldDefaultValue::String("value".to_string()),
                }],
                value: StorageConnectorFieldDefaultValue::String("default".to_string()),
            }];
            field
        };
        let mut conditional_cycle = s3_descriptor();
        conditional_cycle.fields = vec![
            conditional_default("field_a", "field_b"),
            conditional_default("field_b", "field_a"),
        ];
        assert!(
            conditional_cycle
                .validate()
                .unwrap_err()
                .to_string()
                .contains("conditional field cycle")
        );

        let conditional_visibility = |name: &str, dependency: &str| {
            let mut field = storage_connector_field(
                name,
                StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Text,
                false,
                false,
            );
            field.visible_when = vec![StorageConnectorFieldCondition {
                negate: false,
                scope: None,
                field: dependency.to_string(),
                value: StorageConnectorFieldDefaultValue::String("value".to_string()),
            }];
            field
        };
        let mut visibility_cycle = s3_descriptor();
        visibility_cycle.fields = vec![
            conditional_visibility("field_a", "field_b"),
            conditional_visibility("field_b", "field_a"),
        ];
        assert!(
            visibility_cycle
                .validate()
                .unwrap_err()
                .to_string()
                .contains("conditional field cycle")
        );
    }

    #[test]
    fn descriptor_condition_values_match_select_value_kind() {
        let mut string_select = storage_connector_select_field(
            "mode",
            StorageConnectorFieldScope::ConnectorConfig,
            true,
            vec![StorageConnectorSelectOptionInput {
                value: "check",
                label_key: "plugin_mode_check",
                description_key: None,
            }],
        );
        string_select.default_value = Some(StorageConnectorFieldDefaultValue::String(
            "check".to_string(),
        ));
        let mut dependent = storage_connector_field(
            "dependent",
            StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Text,
            false,
            false,
        );
        dependent.visible_when = vec![StorageConnectorFieldCondition {
            negate: false,
            scope: None,
            field: "mode".to_string(),
            value: StorageConnectorFieldDefaultValue::Integer(1),
        }];

        let mut descriptor = s3_descriptor();
        descriptor.fields = vec![string_select, dependent];
        assert!(
            descriptor
                .validate()
                .unwrap_err()
                .to_string()
                .contains("condition value does not match referenced field 'mode' kind")
        );
    }

    #[test]
    fn custom_action_descriptor_exposes_identity_fields_and_execution_contract() {
        let descriptor = plugin_action_descriptor();

        assert_eq!(descriptor.action_id.as_str(), "plugin.validate_path");
        assert_eq!(descriptor.kind, StorageConnectorActionKind::Custom);
        assert_eq!(
            descriptor.endpoints,
            vec![StorageConnectorActionEndpoint::ExecuteDraftStoragePolicyAction]
        );
        assert!(!descriptor.requires_saved_policy);
        assert!(descriptor.requires_authorization);
        assert!(descriptor.requires_confirmation);
        assert_eq!(descriptor.fields, TestPluginActionInput::action_fields());
        assert!(
            descriptor
                .fields
                .iter()
                .all(|field| field.scope == StorageConnectorFieldScope::ActionInput)
        );
    }

    #[test]
    fn action_descriptor_validation_accepts_a_complete_connector_owned_schema() {
        plugin_action_descriptor()
            .validate()
            .expect("complete plugin action descriptor should be accepted");
    }

    #[test]
    fn action_descriptor_validation_rejects_incoherent_contract_boundaries() {
        let valid = plugin_action_descriptor();
        let mut cases = Vec::new();

        let mut empty_label = valid.clone();
        empty_label.label_key = "  ".to_string();
        cases.push((empty_label, "label_key"));

        let mut empty_description = valid.clone();
        empty_description.description_key.clear();
        cases.push((empty_description, "description_key"));

        let mut empty_endpoints = valid.clone();
        empty_endpoints.endpoints.clear();
        cases.push((empty_endpoints, "at least one endpoint"));

        let mut duplicate_endpoint = valid.clone();
        duplicate_endpoint
            .endpoints
            .push(StorageConnectorActionEndpoint::ExecuteDraftStoragePolicyAction);
        cases.push((duplicate_endpoint, "same endpoint"));

        let mut wrong_kind_endpoint = valid.clone();
        wrong_kind_endpoint.kind = StorageConnectorActionKind::Authorization;
        cases.push((wrong_kind_endpoint, "does not accept endpoint"));

        let mut saved_requirement = valid.clone();
        saved_requirement.requires_saved_policy = true;
        cases.push((saved_requirement, "requires_saved_policy"));

        let mut wrong_field_scope = valid.clone();
        wrong_field_scope.fields[0].scope = StorageConnectorFieldScope::ConnectorConfig;
        cases.push((wrong_field_scope, "action_input scope"));

        let mut duplicate_field = valid;
        duplicate_field
            .fields
            .push(duplicate_field.fields[0].clone());
        cases.push((duplicate_field, "declared more than once"));

        let mut empty_output_name = plugin_action_descriptor();
        empty_output_name.output_fields[0].name.clear();
        cases.push((empty_output_name, "output field name"));

        let mut duplicate_output = plugin_action_descriptor();
        duplicate_output
            .output_fields
            .push(duplicate_output.output_fields[0].clone());
        cases.push((duplicate_output, "output field 'request_id'"));

        let mut incomplete_boolean_output = plugin_action_descriptor();
        incomplete_boolean_output.output_fields[0].value_kind =
            StorageConnectorActionOutputValueKind::Boolean;
        cases.push((incomplete_boolean_output, "requires non-empty true_key"));

        let mut non_boolean_labels = plugin_action_descriptor();
        non_boolean_labels.output_fields[0].true_key = Some("plugin_yes".to_string());
        cases.push((non_boolean_labels, "must not declare boolean labels"));

        for (descriptor, expected_message) in cases {
            let error = descriptor
                .validate()
                .expect_err("incoherent action descriptor must be rejected");
            assert!(
                error.to_string().contains(expected_message),
                "expected '{expected_message}' in '{error}'"
            );
        }
    }

    #[test]
    fn action_id_validation_accepts_plugin_namespaces_and_rejects_ambiguous_keys() {
        for value in [
            "run",
            "configure_cors",
            "plugin.verify-path",
            "com.example.storage.verify_1",
        ] {
            assert!(
                StorageConnectorActionId::declared(value).validate().is_ok(),
                "valid action id rejected: {value}"
            );
        }

        for value in [
            "",
            "ab",
            "Invalid",
            "has space",
            ".leading",
            "trailing_",
            "double..segment",
            "插件.action",
        ] {
            assert!(
                StorageConnectorActionId::declared(value)
                    .validate()
                    .is_err(),
                "invalid action id accepted: {value}"
            );
        }
    }

    #[test]
    fn action_input_applies_defaults_normalizes_text_and_accepts_ephemeral_secrets() {
        let descriptor = plugin_action_descriptor();
        let input = BTreeMap::from([
            ("path".to_string(), serde_json::json!("  /uploads/file  ")),
            ("mode".to_string(), serde_json::json!("check")),
            ("token".to_string(), serde_json::json!("secret-value")),
        ]);

        let normalized = normalize_storage_connector_action_input(&descriptor, &input).unwrap();
        let decoded: TestPluginActionInput =
            serde_json::from_value(serde_json::to_value(&normalized).unwrap()).unwrap();

        assert_eq!(decoded.path, "/uploads/file");
        assert_eq!(decoded.mode, "check");
        assert_eq!(decoded.token, "secret-value");
        assert!(!decoded.force);
    }

    #[test]
    fn action_input_rejects_unknown_missing_invalid_option_and_nested_values() {
        let descriptor = plugin_action_descriptor();
        let valid = BTreeMap::from([
            ("path".to_string(), serde_json::json!("/uploads/file")),
            ("mode".to_string(), serde_json::json!("check")),
            ("token".to_string(), serde_json::json!("secret-value")),
        ]);

        let mut unknown = valid.clone();
        unknown.insert("undeclared".to_string(), serde_json::json!(true));
        assert!(matches!(
            normalize_storage_connector_action_input(&descriptor, &unknown),
            Err(StorageConnectorOptionsValidationError::UnknownField(field))
                if field == "undeclared"
        ));

        let mut missing = valid.clone();
        missing.remove("path");
        assert!(matches!(
            normalize_storage_connector_action_input(&descriptor, &missing),
            Err(StorageConnectorOptionsValidationError::MissingRequiredField(field))
                if field == "path"
        ));

        let mut invalid_option = valid.clone();
        invalid_option.insert("mode".to_string(), serde_json::json!("delete"));
        assert!(matches!(
            normalize_storage_connector_action_input(&descriptor, &invalid_option),
            Err(StorageConnectorOptionsValidationError::InvalidOption { field, value })
                if field == "mode" && value == "delete"
        ));

        let mut nested = valid;
        nested.insert("path".to_string(), serde_json::json!({ "nested": true }));
        assert!(matches!(
            normalize_storage_connector_action_input(&descriptor, &nested),
            Err(StorageConnectorOptionsValidationError::TypeMismatch { field, .. })
                if field == "path"
        ));
    }

    #[test]
    fn custom_action_invocation_resolves_endpoint_and_normalizes_values() {
        let mut connector = s3_descriptor();
        connector.actions = vec![plugin_action_descriptor()];
        let input = BTreeMap::from([
            ("path".to_string(), serde_json::json!("  /uploads/file  ")),
            ("mode".to_string(), serde_json::json!("check")),
            ("token".to_string(), serde_json::json!("secret-value")),
        ]);

        let normalized = normalize_storage_connector_custom_action_invocation(
            &connector,
            &StorageConnectorActionId::declared("plugin.validate_path"),
            StorageConnectorActionEndpoint::ExecuteDraftStoragePolicyAction,
            &input,
        )
        .expect("declared custom action endpoint should resolve");

        assert_eq!(normalized["path"], "/uploads/file");
        assert_eq!(normalized["force"], false);
    }

    #[test]
    fn custom_action_invocation_rejects_lookup_and_input_boundaries() {
        let mut connector = s3_descriptor();
        connector.actions = vec![plugin_action_descriptor()];

        for (action_id, endpoint) in [
            (
                "plugin.missing",
                StorageConnectorActionEndpoint::ExecuteDraftStoragePolicyAction,
            ),
            (
                "plugin.validate_path",
                StorageConnectorActionEndpoint::ExecuteSavedStoragePolicyAction,
            ),
        ] {
            assert!(matches!(
                normalize_storage_connector_custom_action_invocation(
                    &connector,
                    &StorageConnectorActionId::declared(action_id),
                    endpoint,
                    &BTreeMap::new(),
                ),
                Err(StorageConnectorActionInvocationError::Unsupported { .. })
            ));
        }

        for input in [
            BTreeMap::new(),
            BTreeMap::from([
                ("path".to_string(), serde_json::json!("/uploads/file")),
                ("mode".to_string(), serde_json::json!("check")),
                ("token".to_string(), serde_json::json!("secret-value")),
                ("undeclared".to_string(), serde_json::json!(true)),
            ]),
            BTreeMap::from([
                ("path".to_string(), serde_json::json!({ "nested": true })),
                ("mode".to_string(), serde_json::json!("check")),
                ("token".to_string(), serde_json::json!("secret-value")),
            ]),
        ] {
            assert!(matches!(
                normalize_storage_connector_custom_action_invocation(
                    &connector,
                    &StorageConnectorActionId::declared("plugin.validate_path"),
                    StorageConnectorActionEndpoint::ExecuteDraftStoragePolicyAction,
                    &input,
                ),
                Err(StorageConnectorActionInvocationError::InvalidInput(_))
            ));
        }
    }

    #[test]
    fn credential_conditions_can_reference_only_explicit_non_secret_config() {
        let mut descriptor = s3_descriptor();
        let mode = storage_connector_field(
            "delivery",
            StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Boolean,
            false,
            false,
        );
        let mut key = storage_connector_field(
            "cdn_key",
            StorageConnectorFieldScope::StaticCredential,
            StorageConnectorFieldKind::Secret,
            false,
            true,
        );
        key.visible_when = vec![StorageConnectorFieldCondition {
            negate: false,
            scope: Some(StorageConnectorFieldScope::ConnectorConfig),
            field: "delivery".into(),
            value: StorageConnectorFieldDefaultValue::Boolean(true),
        }];
        key.required_when = key.visible_when.clone();
        descriptor.fields = vec![mode.clone(), key.clone()];
        descriptor.validate().unwrap();
        let mut implicit = descriptor.clone();
        implicit.fields[1].visible_when[0].scope = None;
        assert!(
            implicit
                .validate()
                .unwrap_err()
                .to_string()
                .contains("undeclared field")
        );
        let mut secret_dependency = descriptor.clone();
        secret_dependency.fields[0].secret = true;
        assert!(
            secret_dependency
                .validate()
                .unwrap_err()
                .to_string()
                .contains("secret")
        );
        let mut cross_default = descriptor.clone();
        cross_default.fields[1].default_rules = vec![StorageConnectorFieldDefaultRule {
            conditions: key.visible_when.clone(),
            value: StorageConnectorFieldDefaultValue::String("do-not-derive-secrets".into()),
        }];
        assert!(cross_default.validate().is_err());
        let mut reversed = descriptor;
        reversed.fields[0].visible_when = vec![StorageConnectorFieldCondition {
            negate: false,
            scope: Some(StorageConnectorFieldScope::StaticCredential),
            field: "cdn_key".into(),
            value: StorageConnectorFieldDefaultValue::String("secret".into()),
        }];
        assert!(reversed.validate().is_err());
    }
}
