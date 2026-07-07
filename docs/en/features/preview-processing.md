---
description: AsterDrive preview and processing feature map covering thumbnails, media metadata, archive preview, WOPI, file editing, and share streaming.
---

# Preview and Processing

Preview and processing turn original files into browser-viewable, openable, or streamable results. This area does not change file ownership, but it depends on storage reads, background tasks, external tools, and WOPI services.

## Capability Boundaries

| Capability | Notes | Related docs |
| --- | --- | --- |
| Thumbnails | Thumbnail generation, cache, and background tasks for supported MIME types | [System Settings](/en/config/runtime), [User Manual](/en/guide/user-guide) |
| Image preview | Browser original rendering, backend-generated medium previews, fullscreen viewing, zoom, pan, rotation, and previous/next navigation | [User Manual](/en/guide/user-guide), [System Settings](/en/config/runtime) |
| Media metadata | Audio/video metadata from local tools or storage-native processing | [Tencent COS](/en/storage/tencent-cos) |
| Archive preview | Read-only listing of supported archive paths, names, sizes, and modified times; disabled by default | [Online Preview and WOPI](/en/guide/preview-and-wopi), [System Settings](/en/config/runtime) |
| WOPI | OnlyOffice / Collabora open and save flows | [Online Preview and WOPI](/en/guide/preview-and-wopi), [File Editing](/en/guide/editing) |
| Browser editing | Text-like file editing, saves, version records | [File Editing](/en/guide/editing) |
| Share streaming | Short-lived audio/video playback session on share pages | [Sharing and Public Access](/en/guide/sharing), [System Settings](/en/config/runtime) |

## Backend Modules

| Module | Owns |
| --- | --- |
| `thumbnail`, `task_service::thumbnail` | Thumbnail and image-preview cache and task dispatch |
| `media::processing` | VIPS / FFmpeg / FFprobe processor resolution |
| `media::metadata` | Audio/video metadata parsing |
| `archive_service`, `archive_preview_service` | Archive scanning, path validation, read-only preview |
| `preview::apps`, `preview::wopi` | Preview apps, WOPI discovery, locks, proof, sessions |
| `share::ticket`, `share::stream` | Share streaming tickets and short sessions |

## Configuration Entry Points

| Entry point | Purpose |
| --- | --- |
| `Admin -> System Settings -> File Processing` | Thumbnails, image preview strategy, media processors, archive preview |
| `Admin -> System Settings -> Site Configuration -> Preview Apps` | WOPI discovery and open methods |
| `Admin -> System Settings -> Runtime` | Share streaming session TTL and runtime limits |
| Storage policy editor | Storage-native processing switches such as Tencent COS |

## Troubleshooting Direction

- WOPI does not open: confirm public site URL, WOPI callback reachability, and enabled file extensions.
- Thumbnails or image previews do not generate: check MIME support, failed background tasks, processor availability, and whether the source file exceeds the size limit.
- Archive preview fails: check global switch, file size, supported format, and archive path safety.
- Share-page media playback stops: check share streaming session TTL and reverse proxy streaming behavior.
