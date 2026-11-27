//! 共享领域类型定义。
//!
//! 子模块公开表达类型来源，`facade` 只维护 `crate::types::{...}` 的稳定兼容入口。
//! 新增类型默认先放在具体子模块；只有跨实体、repo、service 或 API 边界长期共享
//! 的类型，才加入根 facade。

/// 归档条目文件名解码策略。
pub mod archive;
/// 审计动作、实体类型等审计领域枚举。
pub mod audit;
/// 联系方式验证与 MFA 认证类型。
pub mod auth;
/// 实体类型与资源锁目标类型。
pub mod entity;
/// 外部认证 provider 回调模式与 options 存储。
pub mod external_auth_provider;
mod facade;
/// 文件 blob 物理承载方式。
pub mod file_blob;
/// 文件夹层级深度边界。
pub mod folder;
/// 文件夹展示图标类型。
pub mod folder_icon;
/// 媒体元数据抽取载荷类型。
pub mod media_metadata;
/// WebAuthn passkey 凭据存储包装。
pub mod passkey;
/// 用户偏好、locale 标签与配置存储类型。
pub mod preferences;
/// 资源锁领域原语。
pub mod resource_lock;
/// 文件列表排序偏好。
pub mod sort;
/// 存储凭据 provider 与 Microsoft Graph 云端点类型。
pub mod storage_credential;
/// 存储策略传输策略、远端节点传输与媒体处理器类型。
pub mod storage_policy;
/// 标签作用域。
pub mod tag;
/// 后台任务类型、状态与 JSON 存储包装。
pub mod task;
/// 团队成员角色。
pub mod team;
/// 上传 session 数据面、状态与调度契约。
pub mod upload_session;
/// 用户角色、状态与头像来源。
pub mod user;
/// 用户邀请状态。
pub mod user_invitation;
/// WebDAV account access mode.
pub mod webdav;

pub use facade::*;
