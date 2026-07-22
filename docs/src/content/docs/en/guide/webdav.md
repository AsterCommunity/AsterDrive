---
description: How to connect to AsterDrive WebDAV, which protocol features are implemented, how workspaces are projected, and the current filename, same-name resource, property, lock, and DeltaV limits.
title: "WebDAV Features and Limits"
---

:::tip[The short version]
AsterDrive WebDAV is a protocol view of a personal or team workspace, not a separate filesystem. Resources uploaded, moved, copied, or deleted through WebDAV continue to use AsterDrive's workspace, storage-policy, quota, version-history, and audit paths.
:::

## Prepare an Account Before Connecting

The default WebDAV mount address is:

```text
https://your-domain/webdav/
```

To connect:

1. Create a dedicated WebDAV account in the personal or team workspace you want to access.
2. Save the username and password returned during creation. The plaintext password is shown only once.
3. Enter the mount address, username, and password in your WebDAV client.
4. If the account has a root-folder restriction, the client sees only that folder and its descendants.

WebDAV mounts use **Basic Auth with dedicated WebDAV credentials**. A Bearer JWT from the web login is not a WebDAV mount credential, and you do not need to give the client your web-login password.

A personal account enters only its matching personal space. A team account enters only its matching team space and remains subject to team membership, role, and workspace permissions.

For the global switch, path prefix, body limits, and system-file blocking rules, see [WebDAV Configuration](/en/config/webdav/).

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

## Filenames Must Follow URL-Encoding Rules

A WebDAV path is a URI. It is not an operating-system filename appended to a string without encoding. Reserved characters in filenames must be percent-encoded by the client.

For example, Windows allows this filename:

```text
report#draft.txt
```

Its WebDAV URL representation is:

```text
/webdav/report%23draft.txt
```

`#` starts a URI fragment. The following form does not represent a `#` inside the filename:

```text
/webdav/report#draft.txt
```

Common WebDAV clients remove a real fragment before sending a request and encode a `#` that belongs to a filename as `%23`. AsterDrive covers `%23` filename upload/download round trips. If a non-standard request target is deliberately sent with a raw `#fragment`, the underlying HTTP parser may truncate the fragment before AsterDrive processes it. Do not use that form to represent a filename. This parser boundary is tracked in [GitHub #424](https://github.com/AsterCommunity/AsterDrive/issues/424).

## Limit When a File and Folder Have the Same Name

:::caution[WebDAV has one URI namespace]
The AsterDrive product model currently permits a file and a folder under the same parent to have the same name. A WebDAV href can stably identify only one resource. These models are not fully equivalent.
:::

Suppose the same parent contains both:

```text
report        # file
report/       # folder
```

In the WebDAV view, `/report` and `/report/` are not suitable as identifiers for two independently manageable resources. When such a conflict already exists, AsterDrive's WebDAV path resolver gives the folder precedence, so the file with the same name may be hidden in the WebDAV view.

WebDAV writes preserve this single namespace where the method semantics allow it:

- `MKCOL` over an existing file returns `405 Method Not Allowed`;
- `MKCOL` over an existing collection also returns `405 Method Not Allowed`;
- `PUT` over an existing collection returns `405 Method Not Allowed`;
- `COPY` and `MOVE` treat the destination href as one resource and apply `Overwrite` semantics to an existing target.

If same-name objects were created through the web UI, REST API, or an older release, WebDAV does not rename or delete them automatically. Such a directory is a lossy projection, and the file may be unreachable from a WebDAV client. Avoid same-name file/folder pairs in directories that need reliable WebDAV synchronization.

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

Before launch, validate the client version you actually plan to use:

1. List the root and at least two nested directory levels.
2. Upload, download, and rename regular files and filenames containing spaces, non-ASCII characters, and `#`.
3. Test large-file limits, range downloads, and retry behavior after a dropped connection.
4. Test copy, move, delete, and overwrite behavior.
5. Open the same file from multiple clients and confirm lock/conflict messages are acceptable.

## Do Not Let the Reverse Proxy Break WebDAV

WebDAV uses more than `GET` and `PUT`. The reverse proxy must pass extension methods and their request headers, especially:

- Methods: `PROPFIND`, `PROPPATCH`, `MKCOL`, `COPY`, `MOVE`, `LOCK`, `UNLOCK`, `REPORT`, `VERSION-CONTROL`;
- Headers: `Authorization`, `Depth`, `Destination`, `Overwrite`, `If`, `Lock-Token`, `Timeout`.

The proxy may also impose its own request-body limit, timeout, buffering, and path-encoding behavior. When small files work but large files fail, directory creation fails while downloads work, or special filenames change, compare direct AsterDrive access with access through the proxy.

See [Reverse Proxy](/en/deployment/reverse-proxy/) for complete examples.

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
