//! Storage-policy recovery preflight built on the reusable read-only object probe.

use aster_drive_model::entities::{file_blob, storage_policy};
use aster_drive_model::types::file_blob::FileBlobBacking;
use aster_drive_storage::StorageErrorKind;
use aster_forge_crypto::sha256_hex;
use aster_forge_utils::numbers::i64_to_u64;
use futures::{StreamExt, stream};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::db::repository::{file_repo, policy_repo};
use crate::errors::{AsterError, Result};
use crate::runtime::StorageConnectorRuntimeState;
use crate::storage::read_probe::{
    StorageReadProbeOptions, StorageReadProbeResult, StorageReadProbeStatus,
    StorageReadProbeTarget, probe_storage_object_readability,
};

const POLICY_RECOVERY_SAMPLE_LIMIT: u64 = 8;
const POLICY_RECOVERY_PROBE_CONCURRENCY: usize = 4;

/// Aggregate recoverability inferred from a bounded, deterministic object sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StoragePolicyRecoverabilityStatus {
    /// Every sampled stored object was readable and matched its recorded size.
    Recoverable,
    /// Some sampled objects were readable while others were missing or blocked.
    PartiallyRecoverable,
    /// No sampled object was readable and at least one conclusive failure was observed.
    Blocked,
    /// Temporary or unclassified failures prevented a reliable conclusion.
    Indeterminate,
    /// The policy has no stored objects; virtual-empty blobs need no connector access.
    NoStoredObjects,
}

impl StoragePolicyRecoverabilityStatus {
    /// Returns whether starting a recovery migration can preserve known content.
    pub const fn can_start_recovery(self) -> bool {
        matches!(
            self,
            Self::Recoverable | Self::PartiallyRecoverable | Self::NoStoredObjects
        )
    }
}

/// Secret-free result for one stored blob sampled during policy recovery preflight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyRecoveryProbeSample {
    /// Sampled blob ID used to correlate the result with admin blob observability.
    pub blob_id: i64,
    /// Stable object probe status.
    pub status: StorageReadProbeStatus,
    /// Structured storage failure category, when present.
    pub error_kind: Option<StorageErrorKind>,
    /// Whether retrying the sample may reasonably produce a different result.
    pub retryable: bool,
    /// Secret-free administrator diagnostic.
    pub diagnostic: Option<String>,
}

/// Bounded evidence returned before creating a policy recovery migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyRecoveryProbe {
    /// Source policy selected by the administrator.
    pub policy_id: i64,
    /// Source policy revision that must still match when a recovery task is created.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub policy_updated_at: chrono::DateTime<chrono::Utc>,
    /// Aggregate recoverability classification.
    pub status: StoragePolicyRecoverabilityStatus,
    /// Whether the current evidence permits a recovery migration to start.
    pub can_start_recovery: bool,
    /// Number of stored blob rows under the source policy.
    pub stored_blob_count: i64,
    /// Logical bytes represented by stored blob rows.
    pub stored_total_bytes: i64,
    /// Number of virtual-empty blob rows that need only metadata migration.
    pub virtual_empty_blob_count: i64,
    /// Deterministic sample results, bounded by the service sample limit.
    pub samples: Vec<StoragePolicyRecoveryProbeSample>,
    /// Digest binding task creation to the policy revision, counts, and sample evidence.
    pub plan_hash: String,
}

/// Probes a policy for readable recovery candidates without mutating its connector.
///
/// The policy and blob summaries are read from the writer connection. Stored blobs
/// are sampled from both ends of the ID range, probed with bounded concurrency, and
/// aggregated conservatively. A successful sample is evidence to start a task, not
/// proof that every object in the policy exists.
pub async fn probe_policy_recoverability(
    state: &(impl StorageConnectorRuntimeState + Sync),
    policy_id: i64,
) -> Result<StoragePolicyRecoveryProbe> {
    let policy = policy_repo::find_by_id(state.writer_db(), policy_id).await?;
    let stored = file_repo::summarize_blobs_by_policy_and_backing(
        state.writer_db(),
        policy_id,
        FileBlobBacking::Stored,
    )
    .await?;
    let virtual_empty = file_repo::summarize_blobs_by_policy_and_backing(
        state.writer_db(),
        policy_id,
        FileBlobBacking::VirtualEmpty,
    )
    .await?;

    let samples = if stored.count == 0 {
        Vec::new()
    } else {
        let blobs = file_repo::find_stored_blob_probe_sample_by_policy(
            state.writer_db(),
            policy_id,
            POLICY_RECOVERY_SAMPLE_LIMIT,
        )
        .await?;
        match state.driver_registry().get_driver(&policy) {
            Ok(driver) => stream::iter(blobs)
                .map(|blob| {
                    let driver = driver.clone();
                    async move { probe_blob(driver.as_ref(), blob).await }
                })
                .buffered(POLICY_RECOVERY_PROBE_CONCURRENCY)
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .collect::<Result<Vec<_>>>()?,
            Err(error) => vec![sample_from_driver_resolution_error(
                blobs.first().map(|blob| blob.id).unwrap_or_default(),
                error,
            )],
        }
    };
    build_policy_recovery_probe(policy, stored, virtual_empty.count, samples)
}

