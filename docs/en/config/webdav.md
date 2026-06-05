# WebDAV

::: tip This page has two layers

- **`[webdav]` in `config.toml`** - Path prefix and hard upload size limit. **Requires a restart after changes.**
- **`Admin -> System Settings -> WebDAV`** - The global switch and system-file blocking rules. Saved changes affect new requests immediately without a restart.

Regular WebDAV users usually only need to create a dedicated account on the `WebDAV` page in the left sidebar of their personal space, then enter the address in Finder, Windows, or rclone.
:::

## Static Configuration in `config.toml`

```toml
[webdav]
prefix = "/webdav"
payload_limit = 10737418240
xml_payload_limit = 1048576
```

| Option | Default | Purpose |
| --- | --- | --- |
| `prefix` | `"/webdav"` | WebDAV path prefix. Client addresses must change with it. |
| `payload_limit` | `10737418240` | Hard WebDAV file write body limit. Default is 10 GiB. |
| `xml_payload_limit` | `1048576` | Hard XML request body limit. Default is 1 MiB; used by `PROPFIND`, `PROPPATCH`, `REPORT`, and `LOCK`. |

::: warning Restart the service after changing these static options
Unlike the runtime global switch, static configuration is read only once during startup.
:::

## Runtime Settings in the Admin Console

Entry point:

```text
Admin -> System Settings -> WebDAV
```

There are three settings:

- **Enable WebDAV**: the global switch. After it is disabled, desktop clients can no longer access files. **No restart is required.**
- **Block WebDAV System Files**: enabled by default. Blocks operating-system metadata files automatically created by Finder, Windows Explorer, and sync tools.
- **Blocked WebDAV System-File Patterns**: matches file or directory basenames, ignores case, and supports simple `*` wildcards.

The default blocked names are:

- `.DS_Store`
- `._*`
- `.Spotlight-V100`
- `.Trashes`
- `.fseventsd`
- `Thumbs.db`
- `desktop.ini`
- `$RECYCLE.BIN`
- `System Volume Information`

These files are usually not content users intentionally want to keep. When blocking is enabled, clients receive `403` if they try to create them through WebDAV. Normal file uploads are unaffected.

::: tip When to change the rules
Most sites should keep the defaults. Change them only if you explicitly want to back up these system metadata files, or if a client repeatedly fails because of a blocked pattern and that affects normal sync behavior.
:::

## Standard Usage for Regular Users

1. Create a dedicated account on the `WebDAV` page in the left sidebar of your personal space
2. Set a username and password
3. Optionally restrict it to a folder under the root directory
4. Enter the address, username, and password in Finder, Windows Explorer, rclone, or Mountain Duck

::: tip Use a dedicated account. Do not reuse the web login password.
A WebDAV dedicated account has independently managed password and scope. Losing it will not affect the main account.
:::

## Default Address

```text
https://your-domain/webdav/
```

The `/webdav/` mount root exists only as an entry point and listing boundary; it is not a real folder row in the database. Clients may use `PROPFIND` on it to list the root directory, but `PROPPATCH /webdav/` cannot write custom dead properties and explicitly returns `403 Forbidden`. Custom properties must target a concrete file or folder.

If you change `prefix` to `/dav`, change the client address too:

```text
https://your-domain/dav/
```

## Large Uploads Depend on Three Limits

When uploading large files through WebDAV, these three limits apply, and **the smallest one wins**:

1. `webdav.payload_limit`
2. Reverse proxy upload size limit, such as Nginx `client_max_body_size` or Caddy equivalents
3. Single-file size limit in the storage policy

If any one of them blocks the upload, the whole upload is blocked. Check all three while troubleshooting.

`xml_payload_limit` does not limit file content uploads. It only limits XML control requests. Most deployments do not need to adjust it unless a client sends unusually large directory queries, lock requests, or property updates.

## Do Not Drop These When Using a Reverse Proxy

::: warning WebDAV is not only GET/PUT
WebDAV uses a set of extension methods and headers that reverse proxies often drop by default. Make sure the proxy layer forwards:

**Headers:** `Authorization`, `Depth`, `Destination`, `Overwrite`, `If`, `Lock-Token`, `Timeout`

**Methods:** `PROPFIND`, `PROPPATCH`, `MKCOL`, `MOVE`, `COPY`, `LOCK`, `UNLOCK`
:::

See [reverse proxy deployment](/en/deployment/reverse-proxy) for complete reverse proxy examples.
