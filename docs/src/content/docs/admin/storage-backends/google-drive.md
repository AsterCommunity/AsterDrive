---
title: Google Drive 存储策略教程
description: 配置管理员托管的 Google Drive blob 存储、OAuth 授权、共享盘和服务端分片上传。
---

Google Drive 存储策略由管理员授权一个 Google 账号，将指定目录、共享盘或应用数据目录作为 AsterDrive 的底层存储池。普通用户仍使用 AsterDrive 的文件、权限、版本、配额和分享功能，无需授权自己的 Google 账号。这项功能不提供用户 Google Drive 导入、浏览或同步。

## 准备 Google OAuth 应用

1. 在 Google Cloud 项目中启用 Google Drive API。
2. 配置 OAuth 同意屏幕，创建类型为 **Web application** 的 OAuth 客户端。
3. 在 AsterDrive 管理后台创建 Google Drive 存储策略，保存客户端 ID 和客户端密钥。
4. 将策略授权面板显示的完整 OAuth 回调地址加入 Google 客户端的 **Authorized redirect URIs**。公开站点地址应使用可访问的 HTTPS 地址，并与回调地址一致。
5. 测试阶段将管理员账号加入 OAuth 测试用户。长期使用前完成 Google 要求的发布与验证；外部应用处于 Testing 状态时，Drive refresh token 通常只有七天有效期。

普通目录和共享盘需要 `https://www.googleapis.com/auth/drive`，应用数据目录需要 `https://www.googleapis.com/auth/drive.appdata`。普通目录使用完整 Drive scope，管理员应了解授权范围；AsterDrive 的对象查询和删除仍限制在指定存储池和应用标记内。

## 选择存储池

- **根目录 ID**：填写 Drive 目录 URL 中的目录 ID；留空使用账号根目录。建议建立专用目录。
- **共享盘 ID**：使用 Google Workspace 共享盘时填写。根目录 ID 留空时使用共享盘根目录；填写时必须属于该共享盘，管理员需要添加和删除文件的权限。
- **使用 appDataFolder**：使用隐藏的应用数据目录；不能同时填写根目录 ID 或共享盘 ID。这些对象不能作为普通 Drive 文件浏览或分享。

已有对象后不要随意切换存储池、OAuth 应用或账号；逻辑对象键在原存储池和原应用下解析，更改后旧数据可能无法访问。需要迁移时使用 AsterDrive 的存储迁移流程。

## 授权、验证和使用

保存策略后点击授权，以管理员账号完成 Google OAuth 同意。授权采用一次性 state 和 PKCE；离线 refresh token 与客户端密钥保存在后端加密凭据库中。回到后台后检查授权状态，运行凭据验证和连接测试。连接测试沿用 AsterDrive 的临时对象写入和删除流程。

将策略加入[策略组并绑定用户或团队](/admin/storage-policies/)，再执行上传、预览、下载和删除。大文件使用服务端中继和 Google resumable upload，每次最多缓存 8 MiB 分片；上传会话地址及管理员 token 都不会下发浏览器。下载与 Range 预览经过 AsterDrive 服务端，不提供 Google 浏览器直链。

AsterDrive 保持逻辑对象键，并将键写入 Google 文件的 `appProperties.asterdrive_storage_path`。读写和删除只查询存储池内带有该应用标记的文件；遇到相同键的多个对象时拒绝操作，避免误删。不要手动移动或修改这些文件的应用属性。

## 常见故障

- **需要重新授权**：Google token 已过期、被撤销、缺少离线 refresh token，或 OAuth 应用仍处于 Testing 状态。重新授权并检查应用发布状态。
- **权限不足**：检查目录或共享盘权限，以及同意屏幕授予的 Drive scope。
- **配置无效**：填写目录 ID 而非完整 URL；检查共享盘 ID 与目录的归属。
- **API 限流或暂时不可用**：稍后重试，检查 Google 配额、网络代理和服务端出站连接。
- **对象不存在**：检查文件是否被人工移出存储池或删除。恢复原目录和 OAuth 应用后重试，不要直接修改 AsterDrive 数据库中的 blob 键。

Google Drive 的存储容量、API 配额和组织策略仍然生效。AsterDrive 保持自己的配额与 blob 引用计数；实际远端删除发生在引用释放后的回收流程中。
