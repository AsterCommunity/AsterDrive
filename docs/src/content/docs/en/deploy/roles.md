---
title: "Separate API and worker roles"
description: "Role ownership, shared state, cancellation, concurrency and migration for the same binary."
---

`./aster_drive`, `./aster_drive serve` and `./aster_drive serve all` compose the full service.
Single-process SQLite + memory cache + local storage remains supported without another worker.

| Command | Ownership |
| --- | --- |
| `serve all` | Business HTTP, producers, local maintenance, consumers, singleton scheduling and mail |
| `serve api` | REST/frontend/WebDAV/WOPI, uploads/downloads, task queries/cancellation, SSE and process-owned maintenance |
| `serve worker` | Durable consumption, singleton scheduling/cleanup and mail; only `/health*` HTTP routes |

Follower remains selected by `server.start_mode = "follower"`. Default startup and `serve all` respect it;
explicit `serve api` or `serve worker` rejects a Follower configuration. Deployment profile, node mode and process role are independent dimensions.

## State and recovery

Split deployments use the cluster profile with shared PostgreSQL/MySQL, Redis cache/config sync,
application/credential keys and reachable storage. Both roles use the same version, handlers, repositories and product transactions.

| Data | Durability and reachability |
| --- | --- |
| Tasks, progress, leases, metadata, quota, versions and publication receipts | Authoritative database; no file bodies |
| Blobs and published archives/thumbnails | Connector reachable by every role; local policies require an operator-managed shared filesystem |
| Upload staging/chunks and avatars | Existing cluster shared-path requirements; isolated Pod volumes cannot substitute |
| Task scratch | Private, token-scoped attempt directory; takeover rebuilds from durable inputs |
| Configuration and storage SSE | Redis notifications plus authoritative rereads/reconnect; Notify only optimizes latency |
| Reverse tunnels | API/all owns incoming follower connections; workers use owner discovery and authenticated streaming proxy |

API acceptance persists the task and required inputs. Consumers run outside the scheduler leader lease and
claim only local free slots. One transaction locks the authoritative quota, selects and fences rows.
Polling discovers remote work without Notify, bounded by configured idle max (currently at most 164 seconds).
With zero workers, tasks remain queued and synchronous API operations remain available; existing admission policies still apply.

Execution is at least once. Old tokens cannot overwrite progress, checkpoints or terminal states.
Archive compression/extraction and offline downloads commit publication receipts with files/folders,
quota, versions and activity. Retries reuse published results. Failures and cancellation retain committed outputs.
This does not guarantee exactly-once external side effects or recovery of deleted, unprocessed inputs.

## Cooperative cancellation

The task center confirms before calling personal `POST /api/v1/tasks/{id}/cancel` or team
`POST /api/v1/teams/{team_id}/tasks/{id}/cancel`, preserving workspace permissions.
The server supplies `can_cancel`; system maintenance is excluded.

- Pending/Retry becomes Canceled without executing the handler.
- Processing becomes Canceling; the UI continues polling until a worker acknowledges Canceled.
- Canceling/Canceled requests are idempotent; Succeeded/Failed returns 409.
- After a crash, an expired Canceling lease is reclaimed only to acknowledge cancellation, without rerunning business work.
- Cancellation stops subsequent work and retains committed changes. It does not roll back. Canceling cannot retry;
  acknowledged Canceled tasks may be retried manually with the same ID and receipts.

The normal task lease is 60 seconds with a 10-second heartbeat. Crash takeover waits for the remaining lease plus polling/database recovery.
Blocking external calls can delay cooperative stopping; requesting cancellation does not mean execution has already stopped.

## Limits, health and shutdown

Existing `background_task_max_concurrency`, `background_task_archive_max_concurrency`,
`background_task_thumbnail_max_concurrency`, `offline_download_max_concurrency` and
`background_task_storage_migration_max_concurrency` now bound each worker's corresponding lane.
Shared settings are `task_fallback_cluster_concurrency`, `task_archive_cluster_concurrency`,
`task_thumbnail_cluster_concurrency`, `task_offline_download_cluster_concurrency` and
`task_storage_migration_cluster_concurrency`: 0 means unlimited cluster-wide, maximum 65536.
Claims reread the database quota even with stale in-memory configuration. Lowering a limit does not kill running work.

`/health` describes local liveness, independent of transient database/Redis availability.
`/health/ready` requires initialization and critical dependencies; workers additionally require live consumers/scheduler.
Empty queues and healthy standby schedulers are ready. Shutdown stops claims and readiness, drains for 20 seconds,
then requests cooperative cancellation for 5 seconds before closing audit/database. The outer task group adds 5 seconds.
Static `[worker]` settings `drain_timeout_secs` and `cancellation_timeout_secs` have a combined 24-hour maximum.
Pod grace must also cover preStop, HTTP drain and audit flushing.

With the metrics feature and Prometheus enabled, `/health/metrics` exports queue depth,
`aster_task_pending_oldest_age_seconds`, active executions, retry/failure and `aster_task_lease_takeovers_total`.
Resources distinguish `aster_node_mode=primary/follower` from `aster_role=all/api/worker`; Follower currently uses follower + all. Node mode and process responsibility can evolve independently, without unbounded runtime-ID metric labels.

## Deployment and mandatory migration

Helm defaults to `split.enabled=false`. Enable it for independent `split.api` and `split.worker`
replicas/resources/scheduling/PDB/grace/autoscaling. Business Service selects API only.
Workers may scale to 0 with HPA disabled; HPA owns replicas when enabled.
Kustomize provides `deploy/kubernetes/overlays/split`. StatefulSets retain stable internal endpoints.

This is a uniform-version migration with no legacy Forge API or mixed task schema/payload compatibility layer:

1. Back up database/storage. Stop old worker/all acquisition, drain running work and stop old business writers.
2. Upgrade every role to the same release, keys, cluster configuration and shared paths; retain tasks and inputs.
3. The first new process performs coordinated migrations (PostgreSQL advisory lock, MySQL named lock, SQLite single writer).
   `task_resource_publications` is added and follows task-history retention. Other processes wait on the migration lock and become ready only after successful initialization.
4. Verify synchronous API behavior and visible queued work, then start workers and check outputs, quota, versions, audit and SSE.
5. To return to a combined process, drain workers and start **the same new release** with `serve all`, updating Service routing.
   Downgrading to an older release requires stopping writers and restoring matching database/storage backups.

Scaling either role does not require restarting the other. Existing cluster storage capability checks remain enforced.
Identical paths backed by unrelated private volumes are not shared storage. SQLite/memory cache is unsupported for cross-Pod split deployments.
