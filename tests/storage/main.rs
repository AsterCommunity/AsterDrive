//! Storage driver, policy, migration, and remote-node integration tests.

#[macro_use]
#[path = "../common/mod.rs"]
mod common;

mod azure_blob;
mod local_driver_security;
mod policies;
mod qiniu;
mod remote_enrollment;
mod remote_storage;
mod s3;
mod sftp;
mod storage_migration;
mod storage_multipart;