/// Converts policy-level driver construction failure into user-visible probe evidence.
fn sample_from_driver_resolution_error(
    blob_id: i64,
    error: AsterError,
) -> StoragePolicyRecoveryProbeSample {
    let kind = error
        .storage_error_kind()
        .unwrap_or(StorageErrorKind::Unknown);
    let status = match kind {
        StorageErrorKind::Auth
        | StorageErrorKind::Misconfigured
        | StorageErrorKind::NotFound
        | StorageErrorKind::Permission
        | StorageErrorKind::Precondition => StorageReadProbeStatus::Blocked,
        StorageErrorKind::RateLimited
        | StorageErrorKind::Transient
        | StorageErrorKind::Unsupported
        | StorageErrorKind::Unknown => StorageReadProbeStatus::Indeterminate,
    };
    StoragePolicyRecoveryProbeSample {
        blob_id,
        status,
        error_kind: Some(kind),
        retryable: matches!(
            kind,
            StorageErrorKind::RateLimited | StorageErrorKind::Transient
        ),
        diagnostic: Some(crate::errors::sanitize_storage_driver_client_message(
            &error.to_string(),
        )),
    }
}

/// Probes one persisted stored blob after validating its backing and object path.
async fn probe_blob(
    driver: &dyn aster_drive_storage::StorageDriver,
    blob: file_blob::Model,
) -> Result<StoragePolicyRecoveryProbeSample> {
    if blob.backing != FileBlobBacking::Stored {
        return Err(AsterError::internal_error(format!(
            "recovery probe received non-stored blob #{}",
            blob.id
        )));
    }
    let path = blob.storage_path_for_connector().ok_or_else(|| {
        AsterError::internal_error(format!(
            "stored recovery probe blob #{} is missing storage_path",
            blob.id
        ))
    })?;
    let expected_size = i64_to_u64(blob.size, "recovery probe blob size")?;
    let result = probe_storage_object_readability(
        driver,
        StorageReadProbeTarget {
            path,
            expected_size,
        },
        StorageReadProbeOptions::default(),
    )
    .await;
    Ok(sample_from_result(blob.id, result))
}

/// Removes connector-specific details while preserving the structured probe outcome.
fn sample_from_result(
    blob_id: i64,
    result: StorageReadProbeResult,
) -> StoragePolicyRecoveryProbeSample {
    StoragePolicyRecoveryProbeSample {
        blob_id,
        status: result.status,
        error_kind: result.error_kind,
        retryable: result.retryable,
        diagnostic: result.diagnostic,
    }
}

/// Aggregates bounded sample evidence and binds it to a stable recovery plan hash.
fn build_policy_recovery_probe(
    policy: storage_policy::Model,
    stored: file_repo::StoragePolicyBlobSummary,
    virtual_empty_blob_count: i64,
    samples: Vec<StoragePolicyRecoveryProbeSample>,
) -> Result<StoragePolicyRecoveryProbe> {
    let status = aggregate_recoverability(stored.count, &samples);
    #[derive(Serialize)]
    struct PlanIdentity<'a> {
        policy_id: i64,
        policy_updated_at: chrono::DateTime<chrono::Utc>,
        stored_blob_count: i64,
        stored_total_bytes: i64,
        virtual_empty_blob_count: i64,
        samples: &'a [StoragePolicyRecoveryProbeSample],
    }
    let identity = PlanIdentity {
        policy_id: policy.id,
        policy_updated_at: policy.updated_at,
        stored_blob_count: stored.count,
        stored_total_bytes: stored.total_size,
        virtual_empty_blob_count,
        samples: &samples,
    };
    let encoded = serde_json::to_vec(&identity).map_err(|error| {
        AsterError::internal_error(format!("serialize storage recovery probe: {error}"))
    })?;
    Ok(StoragePolicyRecoveryProbe {
        policy_id: policy.id,
        policy_updated_at: policy.updated_at,
        status,
        can_start_recovery: status.can_start_recovery(),
        stored_blob_count: stored.count,
        stored_total_bytes: stored.total_size,
        virtual_empty_blob_count,
        samples,
        plan_hash: sha256_hex(&encoded),
    })
}

