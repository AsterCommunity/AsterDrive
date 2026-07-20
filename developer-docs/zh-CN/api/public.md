# 公共接口

这组路径都相对于 `/api/v1`，且不需要认证。

其中前端启动配置、预览应用、缩略图能力和媒体数据能力给匿名页面启动用；remote-enrollment 两条用于 primary 和 follower 之间的远端节点 enrollment 握手。这些接口只在 `primary` 节点注册。

公开配置读取接口都会带 `Vary: Authorization, Cookie`。匿名响应通常带 `Cache-Control: public, max-age=60`；`GET /public/custom-config` 在请求带有效访问 token 且返回 authenticated 可见条目时使用 `Cache-Control: private, max-age=60`。缩略图能力和媒体数据能力还会在进程内按 60 秒 TTL 缓存，并在媒体处理配置或存储策略变更时主动失效。

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/public/frontend-config` | 读取前端启动所需的公开配置 |
| `GET` | `/public/preview-apps` | 读取匿名态可见的预览应用注册表 |
| `GET` | `/public/custom-config` | 读取当前身份可见的自定义配置条目 |
| `GET` | `/public/thumbnail-support` | 读取当前匿名态可见的缩略图扩展名能力 |
| `GET` | `/public/media-data-support` | 读取当前匿名态可见的媒体元数据能力 |
| `POST` | `/public/remote-enrollment/redeem` | follower 用 enrollment token 兑换远端节点绑定信息 |
| `POST` | `/public/remote-enrollment/ack` | follower 确认 enrollment 已完成 |

## `GET /public/frontend-config`

这条接口是当前前端应用的匿名启动配置入口，返回统一 JSON 包装：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "version": 2,
    "branding": {
      "title": "AsterDrive",
      "description": "Self-hosted cloud storage",
      "favicon_url": "/favicon.svg",
      "wordmark_dark_url": "/static/asterdrive/asterdrive-dark.svg",
      "wordmark_light_url": "/static/asterdrive/asterdrive-light.svg",
      "site_urls": ["https://drive.example.com"],
      "allow_user_registration": true,
      "passkey_login_enabled": true
    },
    "downloads": {
      "archive_download_user_enabled": true,
      "archive_download_share_enabled": true
    },
    "media": {
      "image_preview_preference": "preview_first"
    }
  }
}
```

要点：

- `version` 当前为 `2`，用于前端判断启动配置结构版本
- `branding` 是公开品牌与登录入口配置的唯一当前接口结构；旧 `/public/branding` 路由已移除
- `downloads.archive_download_user_enabled` 决定登录用户是否看到并能使用个人空间、团队空间的 ZIP 打包下载方式
- `downloads.archive_download_share_enabled` 决定公开分享访问者是否能使用分享内容的 ZIP 打包下载方式
- 这两个字段来自后端运行时配置。前端只负责按能力隐藏入口，归档下载端点仍会再次检查开关，不能把前端隐藏当成权限边界
- `media.image_preview_preference` 来自运行时配置 `frontend_image_preview_preference`
- `image_preview_preference` 当前支持 `original_first` 和 `preview_first`
- 前端会缓存这份启动配置，并在相关运行时配置变更后主动刷新

## `GET /public/preview-apps`

这条接口同样返回统一 JSON 包装，`data` 里是一个公开可见的预览应用注册表：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "version": 2,
    "apps": [
      {
        "key": "builtin.formatted",
        "provider": "builtin",
        "icon": "/static/preview-apps/json.svg",
        "enabled": true,
        "labels": {
          "en": "Formatted view",
          "zh": "格式化视图"
        },
        "extensions": ["json", "xml"]
      }
    ]
  }
}
```

要点：

- `apps` 是当前匿名页面可见的预览器定义；`provider` 目前有 `builtin`、`url_template`、`wopi`
- 当前是 v2 结构，不再返回顶层 `rules`；匹配信息直接挂在每个 app 自己的 `extensions` 与 `config` 上
- 返回结果已经过滤掉被禁用的 app
- `config` 是 provider 相关配置：
  - `url_template` 预览器常见字段有 `mode`、`url_template`、`allowed_origins`
  - `wopi` 预览器常见字段有 `mode`、`action` / `action_url` / `action_url_template`、`discovery_url`
- 前端文件预览、公开分享预览和 WOPI 集成入口都会依赖这份注册表，而不是把预览器信息硬编码在前端里
- 管理员当前可以通过 `/api/v1/admin/config/frontend_preview_apps_json` 维护这份注册表

## `GET /public/custom-config`

这条接口返回当前请求身份可见的自定义配置，使用统一 JSON 包装：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "entries": {
      "my-frontend.theme.primary_color": "#6366f1",
      "my-frontend.feature.enable_beta_tab": "true"
    }
  }
}
```

要点：

- 只返回 `source = "custom"` 的条目
- 只返回 `entries` 里的 key/value，不暴露 `id`、`source`、`updated_by` 等后台字段
- 匿名响应带 `Cache-Control: public, max-age=60`
- 带有效访问 token 且返回 authenticated 可见条目时，响应带 `Cache-Control: private, max-age=60` 和 `Vary: Authorization, Cookie`
- 可见度分三档：
  - `private`：仅管理员可在后台看到，不会出现在此公开接口里
  - `public`：匿名即可读取
  - `authenticated`：需要带有效访问 token 才会返回
