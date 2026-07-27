---
description: "The File Processing group of System Settings: archive preview, online extraction, archive building, link import, thumbnails, media processors, and media metadata switches and limits."
title: "File Processing"
---

Entry point: `Admin -> System Settings -> File Processing`. For other groups and when changes take effect, see [System Settings](/en/reference/config/runtime/).

This group controls features that read, scan, transform, or temporarily unpack file contents. Default rules:

| Item | Default |
| --- | --- |
| Online extraction source archive size limit | `512 MiB` |
| Online extraction staging size limit | `2 GiB` |
| Online extraction uncompressed size limit | `1 GiB` |
| Online extraction entry count limit | `10000` |
| Online extraction duration limit | `300` seconds |
| Online archive compression global switch | Enabled |
| User archive-download switch | Enabled |
| Share archive-download switch | Enabled |
| Archive build entry count limit | `10000` |
| Archive build total source size limit | `2 GiB` |
| Archive build output size limit | `2 GiB` |
| Archive preview global switch | Disabled |
| Archive preview user-side switch | Disabled |
| Archive preview share-side switch | Disabled |
| Archive preview source file size limit | `64 MiB` |
| Archive preview entry count limit | `2000` |
| Archive preview manifest size limit | `64 KiB` |
| Archive preview scan duration limit | `30` seconds |
| Link import engine registry | `builtin` enabled, `aria2` disabled |
| Link import file size limit | `1 GiB` |
| Link import download speed limit | `5` MB/s (`0` still means unlimited) |
| Link import concurrency limit | `1` |
| Link import request timeout | `600` seconds |
| aria2 RPC request timeout | `10` seconds |
| aria2 split | `5` |
| aria2 per-server connections | `5` |
| aria2 low-speed limit | `0` (disabled) |
| Thumbnail source file size limit | `64 MiB` |
| Image preview strategy | Original first |
| Media metadata extraction | Enabled |
| Media metadata source file size limit | `256 MiB` |

## Archive Preview

Archive preview is read-only. It scans metadata from supported archive formats and generates a manifest; it does not extract the archive into the user's folder. It is not the same thing as "online extraction".

This group has three layers of switches:

- **Enable Archive Preview**: global switch
- **Enable Archive Preview for Users**: whether logged-in users can preview archives in personal and team spaces
- **Enable Archive Preview for Shares**: whether public share pages can preview archives after passing password and share-scope checks

All three are disabled by default. Enable them only when users really need to inspect archive contents, especially the share-side switch. It lets visitors see metadata such as internal file names, directory structure, sizes, and modified times.

Limits control source archive size, entry count, returned manifest size, and single-scan duration. When an archive is opened for the first time and the manifest has not been cached, the system creates an `archive_preview_generate` background task. After generation completes, reopening reuses the cached manifest.

When users switch `filename encoding` in the preview toolbar, AsterDrive rereads or regenerates the manifest with the selected encoding. This is for old ZIP files or ZIP file names created across language environments that display as garbled text. It does not modify the original archive.

## Online Extraction and Archive Building

Online extraction, online compression, and folder archive download all use the archive background-task lane, and all of them use the server temporary directory. The defaults are sized for common personal and small-team files. Do not raise every limit immediately.

`Enable online archive compression` only controls whether users can create a new ZIP archive from selected files and folders. It is enabled by default. When disabled, new online-compression tasks are rejected. Online extraction, folder archive downloads, and archive preview are controlled by their own settings and are not disabled by this switch.

ZIP package downloads have two independent switches:

- **Enable archive downloads for users**: controls whether signed-in users can download selected content from personal or team workspaces as ZIP files
- **Enable archive downloads for shares**: controls whether public-share visitors can download ZIP files after password, share-scope, and download-count checks

When disabled, the backend rejects new matching ZIP download requests and the official frontend hides those download methods. The switches are independent and do not affect direct single-file downloads or online archive compression that creates a new ZIP file in the workspace.

If users often process large archives or large folders, check these settings separately:

- **Online extraction source archive size limit**: rejects source archives that are too large
- **Online extraction staging size limit**: counts the source archive downloaded locally plus files extracted into staging
- **Online extraction uncompressed size, entry count, path depth, compression ratio, and duration limits**: protect against archive bombs and abnormal metadata-heavy archives
- **Archive build entry count, total source size, and output size limits**: affect batch online compression and folder archive downloads

Before raising these limits, confirm that the disk backing `server.temp_dir`, CPU, and archive-task concurrency can handle the extra load. Otherwise the usual result is not "larger files work", but "tasks queue longer or the temp disk fills faster".

## Link Import

Link import creates a dedicated background-task lane that lets the server download a file from an HTTP/HTTPS source and import it into the current workspace. These runtime settings only control size, speed, concurrency, timeout, and the selected `builtin` / `aria2` download engines.

