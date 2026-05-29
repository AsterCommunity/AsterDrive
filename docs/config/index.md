---
description: AsterDrive 配置总览，帮助区分 config.toml、后台系统设置、存储策略、策略组、远程节点、WebDAV、限流、缓存和日志。
---

# 配置总览

::: tip 这一篇先帮你分清“在哪改”
AsterDrive 的配置分得很清楚。先把这些层分开，后续就能更容易判断哪些属于部署问题、哪些属于后台规则、哪些应写入 `config.toml`。
先看自己要改哪一层，再翻对应页面就行，本页不用从头读到尾。
:::

## 一共有哪几层

- **`config.toml`** —— 决定服务怎么启动：监听地址、节点模式、数据库、日志、WebDAV 前缀、网络信任、限流
- **`管理 -> 系统设置`** —— 全站规则：公开站点地址、品牌、注册登录、邮件、跨域、任务调度、分享流播放、媒体处理、压缩包预览、回收站、版本历史、WOPI、WebDAV 开关、审计日志
- **`管理 -> 外部认证`** —— 外部身份提供商：OIDC / SSO 登录入口、重定向 URI、账号绑定和自动创建策略
- **`管理 -> 存储策略`** —— 文件真正存到哪里、用哪种上传方式
- **`管理 -> 策略组`** —— 不同用户、团队、文件大小走哪条存储路线
- **`管理 -> 远程节点`** —— 主控怎么接 follower，以及 follower 的默认接收落点在哪里
- **反向代理 / 对象存储自己的配置** —— HTTPS、大文件上传、WebDAV 透传、S3 直传

前面几层是 AsterDrive 自己管的；最后一层属于反向代理、对象存储和外部网络环境。

```text
启动服务
  |
  +-- config.toml / ASTER__ 环境变量
  |     负责：监听地址、数据库、日志、WebDAV 前缀、节点模式
  |
  +-- 数据库里的系统设置
  |     负责：站点、注册、Cookie、邮件、分享流播放、压缩包预览、回收站、WOPI、审计
  |
  +-- 外部认证提供商
  |     负责：OIDC / SSO 登录入口、外部身份和本地账号绑定规则
  |
  +-- 存储策略 + 策略组
  |     负责：文件落点、上传方式、用户/团队分流
  |
  +-- 外部环境
        负责：HTTPS、反向代理、S3 CORS、WebDAV 方法透传
```

::: tip 判断规则
服务启动前就必须知道的东西，通常在 `config.toml`。服务已经能打开后才调整的全站规则，通常在后台系统设置。文件写到哪里，去存储策略和策略组。公网入口、证书和上传体积限制，去反向代理。
:::

## 我想做这件事，去哪改

