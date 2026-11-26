use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use aster_drive_model::entities::{file, folder, resource_lock};
use aster_drive_model::types::{EntityType, LockDepth, LockMode, LockRootKind, LockWorkspaceType};

use crate::errors::{AsterError, Result};

/// Workspace a lock lives in: a personal drive or a team workspace.
///
/// Locks never cross this boundary; a lock in one workspace cannot
/// protect or block resources in another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LockWorkspace {
    /// Lock inside the given user's personal drive.
    Personal {
        /// Owning user id.
        user_id: i64,
    },
    /// Lock inside the given team workspace.
    Team {
        /// Owning team id.
        team_id: i64,
    },
}

impl LockWorkspace {
    /// Converts the workspace into the (kind, id) pair stored on lock rows.
    #[must_use]
    pub const fn persistence_key(self) -> (LockWorkspaceType, i64) {
        match self {
            Self::Personal { user_id } => (LockWorkspaceType::Personal, user_id),
            Self::Team { team_id } => (LockWorkspaceType::Team, team_id),
        }
    }

    /// Derives the lock workspace from a file entity.
    ///
    /// Fails when the file has neither a team nor a personal owner; such
    /// rows cannot participate in locking.
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

    /// Derives the lock workspace from a folder entity.
    ///
    /// Fails when the folder has neither a team nor a personal owner.
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

/// The resource a lock is anchored to.
///
/// Workspace-root locks cover every resource in the workspace; folder and
/// file locks cover their subtree or the file itself, up to the lock depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LockRoot {
    /// The whole workspace.
    WorkspaceRoot,
    /// A folder and, depending on depth, its subtree.
    Folder {
        /// Anchoring folder id.
        folder_id: i64,
    },
    /// A single file.
    File {
        /// Anchoring file id.
        file_id: i64,
    },
}

impl LockRoot {
    /// Converts the root into the columns stored on lock rows.
    #[must_use]
    pub const fn persistence_parts(self) -> (LockRootKind, Option<i64>, Option<i64>) {
        match self {
            Self::WorkspaceRoot => (LockRootKind::WorkspaceRoot, None, None),
            Self::Folder { folder_id } => (LockRootKind::Folder, Some(folder_id), None),
            Self::File { file_id } => (LockRootKind::File, None, Some(file_id)),
        }
    }

    /// The concrete entity covered directly by this root; `None` for the
    /// workspace root, which anchors to no single entity.
    #[must_use]
    pub const fn entity(self) -> Option<(EntityType, i64)> {
        match self {
            Self::WorkspaceRoot => None,
            Self::Folder { folder_id } => Some((EntityType::Folder, folder_id)),
            Self::File { file_id } => Some((EntityType::File, file_id)),
        }
    }

    /// Rebuilds the root from a lock row's columns.
    ///
    /// Fails on any invalid column combination, which indicates corrupted
    /// lock data rather than a missing lock.
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

/// Full identity of a lock: where it lives, what it anchors to, and how
/// deep it reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LockTarget {
    /// Workspace the lock belongs to.
    pub workspace: LockWorkspace,
    /// Resource the lock is anchored to.
    pub root: LockRoot,
    /// How far the lock reaches below the root, e.g. single resource or
    /// infinite subtree.
    pub depth: LockDepth,
}

/// Lock state of a single resource as exposed to clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ResourceLockState {
    /// No lock applies to the resource.
    Unlocked,
    /// A lock is anchored on the resource itself.
    Direct {
        /// Lock mode, e.g. shared write protection.
        mode: LockMode,
        /// Expiry instant; `None` means the lock never expires.
        #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    },
    /// The resource is covered by a lock anchored on an ancestor.
    Inherited {
        /// The ancestor the covering lock is anchored on.
        root: LockRootSummary,
        /// Lock mode of the covering lock.
        mode: LockMode,
        /// Expiry instant; `None` means the covering lock never expires.
        #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    },
}

/// Where an inherited lock state's covering lock is anchored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LockRootSummary {
    /// The covering lock is the workspace root lock.
    WorkspaceRoot,
    /// The covering lock is anchored on an ancestor folder.
    Folder {
        /// Anchoring ancestor folder id.
        folder_id: i64,
    },
}
