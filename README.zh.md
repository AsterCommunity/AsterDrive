<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="frontend-panel/public/static/asterdrive/asterdrive-light.svg" />
    <img src="frontend-panel/public/static/asterdrive/asterdrive-dark.svg" alt="AsterDrive" width="320" />
  </picture>
</p>

<p align="center">
  基于 Rust 和 React 构建的轻量自托管云盘。
  <br />
  支持个人 / 团队空间、本地 / S3 / 远程节点存储策略、分享、WebDAV、预览、WOPI、版本历史、回收站、缩略图和大文件上传。
</p>

<p align="center">
  <a href="https://asterdrive.docs.esap.cc/"><img alt="在线文档" src="https://img.shields.io/badge/docs-VitePress-7C3AED?style=for-the-badge&logo=vitepress&logoColor=white"></a>
  <a href="README.md"><img alt="English README" src="https://img.shields.io/badge/README-English-E11D48?style=for-the-badge"></a>
  <a href="docs/guide/getting-started.md"><img alt="快速开始" src="https://img.shields.io/badge/快速开始-guide-2563EB?style=for-the-badge"></a>
  <a href="docs/deployment/ops-cli.md"><img alt="运维 CLI" src="https://img.shields.io/badge/运维-CLI-0EA5E9?style=for-the-badge"></a>
  <a href="developer-docs/architecture.md"><img alt="架构文档" src="https://img.shields.io/badge/架构-总览-0F172A?style=for-the-badge"></a>
  <a href="developer-docs/api/index.md"><img alt="API 文档" src="https://img.shields.io/badge/API-reference-059669?style=for-the-badge"></a>
  <a href="docs/deployment/docker.md"><img alt="Docker 部署" src="https://img.shields.io/badge/docker-deployment-2496ED?style=for-the-badge&logo=docker&logoColor=white"></a>
</p>

## AsterDrive 是什么？

AsterDrive 是一个 MIT 协议的自托管云盘，适合想自己掌控文件、但不想维护一整套重型协作生态的人。它优先做好云盘骨架：上传文件、整理目录、误删恢复、创建分享、挂载 WebDAV 客户端，以及决定文件对象到底落在哪里。

后端使用 Rust，前端使用 React，可以作为单个服务端二进制运行，也可以用 Alpine 容器镜像部署。当前 `v0.1.x` 是早期稳定版本：已经适合个人和小团队自托管使用，但仍在快速迭代。

## 功能亮点

- **默认自托管** - 单服务运行，前端资源内嵌，默认 SQLite，可选 PostgreSQL / MySQL
- **个人和团队工作空间** - 文件、分享、回收站、任务、配额、审计和存储策略组按工作空间隔离
- **灵活存储路由** - 支持本地文件系统、S3 兼容对象存储和另一台 AsterDrive 从节点，并可按用户、团队和文件大小分流
- **适合大文件上传** - 支持普通直传、可恢复分片上传、S3 预签名直传和 S3 分片直传，由策略和文件大小协商决定
- **分享和直链** - 文件 / 文件夹分享支持密码、过期时间、下载次数、公开页面、分享目录继续浏览和单文件直链
- **WebDAV 支持** - 独立 WebDAV 账号、独立密码、根目录限制、数据库锁、自定义属性和小范围 DeltaV 子集
- **预览与编辑** - 常见浏览器可读文件内置预览，文本文件支持 Monaco 编辑器，支持版本历史、缩略图和外部 Office/WOPI 编辑器接入
- **内置运维能力** - 管理后台、运行时配置、存储策略测试、健康检查、审计日志、后台任务、邮件队列、清理任务，以及 `doctor` / 迁移 CLI

## 快速开始

### 从源码运行

```bash
git clone https://github.com/AptS-1547/AsterDrive.git
cd AsterDrive

cd frontend-panel
bun install
bun run build
cd ..

cargo run
```

首次启动时，AsterDrive 会自动：

- 在当前工作目录下生成 `data/config.toml`（如果不存在）
- 使用默认数据库地址时创建 SQLite 数据库
- 执行全部数据库迁移
- 创建默认本地存储策略
- 初始化写入 `system_config` 的内置运行时配置项

默认访问地址：

```text
http://127.0.0.1:3000
```

第一个注册用户会自动成为 `admin`。

正式上线时不要直接暴露 `:3000`。请把站点放在反向代理后面，由代理层统一处理 HTTPS、**页面级** `Content-Security-Policy` 等安全响应头、上传限制，以及 WebDAV / WOPI 透传。不要把整站 CSP 直接改成全站 `sandbox`；脚本能力文件的 inline 沙箱策略会由应用单独处理。

### 使用 Docker 运行

```bash
# 构建镜像
docker build -t asterdrive .

# 运行容器
docker run -d \
  --name asterdrive \
  -p 3000:3000 \
  -e ASTER__SERVER__HOST=0.0.0.0 \
  -e "ASTER__DATABASE__URL=sqlite:///data/asterdrive.db?mode=rwc" \
  -v asterdrive-data:/data \
  asterdrive

# 或使用 Compose
docker compose up -d
```

当前容器镜像为 **Alpine 运行镜像**，默认以非 root 用户运行，并内置基于 `/health/ready` 的健康检查；推荐使用 `/data` 作为持久化卷。

默认 SQLite 搜索现在依赖 `FTS5 + trigram tokenizer`。部署完成后，建议至少跑一次 `./aster_drive doctor`，确认 `SQLite search acceleration` 检查是 `ok`。

