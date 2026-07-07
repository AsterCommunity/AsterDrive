---
description: AsterDrive 身份与访问功能地图，覆盖登录会话、MFA、Passkey、外部认证、WebDAV 账号和公开访问边界。
---

# 身份与访问

身份与访问负责回答三件事：谁在访问、用什么方式证明身份、访问范围到哪里结束。

## 能力边界

| 能力 | 说明 | 相关文档 |
| --- | --- | --- |
| 本地账号登录 | 用户名或邮箱登录、密码校验、会话 Cookie 和访问 token | [登录与会话](/config/auth) |
| 首个管理员初始化 | 新实例没有用户时创建第一个管理员 | [快速开始](/guide/getting-started)、[首次启动检查](/deployment/runtime-behavior) |
| MFA | TOTP、恢复码、邮箱验证码 MFA、登录流程过期和尝试次数限制 | [登录与会话](/config/auth) |
| Passkey | 用户安全设置里的 Passkey 绑定和登录 | [登录与会话](/config/auth) |
| 外部认证 | OIDC、通用 OAuth2、外部身份绑定、邮箱验证和自动创建账号 | [外部认证](/config/external-auth) |
| WebDAV 专用账号 | 用户单独创建 WebDAV 凭证，不复用网页登录密码 | [WebDAV](/config/webdav)、[用户手册](/guide/user-guide) |
| 公开访问 | 分享链接、公开预览、直链、短时效流播放 ticket | [分享与公开访问](/guide/sharing)、[预览与处理](./preview-processing) |

## 后端模块

| 模块 | 负责内容 |
| --- | --- |
| `auth_service` | 注册、密码、会话、邮箱验证和登录主流程 |
| `mfa_service` | MFA 登录流程、TOTP、恢复码、邮箱验证码 |
| `passkey_service` | Passkey 注册、认证和凭证管理 |
| `external_auth_service` | 外部 provider、登录流程、身份解析、账号绑定 |
| `webdav::account` | WebDAV 专用账号和访问范围 |
| `api/request_auth.rs`、`api/middleware` | 请求侧鉴权、管理员权限和认证上下文 |

## 配置入口

| 入口 | 用途 |
| --- | --- |
| `管理 -> 系统设置 -> 认证与 Cookie` | Cookie、安全 token、MFA 邮箱验证码要求等运行时规则 |
| `管理 -> 外部认证` | 管理 OIDC / OAuth2 provider |
| `设置 -> 安全` | 用户自己的 MFA、Passkey、外部身份和 WebDAV 凭证 |
| `config.toml [auth]` | 登录签名密钥、MFA 加密密钥、首次 HTTP 引导等启动前配置 |

## 排障方向

- 登录后反复掉线：先看公开站点地址、Cookie HTTPS 设置和反向代理 Host。
- 外部认证回调失败：先看 redirect URI、provider 配置和公开站点地址。
- WebDAV 能登录但看不到预期目录：先看 WebDAV 账号限制范围和工作空间权限。
- 公开分享能打开但预览或下载失败：继续看 [文件与工作空间](./files-workspaces) 和 [预览与处理](./preview-processing)。
