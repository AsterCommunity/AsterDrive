# Upload Finalization Contracts

This document captures the provider-resumable upload finalization contract. The complete upload service still owns session-kind validation, quota accounting, verified blob finalization, retry behavior, and cleanup. This page records the storage-path rules consumed by both frontend-direct and server-relay provider sessions.

All non-empty uploads through the public file-upload API now start at `/files/upload/init`, which creates a session that freezes filename, MIME type, declared size, placement, policy, and transport before consuming content. The legacy ordinary HTTP multipart upload endpoint has been removed. The regular multipart/server, local-direct, and streaming-direct entries later in the matrix are internal upload/workspace-storage data planes without independent public HTTP entry points; a session body may delegate to the internal streaming-direct path after Init.

## Upload Capacity Admission

Capacity observation and operation-specific assessment are separate contracts:

- `StorageCapacityStatus` is the observation returned by driver, admin, and Remote wire contracts: `supported` means reliable data is present, `unsupported` means the connector has no portable capacity API, and `unavailable` means capacity should be observable but this attempt produced no usable data.
- `StorageCapacityAssessment` compares an observation with the declared size and yields `sufficient`, `insufficient`, `unsupported`, or `unavailable`. Migration and upload paths reuse this assessment instead of interpreting `available_bytes` independently.
- For a conclusively insufficient or currently unavailable candidate, the upload planner adds a request-local target exclusion and reruns the existing placement rules. Only the final policy, transport, and session kind are frozen.
- `unsupported` is a valid capability result, so upload continues and relies on the data-plane result. `unavailable` has no capacity conclusion, so the planner prefers another target and returns a retryable error when none remains.
- Capacity is a fast-fail snapshot, not a cross-request reservation. The final workspace quota is protected by the transactional SQL CAS; target capacity still relies on driver write outcomes and existing cleanup/finalization contracts.

Capacity probes use a demand-driven coordinator owned by `DriverRegistry`, with no periodic scan. It caches raw observations rather than size-specific assessments, coalesces probes per policy, and bounds concurrency across policies. Each driver owns a `StorageCapacityProbePolicy`: Local uses a two-second fresh window, 30-second sufficient stale window, 250-millisecond negative window, and fixed two-second timeout; OneDrive and Remote use a 30-second fresh window, five-minute sufficient stale window, one-second negative window, and a connector-configurable timeout from two to 30 seconds (default ten). Stale insufficient or unavailable decisions require refresh confirmation. A failed refresh preserves the last usable observation for requests it still classifies as sufficient, while larger requests receive the latest probe failure instead of a stale rejection. Policy, credential, and driver invalidation clears the observation, and probe tasks survive HTTP request cancellation while recording failure and latency.

Before session Init returns, `OffsetStaging` / `StreamStaging` serialize capacity admission against the filesystem containing `upload_temp_dir` and use the revision-pinned `aster-fs` allocation contract to reserve physical blocks for the complete `total_size`; a sparse length created by `set_len` is no longer treated as a reservation. On Apple filesystems, new extents use all-or-nothing allocation and validate the kernel-reported byte count before extending logical EOF, while recovery materializes actual sparse holes without changing existing data or falsely reporting success. Admission keeps the configured `server.upload_temp_min_free_bytes` safety floor (256 MiB by default), accepts an exact `required + floor` fit, and returns stable `upload.staging_capacity_insufficient` (HTTP 507) while cleaning the session, temporary directory, and Init-created relative-path directories when space is insufficient. Setting the floor to `0` disables the margin, not physical preallocation.

The cluster profile permits the same staging protocol when operators make every Primary's `upload_temp_dir` resolve to one shared filesystem. Durable receipts, `received_count`, completion state, and the assembly lease remain authoritative in the shared writer database; exclusion for writes to the same chunk depends on cross-instance advisory locks supplied by the deployed filesystem. AsterDrive does not infer mount identity from path strings or create a second shared-filesystem session kind.

The reservation coordinator serializes capacity checks and allocation through the shared staging filesystem without introducing a duplicate database ledger. `upload_sessions.session_kind + total_size + status` remains the durable recovery source. The first staged Init, Chunk PUT, or Complete after process start reloads active staged sessions and allocates only the physical bytes missing according to each file's `allocated_size`. Successful completion, cancellation, expiry cleanup, and forced policy cleanup release the reservation by deleting the session temporary directory; in cluster deployments, any Primary may perform these operations against the same shared session directory.

## Provider Resumable Upload

OneDrive and similar providers expose a stateful upload session whose progress can be queried. The connector selects one of two data paths:

- `FrontendDirect`: the authenticated browser receives the temporary provider upload URL and uploads ranges directly.
- `ServerRelay`: the browser receives only the AsterDrive upload ID and sequential scheduling metadata; a Primary streams each authenticated chunk request into the provider session.

### Initialization

