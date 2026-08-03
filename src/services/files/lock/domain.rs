use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use aster_drive_model::entities::{file, folder, resource_lock};
use aster_drive_model::types::{EntityType, LockDepth, LockMode, LockRootKind, LockWorkspaceType};

use crate::errors::{AsterError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LockWorkspace {
    Personal { user_id: i64 },
    Team { team_id: i64 },
}

impl LockWorkspace {
    #[must_use]
    pub const fn persistence_key(self) -> (LockWorkspaceType, i64) {
        match self {
            Self::Personal { user_id } => (LockWorkspaceType::Personal, user_id),
            Self::Team { team_id } => (LockWorkspaceType::Team, team_id),
        }
    }

    pub fn from_file(file: &file::Model) -> Result<Self> {
        match (file.team_id, file.owner_user_id) {
            (Some(team_id), _) => Ok(Self::Team { team_id }),
            (None, Some(user_id)) => Ok(Self::Personal { user_id }),
            (None, None) => Err(AsterError::internal_error(format!(
                "file #{} has no workspace identity",
                file.id
            ))),
        }
    }

    pub fn from_folder(folder: &folder::Model) -> Result<Self> {
        match (folder.team_id, folder.owner_user_id) {
            (Some(team_id), _) => Ok(Self::Team { team_id }),
            (None, Some(user_id)) => Ok(Self::Personal { user_id }),
            (None, None) => Err(AsterError::internal_error(format!(
                "folder #{} has no workspace identity",
                folder.id
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LockRoot {
    WorkspaceRoot,
    Folder { folder_id: i64 },
    File { file_id: i64 },
}

impl LockRoot {
    #[must_use]
    pub const fn persistence_parts(self) -> (LockRootKind, Option<i64>, Option<i64>) {
        match self {
            Self::WorkspaceRoot => (LockRootKind::WorkspaceRoot, None, None),
            Self::Folder { folder_id } => (LockRootKind::Folder, Some(folder_id), None),
            Self::File { file_id } => (LockRootKind::File, None, Some(file_id)),
        }
    }

    #[must_use]
    pub const fn entity(self) -> Option<(EntityType, i64)> {
        match self {
            Self::WorkspaceRoot => None,
            Self::Folder { folder_id } => Some((EntityType::Folder, folder_id)),
            Self::File { file_id } => Some((EntityType::File, file_id)),
        }
    }

    pub fn from_model(lock: &resource_lock::Model) -> Result<Self> {
        match (lock.root_kind, lock.root_folder_id, lock.root_file_id) {
            (LockRootKind::WorkspaceRoot, None, None) => Ok(Self::WorkspaceRoot),
            (LockRootKind::Folder, Some(folder_id), None) => Ok(Self::Folder { folder_id }),
            (LockRootKind::File, None, Some(file_id)) => Ok(Self::File { file_id }),
            _ => Err(AsterError::internal_error(format!(
                "resource lock #{} has an invalid root column combination",
                lock.id
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LockTarget {
    pub workspace: LockWorkspace,
    pub root: LockRoot,
    pub depth: LockDepth,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ResourceLockState {
    Unlocked,
    Direct {
        mode: LockMode,
        #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    },
    Inherited {
        root: LockRootSummary,
        mode: LockMode,
        #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LockRootSummary {
    WorkspaceRoot,
    Folder { folder_id: i64 },
}
