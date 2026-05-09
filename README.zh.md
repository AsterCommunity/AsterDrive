<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="frontend-panel/public/static/asterdrive/asterdrive-light.svg" />
    <img src="frontend-panel/public/static/asterdrive/asterdrive-dark.svg" alt="AsterDrive" width="320" />
  </picture>
</p>

<p align="center">
  基于 Rust 和 React 构建的自托管云存储系统。
  <br />
  支持单二进制交付、Alpine 容器部署、存储策略、WebDAV、分享、版本历史、回收站、缩略图，以及四种上传模式。
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

## 功能亮点

- **单二进制交付** - 前端资源通过 `rust-embed` 嵌入 Rust 服务端，无需额外 Web 服务器
- **多数据库支持** - 默认 SQLite，也支持 MySQL 和 PostgreSQL，统一通过 SeaORM 接入
- **可插拔存储策略** - 支持本地文件系统和 S3 兼容对象存储，并支持用户级、文件夹级覆盖
- **四种上传模式** - `direct`、`chunked`、`presigned`、`presigned_multipart`，由存储策略和文件大小协商决定
- **分享能力** - 支持文件和文件夹分享，支持密码、过期时间、下载次数限制、公开分享页、分享目录继续浏览、子文件下载和分享缩略图
- **WebDAV 支持** - 独立 WebDAV 账号、访问根目录限制、数据库锁、自定义属性，以及最小 DeltaV 版本树支持
- **生命周期管理** - 内置回收站、版本历史、缩略图、资源锁、周期清理任务、Blob 对账和运行时配置管理
- **管理后台** - 前端面板内置总览页，可管理用户、存储策略、运行时配置、分享、锁、WebDAV 账号和审计日志

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

- 层级文件夹
- 文件上传、下载、重命名、移动、复制、删除
- 支持通过 `relative_path` 的目录上传与自动建目录
- 内联搜索与批量操作
- 缩略图与文件预览
- 版本历史、版本恢复与基于 Monaco 的文本编辑、锁感知

### 存储与传输

- 仅本地策略可选开启的基于 SHA-256 + 引用计数的 Blob 去重
- 本地存储与 S3 兼容存储策略
- 用户默认策略与文件夹策略覆盖
- S3 上传策略：`relay_stream`、`presigned`
- 流式上传 / 下载，避免全量缓冲

### 协作与访问

- HttpOnly Cookie 认证与 Bearer JWT 支持
- 公开分享页 `/s/:token`
- 支持密码保护和过期控制的分享链接
- 支持在分享目录树内继续浏览、下载子文件和读取子文件缩略图
- 用户资料、头像上传 / Gravatar 与偏好设置接口
- 独立密码、根目录限制和 DeltaV 子集支持的 WebDAV 账号

### 运维能力

- 健康检查接口：`/health`、`/health/ready`，可选 `/health/memory`（`debug_assertions + openapi`）、`/health/metrics`（`metrics` feature）
- 存储在 `system_config` 中的运行时配置
- 管理总览、配置 schema 和存储策略连通性测试接口
- 关键操作审计日志
- 带 `openapi` feature 的 debug 构建下提供 Swagger UI，并可通过 `cargo test --features openapi --test generate_openapi` 导出静态 OpenAPI
- 每 5 秒派发邮件和后台任务、每小时跑一次维护清理、每 6 小时做一次 Blob 对账

## 文档导航

- [快速开始](docs/guide/getting-started.md)
- [安装与部署](docs/guide/installation.md)
- [运维 CLI](docs/deployment/ops-cli.md)
- [性能基准与压测](docs/deployment/performance-benchmarking.md)
- [社区行为准则](CODE_OF_CONDUCT.md)
- [开发者文档](developer-docs/README.md)
- [架构文档](developer-docs/architecture.md)
- [Docker 部署](docs/deployment/docker.md)
- [API 概览](developer-docs/api/index.md)
- [用户指南](docs/guide/user-guide.md)

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
