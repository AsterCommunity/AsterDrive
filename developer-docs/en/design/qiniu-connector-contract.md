# Native Kodo connector contract

Qiniu schema V2 replaces the S3 wrapper with native APIs. Wire authority is qiniu/go-sdk at f278ca5c9cc5d4a85f304e4b73f2b0b36437c66e (auth/credentials.go, storagev2/region/query.go, internal API specifications). There is no Rust SDK dependency, protocol auto-detection or S3 fallback. Generic S3 remains separate.

The connector owns configuration, encrypted static credentials, descriptors, conditional fields, probes and factories. The driver owns native signatures, service discovery, object requests, private delivery and structured errors. Services own authorization, upload sessions, integrity checks and file/Blob/version/quota/audit publication. Frontends execute finite backend requests without provider-ID/URL/JSON parsers.

## Resumable protocol

The connector selects ProviderResumable with server_relay or frontend_direct. Kodo v2 supports concurrent numbered parts and explicit merge, unlike OneDrive/Google Drive sequential ranges and implicit commit. ProviderResumableUploadDriver::numbered_parts negotiates that protocol and reuses existing multipart planning, parallel workers, verified completion and cleanup.

All numbered parts return the existing BrowserUploadRequest. Backend drivers translate signed PUT or native capabilities into the same request; completion declares a response header, fixed token, or provider reconciliation. The frontend scheduler has one execution path, with no presigned/native or provider branches. Native response bodies are not parsed by the frontend. Backend listParts supplies authoritative sizes and ETags before completion and recovery. External clients must adopt the new numbered-part schema; single-object presigned PUT stays unchanged. The historical wire mode presigned_multipart identifies the numbered scheduler, not SigV4.

Native sessions bind uploadId, bucket, complete key and provider deadline. Old S3 uploadIds cannot resume as native sessions. complete_upload_session supports explicit native finalization while implicit providers need no additional request; the numbered Kodo path uses the existing verified multipart completion transaction.

## Resource and delivery boundaries

Native form POST supports up to 1 GiB. Multipart v2 supports init, streamed numbered PUT, list, explicit complete and abort; nonfinal parts are at least 1 MiB, each part at most 1 GiB, with at most 10,000 parts and seven-day provider expiry.

Streams use fixed 64 KiB read buffers, not part-sized allocations or temporary-file assembly. ExactSizeReader probes extra bytes before exposing the final declared byte. Failure/cancellation attempts abort; a per-upload guard schedules canceled-stream cleanup and logs failures. Process exit or lost initialization responses retain provider expiry as the final safety net, not a promise of immediate cleanup.

Management uses native stat/list/copy/delete. Listing is paginated and supports a bounded visitor. Private origin reads use native e/token GET/HEAD/Range; ignored/inconsistent ranges fail. Download URLs sign the final host, encoded key and exact deadline, with token last and matching expires_at. Strict filename/response-header requirements fall back to relay. HTTPS origin/CDN configuration is distinct from upload/RS/RSF discovery, which is cached per driver by provider TTL. Redirects are disabled.

`download_auth_validity_seconds` is a descriptor-driven numeric policy field (1..3600, default 180); missing legacy V2 fields use serde defaults. Effective TTL is min(caller, configuration, provider). Default Kodo source domains have an additional 300-second cap. Streamed GET, HEAD, Range and thumbnails consume the same setting, and e/expires_at share one actual deadline. Signing lifetime does not cut off established streams. Only the exact provider expiry-window error is recognized as Misconfigured with a fixed sanitized reason; arbitrary provider bodies remain hidden.

## Upgrade

Static credential V1 encryption and blank-secret preservation remain unchanged. Configuration V2 contains native bucket, prefix, origin, delivery mode, optional CDN, resumable upload strategy and download strategy. Dormant CDN values may be saved with relay; native private CDN authentication uses e/token, not timestamp hotlink protection.

Startup performs the one-time online conversion before listeners and credential reload. The connector decrypts existing policy/target/cleanup credentials with the instance key. The migration-only driver resolver enumerates native buckets, filters by `/v4/query` S3 region alias and obtains default source `io_src.domains`. A unique 16-byte marker written under the legacy S3 prefix must be readable through exactly one native namespace; names or equal-size old objects do not prove identity. Only the random probe is written/deleted, never user content.

Network verification completes outside the transaction. All configurations commit together with original-config/credential compare-and-swap checks. Provider outage, missing permission, ambiguity, cleanup failure or concurrent edits stop startup with original data intact; verification has a five-minute per-connection deadline. Successful conversion discards old S3 fields without backup or runtime fallback. Schema-only migration refuses unverified V1 rows. Native bucket/default HTTPS source are filled automatically; CDN delivery remains explicit. V2 restarts do not repeat discovery.

IDs, keys, prefixes, behavior and credential V1 encryption remain unchanged. Object-only cleanup snapshots convert using the original policy ID/AAD; old S3 multipart cleanup must finish/cancel first. Historical completed/canceled snapshots stay unchanged. Conversion is irreversible; no legacy configuration backup or automatic restore is retained. Published migrations remain immutable.

## Verification and secrecy

AK/SK, capabilities, signed URLs and raw provider error bodies never enter logs, Debug or audit. Errors retain sanitized provider status/RequestId and StorageErrorKind, dropping reqwest URL-bearing errors. Tests cover independent signature vectors, native requests, streaming limits, recovery/completion/abort/expiry, malformed responses, configuration/credentials, SQLite failure atomicity and irreversible upgrade, online namespace confirmation and existing provider/frontend regressions. Generate OpenAPI, SDK and storage documentation through repository tooling. Mock tests do not establish real Kodo, browser CORS or CDN HIT/MISS/key-rotation acceptance.
