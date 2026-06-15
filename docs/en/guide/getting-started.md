# Getting Started

Welcome to AsterDrive.

This is **self-hosted cloud storage**: you install it on your own server, and your files are truly yours, not in the hands of any third party.

It fits three kinds of people:

- People who want to replace commercial cloud drives without stitching together a pile of open-source components
- People who want to share photos and videos with family, in a way parents can also understand
- Small teams that need something **light, fast, and changeable**

If this is your first self-hosted service, AsterDrive's default configuration can run out of the box. Within 10 minutes, you can see your first file sitting on your own server. We tried to make that possible without requiring you to become an operations expert first.

If you already know Docker and reverse proxies, you can jump directly to [Deployment Overview](/en/deployment/).

AsterDrive does not have a paid edition, Pro edition, or feature wall. Every feature is open source under the MIT license, and everyone gets the same capabilities. For the project's tradeoffs and future direction, read [About AsterDrive](./about).

Let's start.

## 1. Start the Service

The simplest path is to run the official Docker image directly. The image runs as the non-root user `aster` (UID/GID `10001:10001`), so prepare the host directory and align ownership before mounting it into the container:

```bash
mkdir -p ./data
sudo chown -R 10001:10001 ./data

docker run -d \
  --name asterdrive \
  -p 3000:3000 \
  -e ASTER__SERVER__HOST=0.0.0.0 \
  -e ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES=true \
  -e ASTER__DATABASE__URL="sqlite:///data/asterdrive.db?mode=rwc" \
  -v "$(pwd)/data:/data" \
  ghcr.io/apts-1547/asterdrive:latest
```

This command is suitable for local, LAN, or temporary HTTP-only trials.

- If you already have HTTPS ready for public access, you can remove `ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES=true`
- In production, keep `bootstrap_insecure_cookies = false` and keep the Cookie security switch enabled in backend system settings

::: tip Why chown to 10001 is required
Inside the image, the `aster` user with UID `10001` writes to `/data`. If you bind mount a directory owned by the current shell user directly into the container, the container cannot write `config.toml`, the SQLite file, or temporary directories, and it will exit with a permission error.

For more detailed commands and Compose examples, see [Docker Deployment](/en/deployment/docker).
:::

::: details Why is SQLite the default instead of PostgreSQL?
SQLite gives you zero operations, single-file backup by direct copy, and an almost nonexistent trial barrier.

Our judgment is: **letting you see "what this project is" in 5 minutes** matters more than **forcing you to install a database first**.

After production launch, if your data keeps growing, you can switch to PostgreSQL later. AsterDrive ships with a [cross-database migration tool](/en/deployment/ops-cli) so the initial database choice does not trap you.
:::

After the primary instance starts successfully for the first time, it automatically prepares:

- `config.toml` under `data/` in the current working directory
- Database creation or connection, with automatic schema updates
- Default local storage policy `Local Default`
- Default policy group `Default Policy Group`
- Local upload directory `data/uploads`
- Temporary directories `data/.tmp` and `data/.uploads`
- Default background settings and required background tasks

If you start a follower node, it follows a different flow. The follower-node chapter covers that separately.

If you use the official Docker image and mount `./data:/data` as shown above:

- The database and upload directory land in `/data` inside the container, corresponding to `./data/` on the host
- `config.toml` is generated at `/data/config.toml` inside the container, corresponding to `./data/config.toml` on the host, and you can edit it directly on the host

Open:

```text
http://server-address:3000
```

## 2. Create the First Administrator Account

After you open the site in a browser, the login page automatically decides the flow based on the username or email you enter:

- There are no users in the system: create the administrator account
- The input is an existing account: log in
- The input is a new account and public registration is allowed: register a regular account

The first successfully created account automatically becomes an administrator.

The first administrator account is created directly and is usable immediately.  
Later regular users created through public registration must complete email activation first.

If you plan to expose the service directly to the public internet, confirm at least:

1. Whether public registration should really be enabled
2. Whether mail delivery and the public site URL are configured

## 3. Run the Basic Usability Check

After logging in as an administrator, complete these steps in `My Space`:

