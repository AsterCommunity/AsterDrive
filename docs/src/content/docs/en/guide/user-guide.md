---
title: "User Manual"
---

Welcome to AsterDrive.

This page is for **regular users**: receiving an account, logging in, uploading the first file, sharing it with others, and recovering content from the trash when needed. The manual is organized by page. You do not need to read it in order; after login, jump to the section for the area you do not understand.

If you are an administrator, it is useful to read this page first to understand the regular-user perspective, then read [Admin Console](./admin-console/).  
If you plan to mount WebDAV or connect Office files to external editors, the related sections cover those specifically.

## What We Mean by "Easy to Use"

AsterDrive is not trying to be "the cloud drive with the most features". We want to build something like this:

- **Common actions should not make you think**: upload, find files, share, and recover from mistaken deletion should each be one clear action
- **Expected things should not interrupt you**: unnecessary prompts, unnecessary dialogs, and unnecessary "are you sure" moments should be minimized
- **When something really goes wrong, give a clear signal**: error code + readable message + an actionable next step

If something feels awkward after you use it, [open an issue and tell us](https://github.com/AsterCommunity/AsterDrive/issues). That is the most direct feedback path.

## Login and First Entry

The login page does not ask you to decide first whether this is login or registration.  
After you enter a username or email, the page decides the flow automatically:

- There are no users in the system: create the administrator
- The input is an existing account: log in
- The input is a new account and public registration is allowed: register a regular account

Important notes:

- The first successfully created account automatically becomes an administrator
- Later newly registered regular accounts usually need to click the activation email before logging in
- If public registration is disabled, contact an administrator to create an account for you
- If mail is configured, you can request a password reset email directly from the login page
- If you have added a passkey, you can log in directly from the login page with device unlock or a security key
- If the administrator configured external authentication, the login page shows the corresponding external login entries
- If your account has MFA enabled, after password or external identity login, you must complete second-factor verification. Common methods are authenticator codes and recovery codes; administrators can also enable email codes.

## Understand Workspaces First

After login, the top of the left side shows the workspace list:

- `My Space`: your personal files, personal shares, personal trash, and WebDAV accounts are here
- Team spaces: appear only after you join teams; each team space has its own files, shares, trash, and task list

Search, shares, tasks, and trash all follow the current workspace.  
To manage a group of content, switch to the corresponding workspace first.

## Common Areas on the Files Page

- Left-side workspace list: switch personal space or team spaces
- Left-side folder tree: jump quickly into the target folder
- Top search box: search files and folders in the current workspace
- `Trash`: handle deleted content in the current workspace
- `My Shares`: view links already sent from the current workspace
- `Task Center`: view background tasks such as online compression, online extraction, package downloads, and link import
- `WebDAV`: creates desktop client accounts for the current workspace; personal and team spaces manage their own accounts
- `Settings` in the top-right user menu: adjust profile, interface, security, and team-related settings

## Upload and Organize Files

The file list, context menu, and top action area handle most daily work:

- Create folders
- Create blank text files
- Upload files
- Upload folders
- Import files from HTTP/HTTPS links
- Download files
- Rename, copy, move, and delete files and folders
- View details
- Manage tags
- Manually lock or unlock files
- Online compression, online extraction, and folder package download
- Switch between list view and grid view
- Sort by name, size, creation time, update time, or type

You can also drag directly:

- Drag files or folders to target folders in the left-side folder tree
- Drag files or folders to a parent directory in the top breadcrumbs
- Drag files or folders to the left-side trash

## Download Files, Folders, and Multiple Items

After you click `Download` for a file, folder, or multi-selection, the page offers the download methods supported by the current browser:

- **Frontend proxy download**: the current page receives the file stream and shows progress, speed, and estimated remaining time in the bottom-right download center; use it when you want visible or cancellable progress for one file
- **Download to folder**: choose a local directory and write the selected items into it; folder and multi-item downloads preserve relative directory structure, and the browser asks for directory-write permission the first time
- **Use browser default download**: hand one file directly to the browser, which manages the save location and progress in its own download list
- **Proxy ZIP download**: AsterDrive packages the selection and downloads it through the page, with progress in the bottom-right download center
- **Use browser ZIP download**: AsterDrive packages the selected content and hands the ZIP directly to the browser

Browser capabilities differ. If direct directory writing is unavailable, the page offers a ZIP fallback when the administrator allows ZIP downloads. When the administrator disables archive downloads, ZIP choices disappear; this does not mean the page is broken. Browser-default download for a single file remains available.

Uploads and downloads share the bottom-right transfer activity area. The collapsed view shows a summary; expand it to inspect individual items. Proxy downloads can be canceled, failed items can be retried, and completed items can be cleared. The page reserves space for the activity area so it does not cover the final rows of file content.

:::tip[Download Center and Task Center are different]
The bottom-right download center tracks proxy and local-directory downloads running in the current browser. `Task Center` tracks server-side work such as online compression, online extraction, and link import. After choosing “Use browser default download” or “Use browser ZIP download”, check progress in the browser's own download list.
:::

## Search, Multi-Select, and Batch Operations

The top search box searches files and folders by name in the current workspace.  
You can click the search box directly or use `Ctrl + K` to open the search panel. In the panel, you can switch between "all / files only / folders only", quickly filter by images, videos, music, documents, spreadsheets, presentations, archives, code, and others, and select one or more tags.

After entering a keyword or choosing filters, press `Enter` or click `Search` to open the search results page for the current workspace. On that page, you can continue opening files, jumping to file locations, viewing details, or using available item actions. Team-space results and personal-space results are not mixed.

Tag filtering has two modes:

- `Match any`: an item appears if it has any selected tag
- `Match all`: an item appears only if it has every selected tag

The left side also has quick entries for common types such as images, videos, music, and documents. Clicking one still searches by file category inside the current workspace.

Search state on the files page follows the current workspace.  
Results you find in a team space do not mix with personal-space files.

When you need to handle multiple items at once, select them and batch execute:

- Batch move
- Batch copy
- Batch delete
- Batch package download

### Copy or move between workspaces

The destination picker can switch workspaces for both copy and move. A typical flow is:

1. Select files or folders in the current workspace.
2. Click `Copy to` or `Move to`.
3. Choose the destination workspace, then open the destination folder.
4. Confirm the action.

Copy keeps the original items. A successful move places the originals in the source workspace's trash. Folders are handled together with their contents.

Copying one file or folder uses the single-item action. Batch copy is used when you select multiple items.

Moving one file or folder uses the single-item action. Batch move is used only when you select multiple items.

Keep these points in mind:

- You need access to both the source and destination workspaces.
- If the destination does not have enough space, the source items are kept.
- Locked files or folders cannot be moved between workspaces.
- If a name already exists at the destination, AsterDrive adds a copy number automatically.
- For a multi-item operation, check the success and failure counts. When some items fail, refresh both workspaces before trying again.
- Moving inside the same workspace remains a normal folder move and does not create a new copy.

In the file list and trash, you can use `Ctrl + A` or `Cmd + A` to select all items on the current page.

File or folder "Details" shows name, size, storage used, type, creation time, modification time, lock status, share status, storage policy ID, and other information.
When diagnosing "which policy is this file on" or "is it locked", start here.

"Size" and "storage used" are not the same:

- File size is the size of the current file itself
- File storage used also counts historical versions
- Folder storage used recursively counts everything under that folder, which helps find which directory is consuming quota

## How Tags Work

Tags belong to the current workspace. Tags in your personal space do not automatically appear in team spaces, and each team has its own tag library.

You can manage tags from a file or folder context menu, action menu, or details panel:

- Add or remove tags on one file or folder
- Select multiple items and add or remove tags in batch
- Open `Tag Library` to create, rename, recolor, or delete tags
- Filter search results by tags

If no tag matches your search in the tag picker, you can create a new tag directly. After creating it, still confirm the tag changes for the current item; the change is persisted only after the page reports that it has been saved.

Deleting a tag removes it from every file and folder using it in the current workspace. It does not delete files. If you only want to remove a tag from a few files, remove the tag from those items instead of deleting it from the library.

## Open, Preview, and Edit

Many common files can open directly on the web page, such as:

- Images
- Audio and video
- PDF
- Markdown
- CSV / TSV
- JSON
- XML
- Archive listings
- Plain text and common code files

Text files can usually be edited directly. When saving, the system automatically:

- Checks whether the file was changed by someone else after you opened it
- Creates a new version
- Releases the edit lock

If the administrator configured extra online preview or online editing capabilities for the site, some files will also show extra "open with" entries.  
The most common scenario is handing Office files to an external previewer or WOPI online editor.

Whether such entries appear depends on whether the administrator configured a corresponding preview app for the current file type.  
If your `docx`, `xlsx`, or `pptx` file does not show an extra open method, the site has usually not connected that file type to a usable external service yet.

Image preview first uses the original file when the browser can render it directly. If the browser cannot render the format, or the administrator configured the site to prefer medium previews, the page uses a backend-generated preview instead. After opening an image, you can move to the previous / next image in the current list, folder, or share scope. You can also enter fullscreen, zoom, pan, and rotate. For very large photos, the administrator may configure previews to load the medium image first and download the original only when you explicitly choose to view it.

Archive preview is a read-only listing preview. It shows only directories, files, sizes, and modification times inside the archive. It does not extract the archive into the current folder and cannot download a single file inside the archive. The first time you open an archive, it may show generation in progress until the background task finishes and the listing is displayed.

If filenames inside the ZIP look garbled, switch `Filename encoding` in the preview toolbar. Try `Auto` first. If it does not work, choose based on the archive source, such as `GB18030`, `CP437`, `Shift_JIS`, or `Big5`. This choice only affects listing display and does not modify the archive itself.

## How to Use Version History

In version history, you can:

- View old versions
- Restore an old version
- Delete old versions you no longer need

After restoring an old version, versions newer than that one are truncated, so confirm before restoring.

If a conflict prompt appears while saving, it usually means this file was changed by someone else after you opened it. Refresh the content first, then decide whether to continue saving.

## Task Center

In the current UI, the most common actions that enter `Task Center` include:

- Package downloading a folder
- Online compression after selecting a batch of files or folders
- Online extraction of an archive
- Import files from links
- Generating a listing the first time an archive preview opens
- Emptying the whole trash

If the site later adds other background file tasks, they will appear here in the same way.

In `Task Center`, you can:

- See whether a task is queued, processing, completed, canceled, or failed
- View creation time, start time, and completion time
- View current progress and status description for each step
- Open the result folder directly after completion
- Requeue after failure

Task Center also follows the current workspace.  
Tasks started in a team space must be viewed in that team's `Task Center`.

## Import Files From Links

If you already have an HTTP/HTTPS download URL instead of a local file, you can use `Import from link` on the files page. AsterDrive asks the server to download that URL and import the result into the target folder in the current workspace.

Typical cases:

- The server can reach the source site more reliably than your browser
- You want to save a public download URL directly into AsterDrive
- You want to import into a team space without downloading locally first

When creating the task, you can fill in:

- Source URL: must be `http://` or `https://`
- Filename: optional; if omitted, the server prefers the response header or the URL path
- Target folder: defaults to the current folder
- Expected SHA-256: optional; if set, the task verifies the final file hash and fails on mismatch

Link import creates a background task. It does not block the page. Check progress in the current workspace's `Task Center`; tasks created in a team space only appear in that team's task center.

Administrators can limit the maximum file size, per-task download speed, number of concurrent link-import tasks, and request timeout. They can also enable the built-in downloader, an administrator-managed aria2 engine, or both in fallback order through the link-import engine registry. If all engines are disabled, new link-import tasks are rejected. For full configuration and troubleshooting, see [Offline Download](/en/config/offline-download/).

The UI shows the speed limit in MB/s because it is easier for most users to understand than Mbps. The backend setting still uses bytes-per-second internally.

:::tip[Not every link is allowed]
To prevent the server from being used to reach internal or metadata addresses, link import only supports HTTP/HTTPS and rejects hosts that resolve to loopback, private, link-local, multicast, documentation, or cloud metadata ranges. The current implementation does not follow HTTP redirects. If the source site returns a redirect, use the final direct download URL instead. If the site enables the aria2 engine, AsterDrive still performs these checks first, but the actual outbound connection is made by the administrator-managed aria2 service.
:::

## Trash

Normal deletion does not immediately remove files or folders permanently. They enter the trash first.

In trash, you can:

- Restore items
- Permanently delete a single item
- Empty the whole trash
- Batch restore
- Batch permanently delete

If the original parent directory no longer exists, the item returns to the root directory during restore.

Emptying the whole trash creates a background task instead of freezing the page until everything is deleted synchronously. After confirming, open the current workspace's `Task Center` and check the `Trash purge` task progress. A team-space trash purge task appears only in that team space's task center.

## Share Links

Shares follow the current workspace.  
To share files in a team space, switch to that team space first, then create the share.

AsterDrive supports file shares and folder shares. When creating a share, you can set:

- Password
- Expiration time
- Maximum download count

Rules to note:

- The same file or folder can have only one active share at the same time
- If you want to change the password, expiration time, or download count, you do not need to recreate the link; edit it directly in `My Shares`
- If the link has a password, visitors usually do not need to enter it again for about 1 hour after successful verification in the current browser

If you share a single file, the dialog can also switch to `Direct link` mode. Direct links do not support password, expiration time, or download count limits, but they provide an additional "force download link". The default direct link is better for opening directly; the force download link explicitly asks the browser to download. When the file lives on third-party storage with a `presigned` download policy enabled, this force download link may first redirect to a short-lived download URL.

When playing audio or video on a share page, the system creates a short-lived playback session, valid for about 3 hours by default. This session is only for the browser player. The share link's own password, expiration time, and download count limits still apply normally.

If the administrator enables share-side archive preview, visitors can also view a read-only listing of supported archive contents on the share page. If the share has a password, visitors must pass password verification before seeing the listing.

## My Shares

The left-side `My Shares` page lists links already sent from the current workspace.

There you can:

- Copy share links
- Open public pages
- Edit password, expiration time, and download count limits
- Delete shares no longer needed
- View open counts and download counts
- Batch remove shares after multi-select

The share list is paginated. When there are many shares, confirm the current page before batch operations so you do not mistakenly think all historical shares were selected at once.

## WebDAV

If you want to use AsterDrive directly from Finder, Windows Explorer, rclone, or other desktop tools, create a dedicated WebDAV account.

WebDAV accounts follow workspaces. An account created in personal space opens only your personal files; an account created in a team space opens only that team's files. The WebDAV address is the same; the credentials decide which workspace the client enters.

Common practice:

- One independent account per device
- Disable only that account if a device is lost
- Restrict an account to a folder under the root directory

When creating a WebDAV account, you can:

- Customize the username
- Customize the password, or let the system generate one automatically
- Specify the access scope

Team-space WebDAV accounts are managed under `Settings -> Teams -> Team Details -> WebDAV`. Team owners and administrators can see all team accounts; regular members can manage only the team WebDAV accounts they created.

The password is shown only once after creation. Save it immediately.

The default WebDAV address is usually:

```text
https://your-domain/webdav/
```

If the administrator changed the WebDAV prefix, use the new address.

For implemented methods, filename URL encoding, same-name file/folder behavior, and property, lock, and DeltaV boundaries, see [WebDAV Features and Limits](/en/guide/webdav/).

## Settings

After entering from the top-right user menu, the settings page has four sections.

### Profile

You can change:

- Display name
- Avatar

Avatar supports common methods:

- Upload and crop an avatar
- Use a Gravatar avatar generated from your email
- Clear the current avatar

Your username is shown here.  
Email status is also shown, but changing the bound email is handled under `Settings -> Security`.

### Interface

You can adjust:

- Light / dark / follow system
- Theme color
- Display language
- Default file browser view
- Single-click or double-click to open files and folders
- Whether to enable realtime file-change sync
- Display time zone

Here, "realtime file-change sync" means the web page refreshes the current view through realtime push. It is not a desktop local-folder sync client, so keep it separate from local sync capability.

### Security

This section handles email status, email change, password, MFA, passkeys, external identities, and login devices.

You can see:

- Whether the current email has been verified
- Whether passkeys can be added
- Whether MFA is enabled
- Which external identities are bound
- Whether there is a "new email pending confirmation"

If the current email has been verified, you can:

- Enter a new email
- Send a confirmation email to the new address
- Resend the confirmation email when needed

The new email takes effect only after you open the confirmation link.

You can also change the login password here. Changing the password requires entering the current password first.

In the current version, after a password change succeeds, the current browser session stays logged in, while login sessions on other devices become invalid and must log in again.

If an administrator requires a password change, AsterDrive sends you directly to the password-change screen after a successful login. Until you enter the current password (if admin reset it, this is a temporary password) and set a new one, normal file, team, share, and admin areas are unavailable; you can only read basic account state, change the password, or sign out. After the change succeeds, the account returns to normal login state.

The `Multi-factor authentication` tab can add a second factor to the account. The factor users can bind themselves is a TOTP authenticator app, such as 1Password, Bitwarden, Google Authenticator, or Microsoft Authenticator.

When enabling MFA, the system asks you to:

- Scan a QR code with an authenticator app, or manually enter the secret
- Enter the current 6-digit code to finish binding
- Download or copy recovery codes

Recovery codes are shown in plaintext only once when generated, and each can be used only once. Save them to a password manager, encrypted note, or another safe place. If you lose the authenticator, you can complete second-factor verification on the login page with a recovery code. After logging in, regenerate recovery codes.

After MFA is enabled, password login and external identity login both require the second factor. Passkey login itself relies on device unlock or a security key to complete user verification, so it does not enter this TOTP challenge. Disabling MFA or regenerating recovery codes also requires entering the current TOTP code or an unused recovery code to confirm.

If the administrator enables email-code MFA and your email address is verified, the login second-factor page may also show `Email code`. After you send one, AsterDrive sends an 8-digit one-time code to your verified email address. Codes are valid for 10 minutes by default, but never longer than the remaining lifetime of the current MFA login flow; the same user cannot resend within 60 seconds by default.

If you already have an authenticator enabled, whether email code can be used as a backup method depends on administrator configuration. Security-sensitive sites may disable this fallback and allow only authenticator codes or recovery codes.

If the authenticator and recovery codes are both lost, and the current site has no usable email-code path, contact an administrator to reset MFA in user details.

The `Passkey` tab manages passwordless login methods:

- Add a new passkey
- Rename existing passkeys
- View creation time and last used time
- Delete passkeys no longer used

When adding one, the browser opens the system verification window. After success, the login page can use device unlock, fingerprint, face, or security key to log into the account directly. The exact method depends on your browser and system.

If you have already added a passkey but the login page suddenly does not show the passkey sign-in entry, the current browser environment may not support it, or an administrator may have temporarily disabled site-wide passkey sign-in. That switch does not delete registered passkeys; they can be used again after the administrator re-enables it.

The `External identities` tab lists external login identities bound to the current account. After unbinding, that identity can no longer directly log into this account. If the administrator enabled auto-binding by verified email, it may still bind again later when the rules are met.

In `Login devices`, you can also:

- View devices that are still logged in
- Remove one device separately
- Sign out other devices at once

If you remove the current device, the current browser logs out immediately.

### Teams

`Settings -> Teams` lists the teams you have joined.

You can:

- View team name, description, member count, and space usage
- Open a team workspace directly
- Enter the team management page
- View archived teams

If you are an administrator or owner in a team, you can continue managing members and viewing team audit.  
If you have the corresponding permission, you can also restore archived teams.
