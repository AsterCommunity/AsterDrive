---
description: "AsterDrive upload and download: how upload paths are decided, large-file chunking and resume, browser download methods, importing from links, the task center, and upload failure triage."
title: "Upload and Download"
---

:::tip[What this page covers]
Upload and download from the regular user's perspective: you do not need to tell the underlying mechanisms apart, but when something fails you know where to look first. Admin-side backend preparation is in [Storage Policies and Groups](/en/admin/storage-policies/) and the storage backend tutorials.
:::

## Remember These About Uploads

- Small files usually finish quickly
- Large files are automatically split into chunks
- After an interruption, uploads resume whenever possible
- To resume, go back to the original folder and pick the same file again
- Resumable uploads expire: regular chunks usually last 24 hours; direct-to-object-storage uploads are usually shorter

In daily use you do not need to distinguish "regular upload", "chunked upload", or "direct-to-object-storage" — the system picks the path automatically based on the policy group bound to the current workspace and the file size.

:::tip[Check the policy group first when uploads fail]
Many upload problems look like "the file won't upload" but are actually the wrong policy group matching the current user or team. Confirm the workspace first, then the policy group, then the specific storage policy.
:::

## Choosing Uploads for a Cluster Deployment

If your site runs multi-Primary (cluster profile), fewer upload paths are available than on a single instance: upload modes that keep temporary state on one node are rejected when the session is created. This is not a problem with your file — it is a deployment topology limit, and a rejection leaves no leftover upload session or staging files behind; just upload again over an available path.

You do not need to diagnose which case a rejection belongs to; available paths are still decided automatically by the policy group and storage policy. The administrator-side restriction list and its reasons are in [Multi-Instance and Load Balancing](/en/deploy/multi-instance/).

## Downloading Files, Folders, and Selections

After clicking `Download` on a file, folder, or multi-selection, the page offers the download methods suitable for your browser:

- **Frontend proxied download**: the current page receives the file stream, and the download center at the bottom right shows progress, speed, and remaining time; good for single-file downloads where you want to watch or cancel progress
- **Download to folder**: pick a local directory and write the selected files into it; downloading folders or multiple items preserves the relative directory structure, and the browser asks for directory write permission on first use
- **Use browser default download**: hand a single file directly to the browser; save location and progress live in the browser's own download list
- **Proxied ZIP archive download**: AsterDrive packs the files and the page downloads them, with progress in the download center
- **Use browser download for ZIP**: AsterDrive packs the selection and hands the result directly to the browser

Browsers differ in what they support. If the browser cannot write to a local directory, the page suggests ZIP when the administrator allows it; when the administrator disables ZIP archive downloads, those options simply disappear — the page is not broken. Single-file browser default download still works.

The bottom-right area is the shared transfer activity zone for uploads and downloads. Collapsed, it shows only a summary; expanded, it shows each item's status. Proxied downloads can be cancelled, failed items retried, and finished items cleaned up.

:::tip[The download center is not the task center]
The bottom-right download center tracks proxied or directory downloads running in the current browser. The `Task Center` tracks server-side background tasks such as online compression, extraction, and link imports. After choosing "browser default download" or "browser ZIP download", check the browser's own download list for progress.
:::

## Importing Files from Links

If what you have is an HTTP/HTTPS download link rather than a local file, use `Import from link` on the file page. AsterDrive has the server download that address and imports the result into the target folder of the current workspace.

Good fits:

- The server's network to the target site is more stable than your browser's
- You want to save a public download URL straight into AsterDrive
- You need to import a file into a team space without downloading it locally first

When creating the task you can fill in:

- Source URL: must be `http://` or `https://`
- Filename: optional; when empty, the server response headers or URL path are used to derive it
- Target folder: defaults to the current folder
- Expected SHA-256: optional; when set, the file hash is verified after download and the task fails on mismatch

Link imports create background tasks and do not block the page. Check progress in the current workspace's `Task Center`.

Administrators can limit per-file size, per-task download speed, concurrent task count, and request timeout, and can configure the fallback order between the built-in downloader and the aria2 engine; when all engines are off, new import tasks are rejected. Full configuration is in [Offline Download](/en/admin/offline-download/).

:::tip[Not every link can be imported]
To prevent the server from being abused to reach intranet or metadata addresses, link import only supports HTTP/HTTPS and rejects hosts resolving to loopback, private, link-local, multicast, documentation-reserved, and cloud metadata addresses. The current implementation does not follow HTTP redirects; if the download site responds with a redirect, use the final real download address. When the aria2 engine is enabled, AsterDrive still runs these checks first, but the actual outbound connection is made by the administrator-deployed aria2 service.
:::

## Task Center

The operations that most commonly land in the `Task Center`:

- Folder archive downloads
- Online compression of a batch of selected files or folders
- Online extraction of archives
- Importing files from links
- Generating the manifest the first time an archive preview is opened
- Emptying the entire recycle bin

In the `Task Center` you can:

- See whether a task is queued, processing, completed, cancelled, or failed
- See creation, start, and finish times
- See each step's current progress and status message
- Open the result folder directly after completion
- Requeue a failed task

The task center also follows the current workspace. Tasks you started in a team space only appear in that team space's `Task Center`.

## Upload Failure Triage Order

1. Is the current workspace the right one?
2. Which policy group is bound to the current user or team?
3. Is the matched storage policy's single-file size limit enough?
4. Are the reverse proxy's request body size and timeout enough?
5. If using direct-to-object-storage, is CORS configured correctly?
6. If using OneDrive Graph direct upload, are browser extensions or the company network blocking access to Microsoft?
7. If using SFTP, are the endpoint, SSH credentials, base path, and host key fingerprint correct?
8. If using a follower node, is the node enabled, does the current transport pass the connection test, are protocol capabilities compatible, and is a default remote storage target applied?
9. Is the user or team quota full?
