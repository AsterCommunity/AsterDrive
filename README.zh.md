<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="frontend-panel/public/static/asterdrive/asterdrive-light.svg" />
    <img src="frontend-panel/public/static/asterdrive/asterdrive-dark.svg" alt="AsterDrive" width="320" />
  </picture>
</p>

<p align="center">
  面向小团队的 Rust 自托管文件基础设施：在不引入完整私有云套件的前提下，提供存储控制、可靠大文件上传、WebDAV/WOPI 和运维可观测性。
  <br />
  用一个 MIT 协议的 Rust + React 服务，把文件路由到本地、S3 兼容存储或远程节点，并保留清晰的部署、审计和二开边界。
</p>

<p align="center">
  <a href="https://drive.astercosm.com/"><img alt="在线文档" src="https://img.shields.io/badge/docs-Astro_Starlight-7C3AED?style=for-the-badge&logo=astro&logoColor=white"></a>
  <a href="README.md"><img alt="English README" src="https://img.shields.io/badge/README-English-E11D48?style=for-the-badge"></a>
  <a href="https://drive.astercosm.com/guide/getting-started/"><img alt="快速开始" src="https://img.shields.io/badge/快速开始-guide-2563EB?style=for-the-badge"></a>
  <a href="https://drive.astercosm.com/deployment/ops-cli/"><img alt="运维 CLI" src="https://img.shields.io/badge/运维-CLI-0EA5E9?style=for-the-badge"></a>
  <a href="developer-docs/zh-CN/architecture.md"><img alt="架构文档" src="https://img.shields.io/badge/架构-总览-0F172A?style=for-the-badge"></a>
  <a href="developer-docs/zh-CN/api/index.md"><img alt="API 文档" src="https://img.shields.io/badge/API-reference-059669?style=for-the-badge"></a>
  <a href="https://drive.astercosm.com/deployment/docker/"><img alt="Docker 部署" src="https://img.shields.io/badge/docker-deployment-2496ED?style=for-the-badge&logo=docker&logoColor=white"></a>
</p>

<p align="center">
  <img src="assets/Readme/Screenshot-Chinese.webp" alt="AsterDrive 中文截图" width="1280" />
</p>

## AsterDrive 是什么？

AsterDrive 是一个 MIT 协议的自托管文件服务，适合想掌控文件存放位置、传输路径和运维边界的人。它围绕云盘最核心的工作流设计：可靠上传、整理文件夹、误删恢复、分享访问、接入 WebDAV 客户端、通过 WOPI 兼容服务打开 Office 文件，并把对象路由到合适的存储后端。

相比起实现完整的私有云套件，AsterDrive 更关注文件基础设施：存储策略、大文件上传路径、个人和团队空间、分享、版本历史、WebDAV、WOPI、审计能力，以及部署和运维工具。

目前 `v0.4.x` 是活跃开发线。在 `0.x` 阶段，中版本号承载主要兼容性或产品范围变更，小版本号承载较小的功能更新和维护更新。

## 适用场景

AsterDrive 适用于：

- 前端资源内嵌、单服务运行的自托管文件系统
- 默认用 SQLite 起步，后续按需要切到 PostgreSQL / MySQL
- 文件可以落到本地文件系统、S3 兼容对象存储，或是远程的 AsterDrive 从节点
- 小文件和大文件都要有合适上传路径：普通直传、可恢复分片、对象存储预签名直传、对象存储 multipart
- 个人空间和团队空间需要配额、分享、回收站、任务、审计和存储策略组
- 需带独立账号、独立密码和根目录限制的 WebDAV
- 需要通过 OnlyOffice、Collabora 或其他 WOPI 服务预览/编辑 Office 文件
- 希望代码可读、可改、可部署，而不是接入一整套插件市场或企业协作生态

AsterDrive 目前不适用于：

- 需要日历、联系人、聊天、邮件和应用生态的完整协作套件
- 现在就需要成熟的桌面端和移动端同步客户端
- 只是想给服务器上的一个目录套网页管理界面
- 需要多主集群、自动故障切换或企业合规认证
- 想要别人托管一切、自己不承担部署和数据责任的 SaaS

## 设计重点

