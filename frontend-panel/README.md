# AsterDrive Frontend Panel

前端管理面板基于 React 19、Vite、React Router 和 Zustand，构建产物会被后端嵌入，也可以在运行时通过工作目录下的 `./frontend-panel/dist` 覆盖嵌入资源。

## 开发命令

```bash
bun install
bun run dev
```

默认开发配置：

- 前端地址：Vite 默认端口
- API 基地址：`/api/v1`
- 可通过 `VITE_API_BASE_URL` 覆盖
- 视频自定义浏览器可通过以下环境变量启用：
  - `VITE_VIDEO_BROWSER_URL_TEMPLATE`
  - `VITE_VIDEO_BROWSER_LABEL`
  - `VITE_VIDEO_BROWSER_MODE=iframe|new_tab`
  - `VITE_VIDEO_BROWSER_ALLOWED_ORIGINS`

## 构建

```bash
bun run typecheck
bun run build
```

构建命令会先执行 TypeScript 7 原生 `tsc` 增量类型检查，再执行 `vite build`。

## 代码生成

前端依赖静态 OpenAPI 规范生成类型：

```bash
cargo test --features openapi --test generate_openapi
cd frontend-panel
bun run generate-api
```

## 质量检查

```bash
bun run check
bun run check:fix
bun run format
```

## 当前页面

| 路由 | 作用 |
| --- | --- |
| `/login` | 登录页 |
| `/` | 文件浏览器 |
| `/trash` | 回收站 |
| `/settings/webdav` | WebDAV 账号管理 |
| `/s/:token` | 公开分享页 |
| `/admin/users` | 用户与用户策略管理 |
| `/admin/policies` | 存储策略管理 |
| `/admin/shares` | 全站分享管理 |
| `/admin/locks` | 锁管理 |
| `/admin/settings` | 运行时配置管理 |

## 已接入的核心能力

- 文件浏览、文件夹树、文件预览
- 直传、分片上传、S3 预签名上传
- 分享创建与公开访问
- 版本历史查看与恢复
- 回收站恢复与清空
- WebDAV 账号创建、启停、测试
- 管理员用户、策略、锁、系统设置
