---
description: "AsterDrive WebDAV protocol compatibility reference: implemented methods and capabilities, PROPFIND / COPY / MOVE boundaries, lock and DeltaV limits, client validation scope, and the limits quick table."
title: "WebDAV Protocol Compatibility"
---

:::tip[This page is the protocol-layer authority]
For account creation, mounting, filename encoding, and same-name limits from the user side, see [Using WebDAV](/en/using/webdav/); for the global switch, path prefix, and `config.toml` static fields, see [WebDAV Configuration](/en/reference/config/webdav/); for runtime switches and system-file blocking, see [WebDAV (System Settings)](/en/reference/config/runtime/webdav/).
:::

## Implemented Protocol Features

| Category | Methods or capability | Current behavior |
| --- | --- | --- |
| Capability discovery | `OPTIONS` | Returns supported methods and DAV capability declarations |
| Downloads | `GET`, `HEAD` | Supports ETag, `Last-Modified`, conditional requests, and byte `Range`; range reads return `206` |
| Uploads | `PUT` | Creates or overwrites files while applying conditional-header, lock, quota, and storage-policy checks |
| Resource management | `MKCOL`, `DELETE`, `COPY`, `MOVE` | Creates collections, deletes, copies, and moves resources, with `Destination`, `Overwrite`, and related precondition handling |
| Properties | `PROPFIND`, `PROPPATCH` | Reads live properties and stores dead properties on concrete files or folders |
| Locks | `LOCK`, `UNLOCK` | Database-backed exclusive/shared write locks with `If` and `Lock-Token` handling |
| Minimal DeltaV | `VERSION-CONTROL`, `REPORT` | Supports file `DAV:version-tree` reports generated from AsterDrive file versions |

`GET` streams directly from the storage driver that owns the file. WebDAV does not bypass storage policies: data may still be on local disk, S3-compatible object storage, Azure Blob, OneDrive, or a remote follower node, depending on the workspace's active storage policy.

## `PROPFIND` and Property Boundaries

- A missing `Depth` header is parsed as `infinity`.
- `Depth: infinity` on a collection returns `403 Forbidden` with `DAV:propfind-finite-depth`; the server does not perform unbounded recursive enumeration.
- `Depth: infinity` on a file is handled as a single-resource request.
- `/webdav/` is a virtual mount root, not a persisted folder row. It supports `PROPFIND`, but `PROPPATCH` on the root returns `403 Forbidden`.
- Custom dead properties are stored only on concrete files or folders. Properties in the protected `DAV:` namespace are controlled by the server.

Clients should use `Depth: 1` to list a directory. Do not use the WebDAV mount as an unbounded recursive workspace-enumeration API.

Regular WebDAV clients generate the correct XML automatically, so you do not need to configure the following rules manually. They matter only when you write a script or implement a protocol client:

- `prop`, `allprop`, `propname`, and `include` must belong to the `DAV:` namespace; a same-named element without that namespace is not equivalent;
- an empty request body is treated as `allprop`; once non-empty XML is sent, the body must explicitly select one of `prop`, `allprop`, or `propname`;
- `include` may appear only once and only together with `allprop`;
- after a valid selector is present, other extension elements are ignored according to WebDAV rules instead of making the whole request fail merely because the server does not recognize them; a body containing only unknown elements still has no valid selector and is rejected.

In a handwritten request, declare `xmlns="DAV:"` or use a prefix bound to `DAV:` on the relevant elements. If a regular client suddenly cannot list directories, capture the actual request body and confirm that the reverse proxy did not rewrite the XML.

## `COPY` / `MOVE` Boundaries

- `Destination` must use the same origin as the current WebDAV server and remain under the current WebDAV path prefix.
- Cross-server `COPY` and `MOVE` are outside the current scope.
- `COPY` accepts `Depth: 0` or a missing / `infinity` depth and explicitly rejects `Depth: 1`.
- For a folder, `COPY Depth: 0` copies only the folder itself and its dead properties, not its children.
- Requests apply ETag conditions, `If` / `Lock-Token`, and `Overwrite` handling.

## Lock and DeltaV Limits

AsterDrive supports persistent exclusive/shared write locks and checks relevant lock conditions before move, copy, delete, and overwrite operations. Expired locks are cleaned up, and administrators can remove abnormally retained locks from the admin console.

A collection lock created with `Depth: infinity` covers descendant resources. When a client operates on a descendant and submits the same lock token in the `If` header according to WebDAV rules, AsterDrive validates it against the locked collection's own href instead of treating the valid parent lock token as unauthorized.

Current boundaries:

- DeltaV is a minimal subset: `VERSION-CONTROL` and file `REPORT DAV:version-tree`. AsterDrive's own version history is not a complete RFC 3253 version-control server.
- `REPORT version-tree` supports files, not folders.

## How to Read Client Compatibility Claims

The repository uses three layers of WebDAV checks:

1. Rust protocol regression tests;
2. a pinned Litmus 0.18 Phase 0 baseline;
3. real-client workflows for rclone, curl, and cadaver.

Regression coverage also includes common Finder-style `PUT` shapes, special filenames, ranges, conditional requests, locks, and property operations. This means compatibility behavior has repeatable checks; it does not mean every operating system, client, and version combination has been fully certified.

## Limits at a Glance

| Scenario | Current result | Recommendation |
| --- | --- | --- |
| Filename contains `#` | Supported; the URI must contain `%23` | Use a normal WebDAV client; do not hand-write a raw fragment |
| Same-name file/folder siblings | Allowed by the product model; ambiguous in WebDAV, with folder precedence | Avoid these pairs in WebDAV-synchronized directories |
| Collection `PROPFIND Depth: infinity` | `403` with `DAV:propfind-finite-depth` | Use `Depth: 1` for directory listings |
| Mount-root `PROPPATCH` | `403` | Store custom properties only on concrete files/folders |
| Cross-server `COPY` / `MOVE` | Destination is rejected | Download/upload or use client-side synchronization |
| Recursive collection locks | A `Depth: infinity` lock covers descendants, and descendant operations may submit the parent collection lock token | Confirm that the client keeps sending the lock token in the `If` header |
| Complete DeltaV | Only a minimal file version-tree subset is implemented | Manage full version history in the AsterDrive web UI/API |