完整部署示例见 [`docker-compose.yml`](docker-compose.yml) 和 [`docs/deployment/docker.md`](docs/deployment/docker.md)。

如果你部署完成后想在命令行里做离线检查、批量改系统设置，或者把 SQLite 迁到 PostgreSQL / MySQL，直接看 [`docs/deployment/ops-cli.md`](docs/deployment/ops-cli.md)。

## 核心能力

### 文件管理

- 层级文件夹、目录树导航、列表 / 网格视图和面包屑导航
- 文件上传、文件夹上传、下载、重命名、移动、复制、删除、恢复和永久删除
- 当前工作空间内搜索、多选、批量移动 / 复制 / 删除和打包下载
- 在线压缩、在线解压和后台任务进度跟踪
- 缩略图、浏览器原生预览、压缩包预览和可配置外部预览应用
- 版本历史、版本恢复 / 删除，以及基于 Monaco 的文本编辑和锁感知
- 通过浏览器实时事件刷新当前文件视图

### 工作空间、分享与访问

- 个人空间和团队空间，文件、分享、回收站、任务、配额和审计记录按空间隔离
- 团队成员支持所有者 / 管理员 / 成员角色，支持团队归档 / 恢复和团队策略组绑定
- 文件和文件夹公开分享页 `/s/:token`
- 分享支持密码、过期时间、下载次数限制、访问 / 下载计数和分享管理页
- 分享目录内继续浏览、子文件下载、预览和缩略图访问
- 单文件直链，提供 inline 打开和强制下载两种形式
- 独立密码、根目录限制、数据库锁、自定义属性和 DeltaV 子集支持的 WebDAV 账号

### 存储与传输

- 本地存储、S3 兼容对象存储和远程 AsterDrive 从节点存储策略
- 策略组可按用户、团队和文件大小决定上传路线
- 仅本地策略可选开启基于 SHA-256 + 引用计数的 Blob 去重
- S3 上传 / 下载策略：`relay_stream`、`presigned`，大文件可走 multipart 上传
- 远程节点上传 / 下载策略：`relay_stream`、`presigned`，从节点接收落点可落本地或 S3
- 在所选策略允许时使用流式上传 / 下载，避免全量缓冲

### 认证与用户设置

- HttpOnly Cookie 认证与 Bearer JWT 支持，方便网页和 API 客户端使用
- 第一个用户初始化、公开注册开关、注册激活、密码重置和邮箱改绑确认流程
- 用户资料、头像上传、Gravatar 头像、主题 / 语言 / 时区 / 视图偏好和登录设备管理
- 可选 Passkey / WebAuthn 注册与登录接口

### 运维与管理

- 管理总览、用户管理、团队管理、存储策略、策略组、远程节点、分享、任务、锁、运行时设置和审计日志
- 运行时配置存储在 `system_config`，支持 schema 驱动的管理界面和离线 CLI 操作
- 健康检查接口：`/health`、`/health/ready`，可选 `/health/memory`（`debug_assertions + openapi`）、`/health/metrics`（`metrics` feature）
- 存储策略和远程节点连通性测试
- 后台任务记录覆盖压缩包任务、缩略图生成、邮件派发、清理任务和系统运行任务
- 定期清理上传会话、回收站、锁、审计日志、团队归档、WOPI 会话和孤儿 Blob
- 带 `openapi` feature 的 debug 构建下提供 Swagger UI，并可通过 `cargo test --features openapi --test generate_openapi` 导出静态 OpenAPI

## 文档导航

- [快速开始](docs/guide/getting-started.md)
- [用户指南](docs/guide/user-guide.md)
- [团队与权限](docs/guide/teams-and-permissions.md)
- [分享与公开访问](docs/guide/sharing.md)
- [在线预览与 WOPI](docs/guide/preview-and-wopi.md)
- [存储后端](docs/storage/index.md)
- [远程节点存储](docs/storage/remote-follower.md)
- [Docker 部署](docs/deployment/docker.md)
- [运维 CLI](docs/deployment/ops-cli.md)
- [开发者文档](developer-docs/README.md)
- [架构文档](developer-docs/architecture.md)
- [API 概览](developer-docs/api/index.md)

## 开发

### 环境要求

- Rust `1.91.1+`
- Bun
- Node.js `24+`（当前 Docker 前端构建阶段会用到）

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

- 类型检查使用 `tsgo`，不是 `tsc`
- Lint 使用 `biome`，不是 ESLint
- 禁止 TypeScript `enum`，请使用 `as const` 对象
- 类型导入必须使用 `import type`

## 配置

静态配置加载优先级：

```text
环境变量 > data/config.toml > 内置默认值
```

示例：

```bash
ASTER__SERVER__HOST=0.0.0.0
ASTER__SERVER__PORT=3000
ASTER__DATABASE__URL="postgres://aster:secret@127.0.0.1:5432/asterdrive"
ASTER__WEBDAV__PREFIX="/webdav"
```

运行时配置存储在数据库中，可通过管理 API 或管理后台在线修改。

## 项目结构

```text
src/                    Rust 后端
migration/              Sea-ORM 迁移
frontend-panel/         React 管理 / 文件前端
docs/                   部署与面向最终用户的文档
developer-docs/         面向开发者的 API 与架构文档
tests/                  集成测试
```

## 许可证

[MIT](LICENSE) - Copyright (c) 2026 AptS-1547
