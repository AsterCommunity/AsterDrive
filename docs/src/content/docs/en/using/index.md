---
description: Using AsterDrive section home, with the workspace mental model and task page entries covering files, upload and download, workspaces, sharing, recycle bin and versions, preview and editing, WebDAV, and account security.
title: "Using AsterDrive"
---

Welcome to AsterDrive ✨

This section is for **regular users** — get an account, log in, upload your first file, share it with someone, and recover things from the recycle bin when needed. You do not have to read it in order; jump to the task page for whatever is unclear.

If you are an administrator, read this first for the regular-user perspective, then see [Administration](/en/admin/).

## Our Definition of "Easy to Use"

AsterDrive does not aim to be "the cloud drive with the most features". What we want to build:

- **Common tasks don't make you think** — uploading, finding files, sharing, and recovering from accidental deletion should each be one action
- **No surprises means no interruptions** — unnecessary prompts, dialogs, and "are you sure?" are removed as much as possible
- **Real problems give you clear signals** — error codes + readable messages + actionable next steps

If something feels off, [open an issue](https://github.com/AsterCommunity/AsterDrive/issues) — it is the most direct way to reach us.

## One Mental Model: Workspaces

After logging in, remember one thing: **everything follows the current workspace**.

- `My Space`: your personal files, personal shares, personal recycle bin, and WebDAV accounts
- Team spaces: appear only after you join a team; each team space has its own files, shares, recycle bin, and task list

Search, shares, tasks, and the recycle bin all operate on the current workspace. When troubleshooting "why can't I see a file / share / task", the first step is always: confirm which workspace you are in. See [Workspaces and Teams](/en/using/workspaces-teams/).

## Find a Page by Task

| What you want to do | Where to go |
| --- | --- |
| Organize files, search, tags, batch operations | [Files and Organization](/en/using/files/) |
| Upload, download, resume large files, import from links | [Upload and Download](/en/using/upload-download/) |
| Switch workspaces, collaborate in teams | [Workspaces and Teams](/en/using/workspaces-teams/) |
| Send links, manage shares you created | [Sharing and Public Access](/en/using/sharing/) |
| Recover deleted items, file versions | [Recycle Bin and Versions](/en/using/recycle-bin/) |
| Open, preview, and edit files online | [Preview and Editing](/en/using/preview-editing/) |
| Mount via Finder / Windows / rclone | [Using WebDAV](/en/using/webdav/) |
| Change password, MFA, Passkey, devices | [Account and Security](/en/using/account-security/) |

## Login and First Entry

The login page does not ask you to decide "login or register" first. After you enter a username or email, the page figures out the flow:

- No users exist yet: create the administrator
- Existing account: log in
- New account, and public registration is allowed: register a regular account

The first successfully created account automatically becomes the administrator; later regular registrations usually need email activation before logging in. For more login methods (MFA, Passkey, external authentication), see [Account and Security](/en/using/account-security/).
