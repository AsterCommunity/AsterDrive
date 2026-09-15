---
title: "Load Balancing and Multi-Instance Deployments"
description: AsterDrive multi-primary load-balancing contract covering shared dependencies, storage and upload limits, configuration synchronization, health checks, task coordination, and launch validation.
---

:::caution[Declare the cluster profile first]
Put Primary instances into the same load-balancer upstream only after every Primary sets `[deployment].profile = "cluster"` and passes startup, topology, and readiness checks. A Follower is a remote storage node, not a Primary replica for normal user traffic.
:::

## Supported Topology

```text
Browser / WebDAV / WOPI / Follower tunnel
                    |
              Ingress / LB
             +------+------+
          Primary A     Primary B
             +------+------+
        PostgreSQL / MySQL + Redis
                    |
          Shared storage / remote Follower
```

The load balancer may send each new HTTP request to any ready Primary. Supported AsterDrive multi-instance paths do not require sticky sessions for correctness: authentication and business metadata use the shared database, cache and cross-instance notifications use Redis, and file content must live on a data plane reachable by every Primary.

An established long-lived connection still remains on one Primary. When that Primary exits, SSE, WebDAV requests, in-progress HTTP bodies, and reverse-tunnel connections must reconnect or retry at the client; a load balancer does not move an established connection seamlessly to another instance.

## What Every Primary Must Share

| Item | Requirement |
| --- | --- |
| Authoritative database | Use the same PostgreSQL or MySQL database; cluster rejects SQLite |
| Cache | Set `[cache].backend = "redis"` and use the same shared cache |
| Configuration and event notifications | Set `[config_sync].backend = "redis"` and use the same endpoint and topic on every instance |
| Storage data plane | The default policy and every policy reachable by users or teams must be accessible from every Primary |
| Static secrets | Keep `jwt_secret`, `share_cookie_secret`, `direct_link_secret`, `mfa_secret_key`, and `storage_credential_secret_key` identical on every Primary |
| Internal proxy secret | When reverse tunnels are used, every Primary must use the same `internal_proxy_secret` |

`config_sync` only tells other processes to reload runtime settings from the database. It does not synchronize `config.toml`, environment variables, or Secrets. Except for instance-specific values such as listen addresses, instance names, and `deployment.internal_endpoint`, use one static configuration template for all Primaries.

Uploaded user avatars still live under `avatar_dir`. If avatar uploads are enabled, mount that directory as shared read-write storage at the same path on every Primary. Otherwise, an avatar uploaded through Primary A may not be readable through Primary B. Gravatar does not use this directory.

## Storage and Upload Limits

Cluster mode permits the existing `local` storage policy as an operator-managed data plane. Every Primary must mount the policy `base_path` from the same shared filesystem and configure `server.upload_temp_dir` as staging visible to every Primary. The two paths may live on separate shared filesystems; they do not need to be one mount.

The shared filesystems must provide the cross-instance visibility, `fsync`, atomic rename/delete, and advisory-lock semantics AsterDrive uses. AsterDrive does not infer a correct mount from identical path strings or an RWX declaration, and a one-time startup probe does not certify the backend's long-term semantics. Separate Pod-local volumes violate the contract even when their path strings match.

| Storage path | Cluster behavior |
| --- | --- |
| S3-compatible, Tencent COS, Azure Blob | Connector-native multipart, presigned, and browser-direct uploads are available |
| OneDrive | Provider-resumable server relay and browser-direct uploads are available |
| Remote Follower | Relay and presigned uploads are available; reverse tunnels also require internal proxy configuration |
| SFTP | Available when every Primary reaches the same service; resumable stream staging also requires shared `upload_temp_dir` |
| `local` policy | Available; the policy `base_path` and `upload_temp_dir` must each be shared by every Primary |

The key boundary is ownership of temporary upload state, not file size. Offset/stream staging keeps durable receipts, counters, and completion state in the shared database and content in shared `upload_temp_dir`; exclusive same-chunk writes rely on cross-instance advisory locks supplied by the deployed filesystem. Connector-native multipart, presigned, browser-direct, and remote relay/presigned paths keep temporary content in the provider data plane instead.

