//! File, folder, upload, metadata, and workspace integration tests.

#[macro_use]
#[path = "../common/mod.rs"]
mod common;

mod archive_preview;
mod batch;
mod directory_upload;
mod edit;
mod files;
mod folders;
mod media_metadata;
mod pagination;
mod properties;
mod search;
mod services;
mod tags;
mod team_write_authorization;
mod thumbnail;
mod trash;
mod trash_coordination;
mod upload;

mod activities;

mod activity_contract;

mod activity_performance;
