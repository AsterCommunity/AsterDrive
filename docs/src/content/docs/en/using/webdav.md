---
description: "Using AsterDrive WebDAV: dedicated accounts, the mount address, filename URL encoding, same-name file/folder limits, a client validation checklist, and reverse proxy pitfalls."
title: "Using WebDAV"
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

For the global switch, path prefix, body limits, and system-file blocking rules, see [WebDAV Configuration](/en/reference/config/webdav/); for implemented protocol methods, property, lock, and DeltaV boundaries, see [WebDAV Protocol Compatibility](/en/reference/webdav-compat/).

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

## Validate with Your Real Clients Before Launch

For the in-repo protocol regression tests and compatibility baselines, see [WebDAV Protocol Compatibility](/en/reference/webdav-compat/). Before launch, validate the client version you actually plan to use:

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

See [Reverse Proxy](/en/deploy/reverse-proxy/) for complete examples.
