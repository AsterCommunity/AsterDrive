---
description: AsterDrive 预览与处理功能地图，覆盖缩略图、媒体信息、压缩包预览、WOPI、文件编辑和分享流播放。
---

# 预览与处理

预览与处理负责把原始文件变成浏览器可查看、可打开或可流式播放的结果。它不改变文件归属，但会依赖存储读取、后台任务、外部工具和 WOPI 服务。

## 能力边界

| 能力 | 说明 | 相关文档 |
| --- | --- | --- |
| 缩略图 | 支持图片等 MIME 的缩略图生成、缓存和后台任务 | [系统设置](/config/runtime)、[用户手册](/guide/user-guide) |
| 图片预览 | 浏览器原图渲染、后端中等预览图、全屏查看、缩放、拖动、旋转和前后切图 | [用户手册](/guide/user-guide)、[系统设置](/config/runtime) |
| 媒体信息 | 音视频元数据解析，支持本地工具或存储原生处理 | [腾讯云 COS](/storage/tencent-cos) |
| 压缩包预览 | 只读查看支持格式的目录、文件名、大小和修改时间，默认关闭 | [在线预览与 WOPI](/guide/preview-and-wopi)、[系统设置](/config/runtime) |
| WOPI | OnlyOffice / Collabora 等外部服务打开和保存 Office 文件 | [在线预览与 WOPI](/guide/preview-and-wopi)、[文件编辑](/guide/editing) |
| 浏览器内编辑 | 文本类文件编辑、保存和版本记录 | [文件编辑](/guide/editing) |
| 分享流播放 | 分享页音视频播放短时效 session | [分享与公开访问](/guide/sharing)、[系统设置](/config/runtime) |

## 后端模块

| 模块 | 负责内容 |
| --- | --- |
| `thumbnail`、`task::thumbnail` | 缩略图、图片预览图缓存和任务派发 |
| `media::processing` | VIPS / FFmpeg / FFprobe 等处理器选择 |
| `media::metadata` | 音视频媒体信息解析 |
| `files::archive::core`、`files::archive::preview` | 压缩包扫描、路径校验和只读预览 |
| `preview::apps`、`preview::wopi` | 预览应用、WOPI discovery、锁、proof、session |
| `share::ticket`、`share::stream` | 分享流播放 ticket 和短会话 |

## 配置入口

| 入口 | 用途 |
| --- | --- |
| `管理 -> 系统设置 -> 文件处理` | 缩略图、图片预览策略、媒体处理器、压缩包预览 |
| `管理 -> 系统设置 -> 站点配置 -> 预览应用` | WOPI discovery 和打开方式 |
| `管理 -> 系统设置 -> 运行时` | 分享流播放 session 有效期等运行时限制 |
| 存储策略编辑页 | 腾讯云 COS 等存储原生处理开关 |

## 排障方向

- WOPI 打不开：先确认公开站点地址、WOPI 服务能否回连、预览应用是否启用对应扩展名。
- 缩略图或图片预览图不生成：看 MIME 是否支持、后台任务是否失败、处理器是否可用、源文件是否超过大小上限。
- 压缩包预览失败：看总开关、文件大小、格式支持和归档内路径安全校验。
- 分享页音视频播放中断：看分享流播放 session TTL 和反向代理流式响应。