Do not use sticky sessions or identical path strings to disguise separate Pod-local volumes. Normal driver/readiness checks report path access failures, while mount identity, locking, and durability semantics remain deployment acceptance responsibilities. See [Uploads and Large Files](/en/using/upload-download/#choosing-uploads-for-a-cluster-deployment) for upload selection and diagnosis.

## Configuration, Events, and Consistency

Runtime settings, storage policies, policy groups, storage credentials, remote-node topology, and user policy-group bindings are authoritative in the database. After the writer commits its database transaction, Redis tells other instances to reload the relevant snapshot. Cross-instance Storage SSE uses a separate Redis topic from configuration reloads.

Redis pub/sub does not retain message history. When a Primary loses the subscription, clients receive `sync.required` and refresh from authoritative APIs. After recovery, the instance reloads the full runtime configuration and storage topology before continuing with new events. See [Configuration Synchronization](/en/reference/config/config-sync/#what-happens-when-redis-fails) for the complete failure behavior.

## Reverse-Tunnel Routing

The reverse-tunnel WebSocket, lanes, and pending requests remain on the owner Primary that accepted the connection. The shared database stores the owner lease and fencing token. When a request reaches a non-owner Primary, AsterDrive forwards it to the owner through an authenticated streaming proxy.

Each Primary's `deployment.internal_endpoint` must be an absolute HTTP(S) URL directly reachable by the other Primaries and uniquely identify that instance. Do not configure every instance with the same public load-balancer URL, because internal forwarding may land on a non-owner again. Every Primary must use the same `internal_proxy_secret` of at least 32 characters, and internal endpoints should be restricted to a trusted network.

Leaving both values empty creates a direct-only cluster. Direct Followers remain available, but an enabled reverse tunnel or an `auto` node with an empty `base_url` fails topology validation.

## Health Checks and Traffic Removal

| Probe | Purpose | Cluster behavior |
| --- | --- | --- |
| `/health` | Liveness | Reports process liveness; it remains `200` during a temporary Redis outage |
| `/health/ready` | Readiness | Always checks the database, active Redis cache, and topology; runs the default storage driver's lightweight readiness check after setup completes |

Kubernetes, Ingress controllers, and other load balancers should add only Primaries with a successful `/health/ready` response to the upstream. Do not use `/health` as readiness. If Redis initialization fails, the Forge cache constructor returns the error and terminates startup so the orchestrator can retry after Redis recovers. A Redis backend created successfully keeps liveness during a temporary runtime outage, fails readiness, reconnects, and returns to ready automatically.

A fresh database creates no default storage policy in either single or cluster. While base dependencies are healthy, `/health/ready` returns `200` during setup with a status of `needs_admin` or `needs_storage`, allowing an administrator to finish setup through the normal load-balanced entry. After a shared policy becomes the default and the administrator policy-group assignment is reconciled, the status changes to `ready`; subsequent failures from the default driver's lightweight readiness check return `503`. The probe does not perform object I/O against remote storage, so monitor real data-plane availability separately.

## Migrations, Scheduler, and Background Tasks

Every Primary may run migrations during startup. PostgreSQL uses a transaction-scoped advisory lock and MySQL uses a named lock to serialize the migration-history check and DDL. The database account still needs DDL permissions.

Scheduled work uses a shared database lease to elect one owner, and a standby takes over after the owner exits or its lease expires. Normal background tasks use database claims, leases, and fencing tokens to prevent two Primaries from committing the same result. After an instance failure, work that used local temporary files may restart from a retry point instead of continuing from the interrupted byte position.

## Rate Limits Are Not Global Counters

AsterDrive's HTTP Governor and WebDAV IP token buckets count independently inside each process. The same client distributed across two Primaries may consume two burst allowances, so the application configuration is not a cluster-wide global limit.

Enforce strict global entry quotas at the Ingress, API gateway, or load-balancer layer. AsterDrive's internal limits can remain as per-instance protection. At either layer, configure `network_trust.trusted_proxies` correctly and trust only the last proxy hop that actually connects to AsterDrive. See [Rate Limiting](/en/reference/config/rate-limit/#multi-instance-counting-boundary).

## Load-Balancer Requirements

- Preserve the real `Host` and public protocol, and pass client IPs through a trusted proxy chain.
- Support streaming for SSE, WebDAV, downloads, and uploads, and disable buffering that breaks streaming requests.
- Support WebSocket Upgrade when reverse tunnels pass through this entry.
- Use sufficiently long read, write, and idle timeouts for uploads, downloads, SSE, WebDAV, and WOPI.
- Set request-body limits high enough for real uploads and WebDAV writes.
- Add only ready instances to the upstream, remove traffic before termination, and then perform graceful shutdown.
- Do not use sticky sessions to hide shared-state or incorrect filesystem/staging mounts.

See [Reverse Proxy](/en/deploy/reverse-proxy/) for proxy examples and request-header details.

## Launch Validation

Complete at least these checks:

1. Start two Primaries concurrently, verify that a fresh database runs migrations once and both instances enter the Service as `needs_admin`; create the administrator and default shared storage through the load-balanced entry, then verify both instances become `ready`.
2. Repeatedly log in, refresh tokens, create folders, upload, download, and use WebDAV through the load-balanced entry, confirming correctness when requests switch instances.
3. Change runtime settings, storage policies, policy groups, and user bindings through Primary A, then verify Primary B uses the new state without a restart.
4. Stop Redis and verify `/health` remains `200`, `/health/ready` becomes `503`, and SSE receives `sync.required`; restore Redis and verify readiness and subscriptions recover automatically.
5. Stop the active scheduler owner and verify the standby takes over; run a background task and confirm there is one final result.
6. With reverse tunnels enabled, send a request to a non-owner Primary, verify the file streams through the owner, and verify a stale fencing token is rejected.
7. Validate every upload strategy used in production. For filesystem/SFTP staging, send init, chunk, progress, complete, and cancel to different Primaries and exercise duplicate chunks and concurrent completion.
8. Test uploaded-avatar reads across instances and verify that global Ingress/LB rate limits behave as expected.

See the [Production Launch Checklist](/en/ops/launch-checklist/) for the complete production review. For the repository's two-Primary StatefulSet, Service, PDB, PVC, and Ingress examples, see [Kubernetes Deployment](/en/deploy/kubernetes/).