- **文件安全优先** - 回收站、历史版本、锁、配额检查和清理任务都是核心流程，不是装饰功能。
- **存储可控** - 存储策略可以按用户、团队和文件大小，把上传路由到本地、S3 兼容对象存储或远程从节点。
- **大文件友好** - 后端会根据策略和对象大小协商普通直传、分片上传、对象存储预签名上传和对象存储 multipart 上传。
- **互操作但不膨胀** - WebDAV 和 WOPI 覆盖实际客户端与 Office 工作流，但项目不会因此变成全家桶云套件。
- **运维内建** - 健康检查、运行时配置、审计日志、后台任务、存储测试、`doctor` 和迁移命令都是一等能力。
- **可二开的核心** - Rust 后端、React 前端、SeaORM 迁移、明确错误码、API 文档，以及清楚的 service/repository 边界。

## 快速开始

### 使用 Docker 运行

本地 HTTP 试用时，先准备可写数据目录，再启动官方镜像：

```bash
mkdir -p ./data
sudo chown -R 10001:10001 ./data

docker run -d \
  --name asterdrive \
  -p 3000:3000 \
  -e ASTER__SERVER__HOST=0.0.0.0 \
  -e ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES=true \
  -e "ASTER__DATABASE__URL=sqlite:///data/asterdrive.db?mode=rwc" \
  -v "$(pwd)/data:/data" \
  ghcr.io/astercommunity/asterdrive:latest
```

打开：

```text
http://127.0.0.1:3000
```

首个注册用户会自动成为 `admin`。

`ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES=true` 只适合本地或内网 HTTP 测试。正式环境请放到 HTTPS 后面，并保持安全 Cookie 开启。

也可以直接使用仓库里的 Compose 文件：

```bash
mkdir -p ./data
sudo chown -R 10001:10001 ./data
docker compose up -d
```

