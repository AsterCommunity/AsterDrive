//! Unified Data Transfer Object (DTO) definitions for the API layer.
//!
//! This module consolidates all request and response structs that were previously
//! scattered across individual route files. Each submodule corresponds to a
//! functional domain.
//!
//! # Organization
//!
//! - `auth`     — Authentication and session management (login, register, password reset, etc.)
//! - `batch`    — Batch file/folder operations (delete, move, copy, archive)
//! - `files`    — File CRUD, upload, and access (WOPI, versions)
//! - `folders`  — Folder management (create, patch, copy, lock)
//! - `shares`   — Share creation and management
//! - `teams`    — Team management and membership
//! - `wopi`     — WOPI protocol structs
//! - `admin`    — Admin-only operations (users, policies, config, etc.)
//! - `webdav`   — WebDAV account management
//! - `properties` — Entity custom properties
//! - `share_public` — Public share endpoints

pub mod admin;
pub mod auth;
pub mod batch;
pub mod files;
pub mod folders;
pub mod properties;
pub mod share_public;
pub mod shares;
pub mod tags;
pub mod teams;
pub mod trash;
pub(crate) mod validation;
pub mod webdav;
pub mod wopi;

// Re-export commonly used types for convenience
pub use admin::{AdminCreateTeamReq, AdminListQuery, AdminPatchTeamReq};
pub use auth::{
    AcceptUserInvitationReq, ActionMessageResp, AuthTokenResp, ChangePasswordReq, CheckResp,
    ContactVerificationConfirmQuery, LoginReq, MeQuery, PasswordResetConfirmReq,
    PasswordResetRequestReq, RegisterReq, RequestEmailChangeReq, ResendRegisterActivationReq,
    SetupReq, UpdateAvatarSourceReq, UpdateProfileReq,
};
pub use batch::{
    ArchiveCompressReq, ArchiveDownloadReq, BatchCopyReq, BatchDeleteReq, BatchMoveReq,
};
pub use files::{
    ChunkPath, CompleteUploadReq, CompletedPartReq, CopyFileReq, CreateEmptyRequest,
    ExtractArchiveRequest, FileQuery, InitUploadReq, OpenWopiRequest, PatchFileReq,
    PresignPartsReq, SetLockReq as FileSetLockReq, UploadIdPath, VersionPath,
};
pub use folders::{CopyFolderReq, CreateFolderReq, PatchFolderReq, SetLockReq as FolderSetLockReq};
pub use properties::{EntityPath, PropPath, SetPropReq};
pub use share_public::{DirectLinkQuery, VerifyPasswordReq};
pub use shares::{BatchDeleteSharesReq, CreateShareReq, UpdateShareReq};
pub use tags::{
    BatchTagBindingReq, CreateTagReq, DEFAULT_TAG_LIMIT, EntityTagsPath, PatchTagReq,
    ReplaceEntityTagsReq, TagEntityPath, TagListQuery, TagPath,
};
pub use teams::{
    AddTeamMemberReq, CreateTeamReq, ListTeamMembersQuery, ListTeamsQuery, PatchTeamMemberReq,
    PatchTeamReq,
};
pub use trash::TrashItemPath;
pub(crate) use validation::validate_request;
pub use webdav::{CreateWebdavAccountReq, TestConnectionReq, WebdavSettingsInfo};
pub use wopi::WopiAccessQuery;
