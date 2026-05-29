# Sharing and Public Access

This page explains how to share files and folders with others, and what recipients see when they open a share link.

Sharing fits two scenarios:

- Send out a single file directly
- Send an entire folder for others to browse

Shares always follow the current workspace: personal files are shared from the personal space, and team files are shared from the corresponding team space.

## Create a Share Link

When creating a share link from a file or folder action menu, you can set:

- Password
- Expiration time
- Maximum download count

The page provides common time options, such as 1 hour, 1 day, 7 days, 30 days, or no expiration.

## Direct File Links

If the target is a single file, the share dialog can also switch to `Direct link` mode.

It is not the same as a normal share page:

- Direct links only apply to files, not folders
- Direct links do not support password, expiration time, or download count limits
- The page gives you both a default direct link and a "force download link"

The difference between the two links:

- The default direct link is better for file types browsers can open directly; the server responds inline
- The force download link asks the browser to download the file as an attachment; if the file lives on object storage with a `presigned` download policy enabled, AsterDrive validates first and then redirects the browser to a short-lived download URL

## Preview and Playback on Share Pages

Share pages can preview file types supported by the browser and site configuration. Common images, PDFs, text files, audio, and video open directly on the page. Whether Office files have additional open methods depends on whether the administrator configured corresponding preview apps or WOPI.

When audio or video plays on a share page, AsterDrive first creates a short-lived streaming session. This session supports Range requests, so seeking through the timeline or playing music in the background does not need to create a new share access every time.

Default behavior:

- Streaming sessions are valid for `3` hours by default
- Administrators can adjust this at `Admin -> System Settings -> Runtime and Scheduling -> Share streaming session TTL`
- The allowed range is `5` minutes to `24` hours
- A download limit counts once per streaming session; segmented player Range requests do not repeatedly increase it

This is not the share link expiration time. The share link's password, expiration time, and maximum download count still apply normally.

## Archive Preview on Share Pages

If the administrator enables share-side archive preview, public share pages can show a read-only listing of a supported archive.

Key points:

- ZIP and 7z
- Shows directories, files, sizes, and modification times
- Does not extract the archive into the user's folder
- Does not provide downloads for individual files inside the archive
- First open may need to wait for the `archive preview generation` background task to finish

If filenames inside the ZIP look garbled, switch `Filename encoding` in the preview toolbar. Common options include `Auto`, `UTF-8`, `GB18030`, `CP437`, `Shift_JIS`, `Big5`, and others. Switching only affects list display and does not modify the archive file.

If the share has a password, visitors must pass password verification before seeing the archive listing. When accessing an archive through a folder share, the system also verifies that the archive really belongs to the shared scope.

Administrators do not enable share-side archive preview by default because it exposes metadata such as internal filenames and directory structure. Enable it from `Admin -> System Settings -> Storage and Retention -> Archive Preview` only when you need this capability.

## File Shares vs Folder Shares

### File Shares

Best for sending a single file. After opening the link, visitors can preview or download it directly.

### Folder Shares

Best for sending a full set of materials. The public page supports:

- Browsing the shared folder
- Entering subfolders
- Returning through breadcrumbs
- Previewing files
- Downloading files
- Switching between list and grid views

## When Shares Become Invalid

The link becomes invalid when any of these happens:

- It reaches the expiration time
- Download count reaches the limit
- You delete the share
- An administrator deletes the share in the admin console

## Can One Item Have Multiple Shares?

In the current version, the same file or folder can have only one active share at the same time.

If you want a new link, you have two options:

- Delete the old link first, then create a new one
- Wait for the old link to expire before creating a new one

If you only want to change the password, expiration time, or download count, you do not need a new link. Edit the existing share directly in `My Shares`.

## How Password Protection Works

If a share has a password, visitors must enter the password before entering the public page.  
After successful verification, the current browser usually remembers it for about 1 hour.

## How to Manage Links You Sent

Regular users can manage shares for the current workspace from the left-side `My Shares` page:

- Copy share links
- Open public pages
- Edit password, expiration time, and download count limits
- Delete shares that are no longer needed
- View open counts and download counts

## What Administrators Can Do

Administrators can view and delete all site-wide share links from `Admin -> Shares`.

Common uses:

- A public link should no longer be accessible
- A share is no longer needed
- You want to check which links are still publicly available
