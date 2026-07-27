---
description: "AsterDrive preview and editing: built-in previews (images, audio/video, archive manifests), in-browser text editing, external openers (WOPI / URL templates), and edit locks from the user perspective."
title: "Preview and Editing"
---

:::tip[What this page covers]
From the regular user's perspective: which files open directly, which can be edited in place, and where extra "open with" entries come from. How administrators configure preview apps and WOPI integration is in [Preview and File Processing](/en/admin/preview-processing/).
:::

## Which Files Open Directly in the Browser

Many common files open directly in the page, for example:

- Images
- Audio and video
- PDF
- Markdown
- CSV / TSV
- JSON
- XML
- Archive manifests
- Plain text and common code files

## Image Preview

When an image opens, AsterDrive chooses between two sources:

- The original image, if the browser can render it directly
- A medium WebP preview generated and cached by the backend

Administrators can adjust the default under `File Processing -> Media Processing -> Image Preview Strategy`: `prefer original` suits sites that want the raw file whenever possible; `prefer medium preview` suits sites with many large photos or sensitive bandwidth. Either way, if the browser does not support the original format, it falls back to the backend preview.

The image viewer supports fullscreen, zoom, pan, rotate, and switching to previous / next within the current list, folder, or share scope. When a backend preview is missing for the first time, a background task is created; until it finishes, the page may show a loading or fallback state, and later opens reuse the cache.

## Audio and Video Preview

For signed-in users, audio and video previews read from the current storage policy and support browser Range requests. On public share pages, audio and video first create a short-lived streaming session (about 3 hours by default) and point the player at that temporary address. This session is not the share link's own expiry; the share's password, expiry, and download limits still apply as usual.

## Read-Only Archive Preview

Archive preview is a **read-only manifest preview**:

- Currently supports ZIP
- Shows only directories, files, sizes, and modification times
- Does not extract the archive into your folder
- Does not offer downloads of individual files inside the archive
- Is not a replacement for "online extraction"

The first time you open an archive without a cached manifest, the backend creates an `archive_preview_generate` background task. The page shows "generating" and retries automatically; after the task completes, later opens use the cache directly.

If filenames inside a ZIP show as mojibake, switch `Filename Encoding` in the preview toolbar. Try `Auto` first, then pick by the archive's origin, such as `GB18030`, `CP437`, `Shift_JIS`, or `Big5`. This only affects the manifest display — it does not modify the archive itself or affect online extraction results.

Archive preview is off by default; whether you can use it depends on whether the administrator enabled the master switch and the corresponding entry (user side / share side).

## Editing Text Files in the Browser

In-browser editing mainly targets text files: Markdown, CSV / TSV, JSON, XML, TOML / YAML / INI configs, logs, scripts, and common code files.

When you save, AsterDrive automatically:

- Locks the file during editing
- Checks whether the file was changed by someone else before saving
- Generates a new historical version after a successful save
- Releases the lock when the editor closes

If you get a conflict prompt when saving, the file was usually changed by someone else while you were editing. Refresh the content first, then decide whether to continue.

## Where Extra "Open With" Entries Come From

If the administrator configured preview apps for the site, some files show extra "open with" entries. These may include:

- Built-in previewers
- External URL template previewers
- WOPI online openers

The most common use of WOPI is opening `docx`, `xlsx`, and `pptx` files in a compatible service like OnlyOffice or Collabora. From the user's perspective, WOPI only requires remembering a few things:

- It is not a separate site entry, but an opener inside the file preview window
- It does not appear on every deployment
- AsterDrive creates a session and signs an access token when you open the file
- Content saved back through WOPI writes to the original file and enters version history
- The editing interface's look and buttons are mainly decided by the external service, not entirely by AsterDrive
- Multi-user collaboration depends on the connected external service

URL template previewers are closer to "handing the file's preview link to an external web page" and usually do not save back. The built-in Microsoft / Google previewers work this way; they require the file preview link to be reachable by the external service, so intranet addresses, `localhost`, or plain HTTP links often fail outright.

If an Office file shows no extra opener, there are usually only two reasons: the administrator has not configured a preview app for that file type, or the deployment has no working WOPI service connected.

## Editing via WebDAV Also Keeps Versions

If you prefer desktop applications, you can also edit files through WebDAV — Finder network locations, Windows mapped drives, rclone sync, or WebDAV-capable editors. Overwrite saves through WebDAV also generate historical versions.

## What to Do About Locks

While a file is being edited, other users cannot freely overwrite, rename, move, or delete it.

Common handling:

- Ask the user holding the lock to save and exit normally
- If a browser tab, WebDAV client, or external WOPI editor died unexpectedly, have an administrator clean the stale lock under `Admin -> Locks`

Expired locks are cleaned up automatically in the background; no manual work is needed.

## Boundaries

- In-browser editing mainly suits text files
- Whether Office files can open online depends on whether the administrator configured a matching preview app
- The system does not merge conflicts automatically
- The number of retained versions is controlled by the administrator; see [Recycle Bin and Versions](/en/using/recycle-bin/)
