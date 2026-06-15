//! 存储驱动实现。
//!
//! 存放具体存储后端驱动，不参与 trait 定义。

pub mod azure_blob;
pub mod local;
pub mod onedrive;
pub mod remote;
pub mod s3;
pub mod s3_compatible;
pub mod s3_config;
pub mod tencent_cos;