- 如果请求没有 token，接口只返回 `public` 条目
- 如果请求显式携带无效 token，接口返回 401，而不是静默降级成匿名
- 这条接口适合给自定义前端读取主题、开关、展示文案和其他非敏感消费者配置

## `GET /public/thumbnail-support`

这条接口返回当前服务端实际可生成缩略图的公开能力，前端用它决定文件列表里哪些扩展名应该尝试显示缩略图：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "version": 1,
    "image_preview": {
      "enabled": true,
      "extensions": ["bmp", "gif", "jpeg", "jpg", "png", "webp"]
    },
    "image_thumbnail": {
      "enabled": true,
      "extensions": ["bmp", "gif", "jpeg", "jpg", "png", "webp"]
    },
    "audio_thumbnail": {
      "enabled": false
    },
    "video_thumbnail": {
      "enabled": false
    },
    "extensions": ["bmp", "gif", "jpe", "jpeg", "jpg", "png", "tif", "tiff", "webp"]
  }
}
```

要点：

- `extensions` 已经做过规范化，统一是不带点的小写扩展名
- `image_preview`、`image_thumbnail`、`audio_thumbnail`、`video_thumbnail` 是当前按用途拆分的能力字段
- 顶层 `extensions` 是给旧客户端保留的兼容并集字段
- 内置图片处理器启用时会暴露常见图片格式
- 内置 `lofty` 处理器启用 `thumbnail:audio` 时会暴露音频后缀，前端可通过同一条 thumbnail 接口请求音频内嵌封面
- `vips_cli` / `ffmpeg_cli` 只有在对应命令可用且处理器启用时，才会把配置里的扩展名暴露出去；因此它可能包含图片以外的文档或视频扩展名
- 这份能力主要来自运行时配置 `media_processing_registry_json`
- 如果某条存储策略启用了原生处理，且实际驱动暴露存储原生缩略图 / 图片预览能力，策略里的 `thumbnail_extensions` 也会合并进公开能力列表；内置 `tencent_cos` 策略可通过 COS CI 暴露这项能力，内置 Local、S3-compatible、Azure Blob、OneDrive 和 Remote 策略不暴露

## `GET /public/media-data-support`

这条接口返回匿名态前端可用的媒体元数据解析能力，主要用于决定文件信息面板、播放器和预览页是否应主动请求 `/media-metadata`：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "version": 1,
    "enabled": true,
    "max_source_bytes": 52428800,
    "kinds": {
      "image": {
        "enabled": true,
        "match": "extensions",
        "extensions": ["bmp", "gif", "jpeg", "jpg", "png", "tif", "tiff", "webp"]
      },
      "audio": {
        "enabled": true,
        "match": "extensions",
        "extensions": ["flac", "m4a", "mp3", "ogg", "wav"]
      },
      "video": {
        "enabled": false,
        "match": "extensions",
        "extensions": []
      }
    }
  }
}
```

要点：

- `enabled` 是媒体元数据总开关，对应运行时配置 `media_metadata_enabled`
- `max_source_bytes` 会按服务端配置值返回，但会裁剪到 JavaScript 安全整数范围内
- `kinds.image` 来自内置 `images` 处理器的 `metadata:image` 用途
- `kinds.audio` 来自内置 `lofty` 处理器的 `metadata:audio` 用途
- `kinds.video` 来自 `ffprobe_cli` 处理器的 `metadata:video` 用途；命令不可用或处理器未启用时会返回 `enabled = false`
- `match = "extensions"` 表示前端应按扩展名匹配；`match = "any"` 当前只会出现在启用 `ffprobe_cli` 且没有配置扩展名过滤时，表示视频元数据可尝试所有视频候选文件
- 这份能力主要来自 `media_processing_registry_json`，并受 `media_metadata_max_source_bytes` 限制
- 启用策略级原生媒体元数据后，支持的扩展名也会合并进音频 / 视频能力；内置 `tencent_cos` 策略可通过 COS CI 暴露这项能力

## `POST /public/remote-enrollment/redeem`

这条接口不是给匿名网页用的，而是给 follower CLI enrollment 流程用的。

请求体：

```json
{
  "token": "enr_xxxxx"
}
```

成功后返回绑定引导数据：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "remote_node_id": 7,
    "remote_node_name": "edge-sh-01",
    "master_url": "https://drive.example.com",
    "access_key": "rk_xxx",
    "secret_key": "rs_xxx",
    "is_enabled": true,
    "ack_token": "enr_ack_xxx"
  }
}
```

要点：

- `master_url` 要求主节点已经配置 `public_site_url`；多来源配置时使用第一项作为 enrollment 主控地址
- `access_key` / `secret_key` 是后续内部存储协议要用的绑定信息；对象命名空间由节点绑定关系在服务端内部解析，不在这条公开响应里返回
- 兑换成功后，并不代表 enrollment 已彻底完成；follower 还需要继续调用 ack 接口

## `POST /public/remote-enrollment/ack`

同样是 enrollment 流程专用接口。

请求体：

```json
{
  "ack_token": "enr_ack_xxx"
}
```

成功时返回空的统一成功响应：

```json
{
  "code": "success",
  "msg": ""
}
```

语义上，这表示 follower 已经拿到绑定信息，并向主节点确认这次 enrollment 会话可以结束。