| 你想做什么 | 去哪里改 |
| --- | --- |
| 改监听地址、端口、线程数、临时目录、primary / follower 模式 | [服务器](/config/server) |
| 改数据库地址、连接池、启动重试 | [数据库](/config/database) |
| 固定登录签名密钥、MFA 加密密钥、第一次纯 HTTP 引导 | [登录与会话](/config/auth) |
| 公开站点地址、品牌、注册、Cookie、Token、调度、分享流播放、压缩包预览、回收站、版本、配额、WOPI、WebDAV、审计 | [系统设置](/config/runtime) |
| 接 OIDC / SSO 外部登录，管理外部身份提供商 | [登录与会话](/config/auth) / [管理后台](/guide/admin-console#外部认证) |
| 配 SMTP、发测试邮件、改邮件模版 | [邮件](/config/mail) |
| 文件存哪里、上传/下载怎么走 | [存储策略](/config/storage) |
| 按教程接 S3 / MinIO / R2 这类后端 | [存储策略后端](/storage/) |
| 不同用户/团队走哪条存储路线 | [存储策略](/config/storage) |
| 接远程 follower，配置默认接收落点 | [远程节点](/guide/remote-nodes) |
| 改 WebDAV 路径或 WebDAV 上传硬上限 | [WebDAV](/config/webdav) |
| 给公网入口加访问限流 | [访问限流](/config/rate-limit) |
| 改缓存或日志输出方式 | [缓存](/config/cache) / [日志](/config/logging) |

## `config.toml` 在哪、怎么写

首次启动时，如果当前工作目录里没有 `data/config.toml`，AsterDrive 会自动生成一份默认配置（含随机的 `jwt_secret` 和 `mfa_secret_key`）。

::: tip 只写要覆盖的项
不需要把整份默认配置全抄出来，`config.toml` 里只写你要改的项即可，其他保留默认。
:::

配置优先级：

```text
ASTER__ 环境变量  >  config.toml  >  内置默认值
```

环境变量用双下划线 `__` 表示层级：

```bash
ASTER__SERVER__PORT=8080
ASTER__DATABASE__URL="postgres://user:pass@localhost/asterdrive"
ASTER__WEBDAV__PREFIX=/dav
```

如果用裸二进制启动，进程会先读取当前工作目录下的 `.env`，再按同一套环境变量规则启动。长期部署时建议把 `.env` 放在服务实际的工作目录里，并把权限收紧。

## 常见环境变量分几类

| 类型 | 示例 | 什么时候用 |
| --- | --- | --- |
| 启动配置覆盖 | `ASTER__SERVER__HOST`、`ASTER__DATABASE__URL`、`ASTER__SERVER__START_MODE` | 服务启动前就要确定的配置；优先级高于 `config.toml` |
| 首次引导开关 | `ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES` | 只影响系统设置第一次写入默认值；初始化完成后，后续请在后台系统设置里改 |
| 从节点自动接入 | `ASTER_BOOTSTRAP_REMOTE_MASTER_URL`、`ASTER_BOOTSTRAP_REMOTE_ENROLLMENT_TOKEN` | Docker follower 首次启动时自动 enroll；成功后建议移除 |
| 媒体处理默认值 | `ASTER_BOOTSTRAP_ENABLE_VIPS_CLI`、`ASTER_BOOTSTRAP_ENABLE_FFMPEG_CLI`、`ASTER_BOOTSTRAP_ENABLE_FFPROBE_CLI` | 只在媒体处理系统设置还不存在时，用来决定初始默认处理器 |
| 运维 CLI 参数 | `ASTER_CLI_DATABASE_URL`、`ASTER_CLI_OUTPUT_FORMAT` | 脚本里不想每次写长参数时使用，见 [运维 CLI](/deployment/ops-cli) |

::: tip 判断一个 ENV 要不要长期保留
像 `ASTER__SERVER__START_MODE`、`ASTER__DATABASE__URL` 这类是长期运行配置，可以保留。

像 enrollment token 这类一次性 bootstrap 输入，成功后就可以删掉，避免下次排查时误以为它还会继续生效。
:::

## `config.toml` 里有哪些分区

| 分区 | 作用 |
| --- | --- |
| [server](/config/server) | 监听地址、端口、线程数、临时目录、节点模式、follower 接收根目录 |
| [database](/config/database) | 数据库连接、连接池、启动重试 |
| [auth](/config/auth) | 登录签名密钥、MFA 加密密钥、首次纯 HTTP 引导 |
| [cache](/config/cache) | 内存缓存 / Redis / 关闭缓存 |
| [logging](/config/logging) | 日志级别、格式、输出文件、轮转 |
| [webdav](/config/webdav) | WebDAV 路径前缀、上传体积硬上限 |
| `[network_trust]` | 受信任的反向代理地址，影响真实客户端 IP 判定 |
| [rate_limit](/config/rate-limit) | 登录、公开分享和一般访问的限流规则 |

## 后台系统设置当前的分组

`管理 -> 系统设置` 现在按这些名字显示：

- 站点配置
- 用户管理
- 认证与 Cookie
- 邮件投递
- 网络访问
- 运行与调度
- 存储与保留
- WebDAV
- 审计日志
- 自定义配置
- 其他

::: tip 上线前容易忽略的几项
- 对外上线前，先填 `公开站点地址`；多个公开域名逐项添加，默认来源放在最前面
- 准备开放注册、找回密码或邮箱改绑前，先把邮件发通
- 准备开放外部认证前，先填对 `公开站点地址`，再到 `管理 -> 外部认证` 复制重定向 URI
- 纯 HTTP 测试环境才临时关闭 Cookie 的 HTTPS 要求
- 容量紧张时，缩短回收站、历史版本、任务产物保留时间
- 缩略图不符合预期时，检查 `存储与保留 -> 媒体处理`
- 需要压缩包清单预览时，检查 `存储与保留 -> 压缩包预览`
- 需要 OnlyOffice 一类在线预览时，去 `站点配置 -> 预览应用` 调整
- 接远程节点时，enroll 成功后还要在远程节点详情里创建默认接收落点
:::

详情见 [系统设置](/config/runtime) 和 [邮件](/config/mail)。

如果后台暂时进不去，或者你想在停机窗口里离线检查、校验或批量写入系统设置，走 [运维 CLI](/deployment/ops-cli)。

## 存储策略和策略组不在 `config.toml` 里

它们在后台页面维护，分别决定：

- **存储策略** —— 文件真正存到哪里、单文件大小上限、分片大小、上传方式
- **策略组** —— 用户或团队上传时命中哪条存储策略

详情见 [存储策略](/config/storage)。

## 路径要分清是相对谁

如果你写相对路径，记住三套语义不一样：

- `data/config.toml` 的位置 —— **相对当前工作目录**
- `[database]` 和 `[server]` 里的相对路径 —— **相对 `data/config.toml` 所在目录**（也就是 `./data/`）
- 默认本地存储策略 `data/uploads` —— **相对当前工作目录**（不是 `data/` 内）

不同部署方式默认落点：

- 本地直接运行：项目目录下的 `data/`
- systemd：`WorkingDirectory/data/`
- Docker 官方镜像：容器里的 `/data`

::: warning 长期部署写绝对路径
数据库路径、本地存储路径、临时目录最好都写绝对路径，避免后续受工作目录变化影响。
:::
