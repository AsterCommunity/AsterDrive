use clap::{Args, ValueEnum};
use serde::Serialize;

#[derive(Debug, Clone, Args)]
pub struct DoctorArgs {
    #[arg(long, env = "ASTER_CLI_DATABASE_URL")]
    pub database_url: Option<String>,
    #[arg(long, env = "ASTER_CLI_DOCTOR_STRICT", default_value_t = false)]
    pub strict: bool,
    #[arg(long, env = "ASTER_CLI_DOCTOR_DEEP", default_value_t = false)]
    pub deep: bool,
    #[arg(long, env = "ASTER_CLI_DOCTOR_FIX", default_value_t = false)]
    pub fix: bool,
    #[arg(long, env = "ASTER_CLI_DOCTOR_DRY_RUN", default_value_t = false)]
    pub dry_run: bool,
    #[arg(long, env = "ASTER_CLI_YES", default_value_t = false)]
    pub yes: bool,
    #[arg(
        long = "scope",
        env = "ASTER_CLI_DOCTOR_SCOPE",
        value_enum,
        value_delimiter = ','
    )]
    pub scopes: Vec<DoctorDeepScope>,
    #[arg(long, env = "ASTER_CLI_DOCTOR_POLICY_ID")]
    pub policy_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum DoctorDeepScope {
    StorageUsage,
    BlobRefCounts,
    StorageObjects,
    FolderTree,
    RevisionLedger,
}

impl DoctorDeepScope {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::StorageUsage => "storage_usage",
            Self::BlobRefCounts => "blob_ref_counts",
            Self::StorageObjects => "storage_objects",
            Self::FolderTree => "folder_tree",
            Self::RevisionLedger => "revision_ledger",
        }
    }
}

pub(super) fn effective_deep_scopes(args: &DoctorArgs) -> Vec<DoctorDeepScope> {
    if args.scopes.is_empty() {
        return vec![
            DoctorDeepScope::StorageUsage,
            DoctorDeepScope::BlobRefCounts,
            DoctorDeepScope::StorageObjects,
            DoctorDeepScope::FolderTree,
            DoctorDeepScope::RevisionLedger,
        ];
    }

    let mut deduped = Vec::new();
    for scope in &args.scopes {
        if !deduped.contains(scope) {
            deduped.push(*scope);
        }
    }
    deduped
}