完整配置见 [Docker 部署文档](https://drive.astercosm.com/deployment/docker/)。

### 从源码运行

```bash
git clone https://github.com/AsterCommunity/AsterDrive.git
cd AsterDrive

cd frontend-panel
bun install
bun run build
cd ..

cargo run
```

AsterDrive首次启动时会自动：

- 在当前工作目录下生成 `data/config.toml`（如果不存在）
- 使用默认数据库地址时创建 SQLite 数据库
- 执行全部数据库迁移
- 创建默认本地存储策略和默认策略组
- 初始化写入 `system_config` 的内置运行时配置项

## 生产部署提醒

- 切勿直接把 `:3000` 暴露到公网。请放在反向代理后面，由代理处理 HTTPS、上传限制、WebDAV/WOPI 透传和安全响应头。
- 在依赖分享链接、WebDAV 地址、邮件链接或 WOPI 回调之前，先配置公开站点地址。
- 部署和升级后运行 `./aster_drive doctor`。默认 SQLite 搜索加速依赖 `FTS5 + trigram tokenizer` 支持。
- 提前规划数据库、上传对象、配置文件和外部对象存储凭据的备份。先看 [备份与恢复](https://drive.astercosm.com/deployment/backup/)。
- 如果启用了 WOPI，请用最终公开地址测试真实的 `docx`、`xlsx`、`pptx` 文件，并确认编辑能保存回 AsterDrive。

## 核心功能

### 文件管理

- 文件夹、面包屑、列表/网格视图、搜索、多选和批量操作
- 文件上传、文件夹上传、下载、重命名、移动、复制、删除、恢复和永久删除
- 打包下载、在线压缩、在线解压和后台任务进度
- 缩略图、浏览器原生预览、ZIP/7z 压缩包只读清单预览和可配置外部预览应用
- 基于 Monaco 的文本编辑、锁感知、版本历史、版本恢复和版本删除

### 工作空间与分享

- 个人空间和团队空间
- 每个空间独立拥有文件、分享、回收站、任务、配额、审计记录和策略组
- 文件和文件夹公开分享页 `/s/:token`
- 分享支持密码、过期时间、下载次数、访问/下载计数和直链
- 分享目录内继续浏览、子文件下载、预览和缩略图访问

### 访问与编辑

- HttpOnly Cookie 认证，以及方便 API 客户端使用的 Bearer JWT
- 首用户初始化、注册开关、注册激活、密码重置和邮箱改绑确认
- WebDAV 独立账号、独立密码、根目录限制、数据库锁、自定义属性和小范围 DeltaV 子集
- 面向外部 WOPI Host 的启动会话和文件端点，用于 Office 预览/编辑
- 可选 Passkey / WebAuthn 注册与登录接口

### 存储与传输

- 本地存储、S3 兼容存储和远程 AsterDrive 从节点存储策略
- 策略组可按用户、团队和文件大小决定上传路线
- 本地策略可选开启基于 SHA-256 + 引用计数的 Blob 去重
- 对象存储上传/下载策略：`relay_stream`、`presigned` 和 multipart 上传
- 远程节点上传/下载策略：`relay_stream` 和 `presigned`
- 在所选策略允许时使用流式上传/下载路径

### 管理与运维

- 管理总览、用户、团队、存储策略、策略组、远程节点、分享、任务、锁、运行时设置和审计日志
- 存储在 `system_config` 中的 schema 驱动运行时配置
- 健康检查接口：`/health`、`/health/ready`，以及可选 `/health/memory` 和 `/health/metrics`
- 存储策略和远程节点连通性测试
- 后台任务记录覆盖压缩包任务、缩略图生成、邮件派发、清理任务和系统运行任务
- 定期清理上传会话、回收站、锁、审计日志、团队归档、WOPI 会话和孤儿 Blob
- 带 `openapi` feature 的 debug 构建提供 Swagger UI，并支持静态 OpenAPI 导出

## 文档

面向用户和部署者的文档使用 Astro Starlight 构建，并统一发布在 [drive.astercosm.com](https://drive.astercosm.com/)。

- [快速开始](https://drive.astercosm.com/guide/getting-started/)
- [用户指南](https://drive.astercosm.com/guide/user-guide/)
- [团队与权限](https://drive.astercosm.com/guide/teams-and-permissions/)
- [分享与公开访问](https://drive.astercosm.com/guide/sharing/)
- [在线预览与 WOPI](https://drive.astercosm.com/guide/preview-and-wopi/)
- [存储后端](https://drive.astercosm.com/storage/)
- [远程节点存储](https://drive.astercosm.com/storage/remote-follower/)
- [Docker 部署](https://drive.astercosm.com/deployment/docker/)
- [生产检查清单](https://drive.astercosm.com/deployment/production-checklist/)
- [备份与恢复](https://drive.astercosm.com/deployment/backup/)
- [运维 CLI](https://drive.astercosm.com/deployment/ops-cli/)
- [开发者文档](developer-docs/README.md)
- [架构文档](developer-docs/zh-CN/architecture.md)
- [API 概览](developer-docs/zh-CN/api/index.md)

## 安全

- 请通过 [GitHub Security Advisories](https://github.com/AsterCommunity/AsterDrive/security/advisories/new) 或 `report@esaps.net` 私下报告漏洞——完整政策见 [SECURITY.md](SECURITY.md)
- 已发布的安全公告：[github.com/AsterCommunity/AsterDrive/security/advisories](https://github.com/AsterCommunity/AsterDrive/security/advisories)
- `security.txt`（RFC 9116）：[drive.astercosm.com/.well-known/security.txt](https://drive.astercosm.com/.well-known/security.txt)

## 开发

### 环境要求

- Rust `1.95.0+`
- Bun
- Node.js `24+`（用于 Docker 前端构建阶段）

### 常用命令

```bash
# 后端
cargo run
cargo check
cargo test
cargo test --features openapi --test generate_openapi

# 前端
cd frontend-panel
bun install
bun run dev
bun run build
bun run check
```

### 说明

- 类型检查使用 TypeScript 7 原生 `tsc` 编译器
- Lint 使用 `biome`，不是 ESLint
- 禁止 TypeScript `enum`，请使用 `as const` 对象
- 类型导入必须使用 `import type`

## 项目结构

```text
src/                    Rust 后端
migration/              SeaORM 迁移
frontend-panel/         React 管理 / 文件前端
docs/                   部署与面向最终用户的文档
developer-docs/         API、架构、测试和内部定位文档
tests/                  集成测试
```

## 许可证

[MIT](LICENSE) - Copyright (c) 2026 AsterCommunity & AptS-1547
