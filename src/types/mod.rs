//! 共享领域类型定义。
//!
//! 子模块公开表达类型来源，`facade` 只维护 `crate::types::{...}` 的稳定兼容入口。
//! 新增类型默认先放在具体子模块；只有跨实体、repo、service 或 API 边界长期共享
//! 的类型，才加入根 facade。

pub mod archive;
pub mod audit;
pub mod auth;
pub mod config;
pub mod entity;
pub mod external_auth_provider;
mod facade;
pub mod file_category;
pub mod mail;
pub mod media_metadata;
pub mod passkey;
pub mod preferences;
pub mod storage_credential;
pub mod storage_policy;
pub mod tag;
pub mod task;
pub mod team;
pub mod user;
pub mod user_invitation;

pub use facade::*;
