---
description: "AsterDrive files and organization: file page layout, organizing uploads, drag and drop, search and filters, tags, multi-select batch operations, cross-workspace copy and move, and file details."
title: "Files and Organization"
---

:::tip[What this page covers]
The things you use every day on the file page: directory tree, search, tags, batch operations, and cross-workspace copy/move. Uploading and downloading themselves are in [Upload and Download](/en/using/upload-download/).
:::

## Most-Used Spots on the File Page

- Left workspace list: switch between personal and team spaces
- Left directory tree: jump to a target folder quickly
- Top search box: search files and folders in the current workspace
- `Recycle Bin`: handle deleted content in the current workspace
- `My Shares`: view links already created in the current workspace
- `Task Center`: view background tasks such as online compression, extraction, and archive downloads
- `WebDAV`: create desktop client accounts for the current workspace; personal and team spaces manage their own accounts
- `Settings` in the top-right user menu: adjust profile, interface, security, and team settings

## Organizing Files

The file list, right-click menu, and top action area cover most daily work:

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
- Online compression, online extraction, and folder archive downloads
- Switch between list and grid views
- Sort by name, size, creation time, update time, or type

You can also drag and drop:

- Drag files or folders onto a target folder in the left directory tree
- Drag files or folders onto a parent directory in the top breadcrumb
- Drag files or folders onto the recycle bin on the left

## Search, Multi-Select, and Batch Operations

The top search box searches files and folders by name in the current workspace. Click the search box directly, or press `Ctrl + K` to open the search panel; the panel can switch between "All / Files only / Folders only", filter quickly by image, video, music, document, spreadsheet, presentation, archive, code, and other categories, and select one or more tags.

After entering a keyword or choosing filters, press `Enter` or click `Search` to enter the current workspace's result page. From the results you can open files, jump to locations, view information, or run available actions; team and personal results are never mixed.

Tag filtering has two modes:

- `Match any`: items with any selected tag appear
- `Match all`: only items carrying every selected tag appear

The left sidebar also has quick entries for common types such as images, videos, music, and documents. Clicking one is still essentially a category search in the current workspace.

To process many items at once, multi-select and run batch operations:

- Batch move
- Batch copy
- Batch delete
- Batch archive download

Use `Ctrl + A` or `Cmd + A` to select everything on the current page in both the file list and the recycle bin.

### Copying or Moving Across Workspaces

When copying or moving, the target picker can switch workspaces. A common flow:

1. Select the files or folders in the current workspace.
2. Click `Copy to` or `Move to`.
3. Choose the target workspace, then open the target folder.
4. Confirm to finish.

Copy keeps the original files; after a move succeeds, the originals go to the source workspace's recycle bin. Folders are processed together with their contents.

Things to note:

- You must have access to both the source and target workspaces.
- If the target space lacks capacity, the move does not delete the source files.
- Locked files or folders cannot move across workspaces.
- If an item with the same name exists at the target, the system appends a copy number to the new item's name.
- When multiple items are selected, the page shows success and failure counts; for failures, refresh both spaces before continuing.
- Moving within the same workspace is just a normal directory move and creates no copies.

## How Tags Work

Tags belong to the current workspace. Tags in your personal space do not automatically appear in team spaces, and tag libraries are independent between teams.

Manage tags from the right-click menu, action menu, or details panel of a file or folder:

- Add or remove tags on a single file or folder
- Multi-select items to add or remove tags in batch
- Open the `Tag Library` to create, rename, recolor, or delete tags
- Filter results by tag in search

If a tag search finds no match, you can create a new tag on the spot. After creating it, still confirm the tag change on the current item; it is only written once the page shows "saved".

Deleting a tag removes it from every file and folder using it in the current workspace; it does not delete the files themselves. To take a tag off only a few files, use "remove tag" instead of deleting the tag from the library.

## What the Details Panel Shows

The "details" of a file or folder show name, size, occupied space, type, creation time, modification time, lock status, share status, storage policy ID, and more. When investigating "which policy is this file actually on" or "is it locked", look here first.

"Size" and "occupied space" are not the same concept:

- File size is the size of the current file itself
- File occupied space also counts historical versions
- Folder occupied space recursively counts everything under it, useful for finding which directory consumes the most quota