Defaults are chosen to be usable without enabling unlimited outbound bandwidth: `builtin` enabled, `aria2` disabled, file size limit `1 GiB`, speed limit `5` MB/s, concurrency `1`, and request timeout `600` seconds. aria2-specific values apply only after the aria2 engine is enabled.

For full behavior, security boundaries, aria2 deployment, and troubleshooting, see [Offline Download](/en/admin/offline-download/).

## Media Processing

Media processing is responsible for backend generation of thumbnails, image previews, and media metadata.
It now has a structured editor under `File Processing -> Media Processing`, so you do not need to edit JSON manually.

You can do these things there:

- Enable or disable a processor
- Bind file extensions to a processor
- Configure commands used by `vips_cli`, `ffmpeg_cli`, or `ffprobe_cli`
- Test whether the command can be executed by the server
- Keep AsterDrive's built-in image processor as a fallback
- Configure the generated thumbnail and image-preview maximum dimensions
- Choose whether the web image preview dialog loads the original file first or the generated medium preview first

The default built-in path covers common image formats.  
If you want to extend support for HEIC, AVIF, PDF covers, video thumbnails, or video metadata, you can connect `vips`, `ffmpeg`, or `ffprobe`, but only if those commands are actually installed in the server environment.

`Thumbnail max dimension` and `Image preview max dimension` control generated derivative size, not source-file eligibility:

- `thumbnail_max_dimension` limits generated thumbnail width and height. The default is `400` pixels.
- `image_preview_max_dimension` limits generated image-preview width and height. The default is `1600` pixels.
- Both settings are maximum-edge limits: generated derivatives keep aspect ratio, and `max(width, height)` is kept at or below the configured value.
- The same dimensions are passed to storage-native thumbnail providers through `NativeThumbnailRequest.max_width` and `max_height`.

Changing either value does not rewrite existing derivatives in place. New cache paths and ETags include the non-default dimension in the derivative version namespace, such as `1-d320` or `1-d2048`, so clients and storage backends do not accidentally reuse a derivative generated at another size. If you switch back to the default value, AsterDrive uses the default derivative version namespace again.

`Image preview strategy` only changes which image source the web preview dialog loads by default:

- **Original first**: when the browser can render the current format, load the original file first; if the browser cannot render it or the original fails, use the backend-generated preview.
- **Medium preview first**: load the backend-generated WebP preview first, and download the original only when the user chooses to view it.

If users often open large photos, or object-storage egress bandwidth is sensitive, choose "Medium preview first". This setting does not change original files and does not affect thumbnail caches; it only changes the frontend preview dialog's default loading order.

:::tip[Keep the built-in processor first]
Unless you have confirmed the command paths, permissions, and extension bindings for vips / ffmpeg / ffprobe, keeping the built-in processor as a fallback is simpler.
:::

<details>
<summary>Media processing ENV on first startup</summary>

When the service initializes system settings for the first time, it reads three bootstrap environment variables to decide whether CLI processors are enabled in the default media processing configuration:

```bash
ASTER_BOOTSTRAP_ENABLE_VIPS_CLI=true
ASTER_BOOTSTRAP_ENABLE_FFMPEG_CLI=true
ASTER_BOOTSTRAP_ENABLE_FFPROBE_CLI=true
```

The official Docker image already installs `vips`, `ffmpeg`, and `ffprobe`, and enables these three bootstrap ENV values by default. New databases usually get the corresponding processors directly.

These three variables only affect the initial default value when `media_processing_registry_json` does not yet exist. This rule table is the unified media processing configuration entry point. It manages enabled state, capability purposes, extension bindings, and command paths for built-in `images`, built-in `lofty`, VIPS CLI, FFmpeg CLI, and FFprobe CLI. Thumbnails and media metadata both use this path.
</details>

## Media Metadata

Media metadata and thumbnails share `media_processing_registry_json`:

- `media_metadata_enabled` is the global switch
- `media_metadata_max_source_bytes` limits the source file size accepted by media metadata background tasks
- When the `images` processor is enabled and has the `metadata:image` purpose, it handles image metadata
- When the `lofty` processor is enabled and has the `metadata:audio` purpose, it handles audio metadata; when it has the `thumbnail:audio` purpose, it generates WebP thumbnails from embedded audio covers
- When the `ffprobe_cli` processor is enabled and has the `metadata:video` purpose, it handles video metadata; its `config.command` can be a command name or an absolute path

If server-side `ffprobe` has been renamed, is not in PATH, or needs a custom installation path, change `ffprobe_cli.config.command` in `media_processing_registry_json` to the corresponding command or absolute path, then run `test_ffprobe_cli` in the media processing registry to probe it.