1. Create a test folder
2. Upload a small file
3. Open the file and confirm it can be previewed, edited, or downloaded
4. Delete the file to the trash
5. Restore it from the trash

If all of these work, the browser side, database, and default storage route are basically usable.

We consider **trash one of AsterDrive's most important features**. Without it, you would not dare to put important files inside. So during the first run, verify "mistaken deletion can be recovered" yourself.

## 4. Try a Share

Create a share link from the action menu of a file or folder, and set as needed:

- Password
- Expiration time
- Maximum download count

Send the link to an incognito window, phone, or another device, and confirm the public page opens normally.

## 5. If You Need WebDAV, Make a Real Connection

If you plan to connect Finder, Windows Explorer, rclone, or sync tools:

1. Switch back to `My Space`, then open `WebDAV` on the left
2. Create a dedicated WebDAV account
3. Copy the WebDAV address, username, and password
4. Perform a real read/write test in the client

The password is shown only once after successful creation. Save it to a password manager immediately.

## 6. Places to Check After the First Admin Login

- `Admin -> Overview`
- `Admin -> Users`
- `Admin -> Teams`
- `Admin -> Storage Policies`
- `Admin -> Policy Groups`
- `Admin -> Files` and `Admin -> File Blob`
- `Admin -> Tasks`
- `Admin -> System Settings`
- After returning to your personal space, left-side `WebDAV`

If you plan to connect follower nodes, also check `Admin -> Follower Nodes`.

Focus on confirming:

- The default storage policy and default policy group have been created
- The default quota for new users is appropriate; if you create teams, verify the actual team quota and default policy group after creation
- If the site will be publicly accessible, `Public site URL` is set to the real HTTP(S) origin; add each public entry point separately
- If registration, password recovery, or email change will be enabled, test mail delivery
- If you will connect WOPI services such as OnlyOffice, `Public site URL` and `Site Configuration -> Preview Apps` are configured, and the external service can call back to AsterDrive's `/api/v1/wopi/...`
- If online compression, online extraction, or other background tasks will be used, `Admin -> Tasks` has no recent continuous failures
- If image / video thumbnails will be used, the processors in `File Processing -> Media Processing` fit the current server environment
- Trash retention days, version count, and team archive retention days match expectations
- Whether WebDAV should stay enabled
- Whether files should continue using the default local policy or move to another storage policy backend; if using follower nodes, whether the follower already has a default ingress target
- If you plan to migrate existing files to a new policy, run `Admin -> Storage Policies -> Migrate Data` and check the plan first, then watch progress under `Admin -> Tasks`; use `Admin -> Files` and `Admin -> File Blob` to spot-check results when needed
- Whether the Gravatar avatar URL is reachable from the current network

## 7. Validate Before Production Launch

The full checklist is in [First-Start Checklist](/en/deployment/runtime-behavior#check-these-items-immediately-after-startup).

For getting started, at least run through:

- Browser login and logout work normally
- Files can be uploaded, downloaded, deleted to trash, and restored
- Share links open normally
- `http://server-address:3000/health` and `/health/ready` return normal responses

After this check passes, continue with domain name, HTTPS, reverse proxy, backup, and upgrade planning.

If you also want an offline command-line check, run `doctor` from [Operations CLI](/en/deployment/ops-cli).

## What Next?

After the basic flow works, you may want to do these next:

- Launch for real, add HTTPS, and configure a reverse proxy -> [Deployment Overview](/en/deployment/) / [Reverse Proxy](/en/deployment/reverse-proxy)
- Build a primary + follower-node deployment -> [Follower Nodes](./remote-nodes)
- Learn what daily use looks like -> [User Manual](./user-guide)
- Go deeper into the admin console -> [Admin Console](./admin-console)
- Connect Office online editing -> [File Editing](./editing)
- Hit a problem -> [Error Code Handling](./errors) / [Troubleshooting](/en/deployment/troubleshooting)

Or, if you want to understand the project itself first: [About AsterDrive](./about).

**Do not add mental overhead to your own data**. That is why we are building AsterDrive.  
Enjoy using it.