/// Conservatively combines object samples without treating temporary failures as loss.
fn aggregate_recoverability(
    stored_blob_count: i64,
    samples: &[StoragePolicyRecoveryProbeSample],
) -> StoragePolicyRecoverabilityStatus {
    if stored_blob_count == 0 {
        return StoragePolicyRecoverabilityStatus::NoStoredObjects;
    }
    let readable = samples
        .iter()
        .filter(|sample| sample.status == StorageReadProbeStatus::Readable)
        .count();
    let conclusive_failure = samples.iter().any(|sample| {
        matches!(
            sample.status,
            StorageReadProbeStatus::Missing | StorageReadProbeStatus::Blocked
        )
    });
    let indeterminate = samples
        .iter()
        .any(|sample| sample.status == StorageReadProbeStatus::Indeterminate);
    if readable == samples.len() && !samples.is_empty() {
        StoragePolicyRecoverabilityStatus::Recoverable
    } else if readable > 0 {
        StoragePolicyRecoverabilityStatus::PartiallyRecoverable
    } else if indeterminate {
        StoragePolicyRecoverabilityStatus::Indeterminate
    } else if conclusive_failure {
        StoragePolicyRecoverabilityStatus::Blocked
    } else {
        StoragePolicyRecoverabilityStatus::Indeterminate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a sample with the requested status for aggregation boundary tests.
    fn sample(status: StorageReadProbeStatus) -> StoragePolicyRecoveryProbeSample {
        StoragePolicyRecoveryProbeSample {
            blob_id: 1,
            status,
            error_kind: None,
            retryable: false,
            diagnostic: None,
        }
    }

    #[test]
    fn aggregate_covers_empty_complete_partial_blocked_and_indeterminate_states() {
        assert_eq!(
            aggregate_recoverability(0, &[]),
            StoragePolicyRecoverabilityStatus::NoStoredObjects
        );
        assert_eq!(
            aggregate_recoverability(1, &[sample(StorageReadProbeStatus::Readable)]),
            StoragePolicyRecoverabilityStatus::Recoverable
        );
        assert_eq!(
            aggregate_recoverability(
                2,
                &[
                    sample(StorageReadProbeStatus::Readable),
                    sample(StorageReadProbeStatus::Missing),
                ],
            ),
            StoragePolicyRecoverabilityStatus::PartiallyRecoverable
        );
        assert_eq!(
            aggregate_recoverability(1, &[sample(StorageReadProbeStatus::Blocked)]),
            StoragePolicyRecoverabilityStatus::Blocked
        );
        assert_eq!(
            aggregate_recoverability(1, &[sample(StorageReadProbeStatus::Indeterminate)]),
            StoragePolicyRecoverabilityStatus::Indeterminate
        );
    }

    #[test]
    fn indeterminate_failure_wins_when_no_sample_is_readable() {
        assert_eq!(
            aggregate_recoverability(
                2,
                &[
                    sample(StorageReadProbeStatus::Blocked),
                    sample(StorageReadProbeStatus::Indeterminate),
                ],
            ),
            StoragePolicyRecoverabilityStatus::Indeterminate
        );
    }

    #[test]
    fn driver_resolution_errors_preserve_blocked_and_retryable_boundaries() {
        let blocked = sample_from_driver_resolution_error(
            7,
            crate::errors::storage_driver_error(
                StorageErrorKind::Permission,
                "source access denied",
            ),
        );
        assert_eq!(blocked.status, StorageReadProbeStatus::Blocked);
        assert!(!blocked.retryable);

        let retryable = sample_from_driver_resolution_error(
            8,
            crate::errors::storage_driver_error(StorageErrorKind::Transient, "source timed out"),
        );
        assert_eq!(retryable.status, StorageReadProbeStatus::Indeterminate);
        assert!(retryable.retryable);

        let sanitized = sample_from_driver_resolution_error(
            9,
            crate::errors::storage_driver_error(
                StorageErrorKind::Auth,
                "request https://objects.example.test/?sig=secret failed",
            ),
        );
        assert!(
            !sanitized
                .diagnostic
                .as_deref()
                .unwrap_or_default()
                .contains("sig=secret")
        );
    }
}