- The connector must select `ProviderResumable(FrontendDirect | ServerRelay)` and the driver must expose `provider_resumable`. Only the direct path additionally requires `frontend_direct_upload = true`.
- Fragment size must satisfy the provider minimum, maximum, and alignment constraints.
- All ranges are sequential: the direct frontend follows `next_expected_ranges`, while the relay backend enforces ordering through scheduling metadata and its shared-database state machine.
- `create_upload_session(object_temp_key)` receives an object path generated by `nondedup_storage_path_for_policy()`.
- The object path comes from the policy connector descriptor:
  - `opaque_uuid`: `files/{upload_id}`
  - `original_filename`: `files/{upload_id}/{normalized_filename}`
- OneDrive declares `original_filename`, so the Graph item keeps the original filename and can return it from a direct download URL.
- Upload naming must not be inferred from `ProviderResumable` or `DriverType` inside upload services.
- The upload URL is a write credential and is encrypted in `upload_sessions.provider_session_ciphertext`.
- Direct sessions persist as `provider_direct_resumable` and return the temporary provider upload URL. Relay sessions persist as `provider_relay_resumable`, return the ordinary `chunked` mode without the provider URL, and declare `sequential` scheduling with `max_chunk_concurrency = 1`.
- Database persistence failure, upload-ID collision, or session encryption failure must abort the provider session and delete `object_temp_key`.

### Frontend-Direct Data Path

The browser sends `PUT` requests with `Content-Range` to the provider upload URL without AsterDrive credentials. The frontend follows `next_expected_ranges`; non-final fragments obey the provider alignment requirement. The provider completes the item after the final range.

### Server-Relay Data Path

- The browser calls only AsterDrive's authenticated chunk endpoint. The Graph upload URL remains encrypted on the server.
- A Primary uses a fixed 64 KiB duplex pipe to connect the Actix request payload to `upload_session_fragment_reader`; a complete fragment is neither staged on disk nor buffered as one allocation.
- Graph range PUTs carry exact `Content-Length` and `Content-Range` headers. The preauthenticated upload URL is not sent an OAuth authorization header. OneDrive non-final fragments stay aligned to 320 KiB and each request is capped at 50 MiB.
- The unique `(upload_id, part_number)` row in `upload_session_parts` is the shared-database claim. An empty ETag is an active claim; `provider-range-v1` is the durable receipt confirming provider acceptance.
- `received_count` is the only legal next chunk number. A claim heartbeat is refreshed every 30 seconds; a claim older than 120 seconds is reclaimed only after the provider still reports the same range start. This supports multiple Primaries without sticky sessions or a shared local staging directory.

### Progress and Completion

Direct progress decrypts the provider session metadata and calls `query_upload_session`. Relay progress additionally converts the provider offset into durable range receipts and advances `received_count` in order.

A failed relay PUT is ambiguous until provider progress is queried:

- An offset at or beyond the range end proves that the fragment committed, so AsterDrive finalizes the receipt without another PUT.
- An offset equal to the range start proves that it did not commit, so the claim is released for retry.
- An offset inside the range marks the session corrupted.
- If the status query also fails, the claim is retained for later reconciliation rather than risking a duplicate PUT.

When the provider session returns `NotFound`, existence of `object_temp_key` distinguishes an uncommitted session from a final range that completed and caused Graph to remove the upload session.

Relay completion first reconciles all provider ranges and requires `received_count == total_chunks`. Both paths then read metadata from `object_temp_key`, verify that the actual size equals `session.total_size`, and enter verified blob finalization and atomic database quota accounting.

### Cleanup Matrix

Initialization, cancellation, expiration, forced policy cleanup, and failed finalization must retain ownership of both provider resources and AsterDrive object paths:

```text
abort provider upload session
delete object_temp_key / named OneDrive UUID namespace
```

Provider `NotFound` during abort is treated as already complete. Retryable failures retain the upload session for another cleanup attempt; permission or configuration failures retain it for operator intervention. A temp-object delete error is considered complete only when an existence check proves that the object is already absent.

Initialization failures still report both abort and delete results:

| Abort | Delete | Result |
| --- | --- | --- |
| success | success | cleanup succeeds |
| failure | success | abort error is returned |
| success | failure | delete error is returned |
| failure | failure | both errors are retained |

Database persistence failure, upload-ID collision, session encryption failure, an empty provider URL, and database finalization failure require the same explicit cleanup ownership.

## Compatibility

Legacy OneDrive objects may use `files/{upload_id}`. They remain readable and
deletable. In `provider_native` mode they can continue to use Graph direct
download, which may expose the provider's UUID filename; in
`strict_current` mode the download service uses relay streaming so the current
AsterDrive filename is applied. No legacy-name check is performed unless the
administrator selects `strict_current`.

See [Object Naming and OneDrive Direct Downloads](./storage-object-naming-and-onedrive-direct-download.md) for the full connector capability, path, and download acceptance matrix.
