---
description: Follower node concepts and management in AsterDrive, clarifying primary vs follower role boundaries, internal storage protocol versions, enrollment path selection, and the semantics of management operations like disabling.
title: "Follower Nodes"
---

:::tip[What this page covers]
This page covers the **concepts and management boundaries** of follower nodes: what the primary and follower each are, how protocol versions stay compatible, and what happens when you disable a node link.

The full enrollment deployment flow lives in the scenario page [Follower Storage Node Deployment](/en/deploy/follower-node/); policy group routing and acceptance for remote policies are in the [Remote Follower Storage Policy Tutorial](/en/admin/storage-backends/remote-follower/).
:::

## Concepts First

AsterDrive's remote-node capability essentially lets **another AsterDrive instance** act as a storage backend.

- **Primary node**: handles login, frontend, admin console, shares, WebDAV, storage policies, and remote-node management
- **Follower node**: only exposes `/health`, `/health/ready`, and the internal remote storage protocol; it accepts object requests signed by the primary node, then writes objects to a follower local directory or S3 according to the **remote storage target** pushed by the primary

The current internal remote storage protocol version is `v5`, and the current primary supports followers whose compatible range overlaps `v4` through `v5`. When the primary tests connectivity and binds remote policies, it compares the declared version ranges and reads the follower's server version, object read/write capabilities, Range capabilities, compose capabilities, metadata capabilities, and the CORS contract required for browser direct upload. Followers limited to `v2` or `v3` must be upgraded first.

By default, AsterDrive runs in `primary` mode.
It becomes a follower node only after `[server].start_mode` is changed to `follower`.

:::caution[A Follower Is Not a Primary]
A follower node is not a second login site or a second admin console.

It has only one goal: **provide a remote object storage target for the primary node**.
The control plane may be one Primary in the single profile or multiple Primaries sharing the database, Redis, and storage in the cluster profile. The Follower itself does not serve normal user traffic or provide a separate control plane. See [Load Balancing and Multi-Instance Deployments](/en/deploy/multi-instance/) for multi-Primary requirements.
:::

## Which Enrollment Path to Take

| Your situation | Where to go |
| --- | --- |
| First time connecting a follower; want the full walkthrough | [Follower Storage Node Deployment](/en/deploy/follower-node/) |
| Deciding between public HTTPS, Tailscale / VPN, a Docker network, or a reverse tunnel | [Follower Node Network Topologies](/en/deploy/follower-node/network/) |
| Already enrolled; need policy group routing and acceptance | [Remote Follower Storage Policy Tutorial](/en/admin/storage-backends/remote-follower/) |

The two enrollment paths differ only in how enroll completes:

- **Docker auto-enroll**: the container reads one-time `ASTER_BOOTSTRAP_REMOTE_*` environment variables at startup and enrolls automatically — recommended
- **Manual enroll**: run `aster_drive node enroll` in the follower's working directory, then restart the service

Both paths end with the same shared sequence on the primary: "test connection -> default remote storage target -> remote storage policy".

## Common Judgment Questions

### Can the Follower Node Be Opened for Regular User Login?

No. In the current design, follower nodes are not regular user login entries.

`follower` mode only exposes:

- `/health`
- `/health/ready`
- Internal remote storage API

### Can a Remote Storage Target Use Another remote Policy?

No.
When a follower receives inbound objects, the target must be directly writable on the follower side, such as `local` or `s3`; it cannot wrap another `remote` layer.

### Can `base_url` Be Empty and Still enroll?

Yes, but it depends on the transport mode:

- `direct`: you can only save the record and complete enrollment. Without `base_url`, the primary cannot test connectivity or send remote storage traffic.
- `reverse_tunnel`: after the follower restarts, it actively connects to the primary. Once the tunnel is online, you can test connectivity, push remote storage targets, and use `relay_stream` remote policies.
- `auto`: empty `base_url` behaves like reverse tunnel; a non-empty `base_url` behaves like direct.

For any mode, remote `presigned` upload/download requires direct transport and a follower `base_url` reachable from the browser.

### Why Restart After enroll Succeeds?

Because the current version only writes the binding into the database and does not hot-refresh the running follower process.
**Write succeeded does not mean it has taken effect**. It starts receiving traffic only after restart. The Docker auto-enroll path completes the binding during first startup and is not subject to this limitation.

### What Happens When a Remote Node Is Disabled?

Remote policies on the primary stop using it, and the follower also rejects corresponding signed inbound requests.
Disabling actually stops the link; it does not merely hide the admin record.

### Can One Follower Bind to Multiple Primaries?

No. A follower binds to one AsterDrive control-plane identity.
Multiple Primaries in a cluster share one control plane (database, static secrets, and one public/LB entry) and can share followers; independent AsterDrive deployments cannot bind the same follower at the same time.
