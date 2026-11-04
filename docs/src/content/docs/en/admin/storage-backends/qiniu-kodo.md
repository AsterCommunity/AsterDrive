---
title: "Qiniu Kodo"
description: "Native Kodo resumable uploads, private origin reads and Qiniu CDN delivery."
---

The Qiniu connector uses native Kodo APIs and replaces the old S3 wrapper. AsterDrive owns file records, permissions, quota, Blob/version publication and cleanup. The generic S3 connector remains available for S3-compatible storage.

## Configuration

Use the ordinary **native bucket name**, not the potentially different S3 space alias. Configure a bound HTTPS origin domain and least-privilege AK/SK credentials for upload, stat, list, copy, delete and private reads. Upload and management endpoints are discovered independently; a download CNAME is not an upload endpoint. Custom upload acceleration is not configured by this connector and has separate provider activation and ingress charges.

| Field | Meaning |
| --- | --- |
| `bucket` | Native Kodo bucket name confirmed in the console |
| `base_path` | Object-key prefix; retain it when taking over existing AD objects |
| `download_origin` | Bound HTTPS origin, without path, query or fragment |
| `download_auth_validity_seconds` | 1–3600 seconds, default 180; bounds internal origin reads and private delivery, with a 300-second cap on default Kodo source domains |
| `provider_resumable_upload_strategy` | `server_relay` streams through AD; `frontend_direct` uploads native parts from the browser |
| `object_storage_download_strategy` | `relay_stream` proxies through AD; `presigned` issues a native private delivery URL |
| `download_delivery` | `origin` or `qiniu_cdn` |
| `download_base_url` | HTTPS CDN origin, required only for CDN delivery |
| AK/SK | Encrypted static credentials; blank secret edits retain saved values |

Origin/CDN domains and the configured bucket/prefix must identify the same objects. Connection tests verify a unique temporary object's upload, stat, HEAD, GET, Range and deletion. They do not establish browser CORS or CDN acceptance.

## Upload and recovery

Kodo enters the provider resumable planner like OneDrive and Google Drive, but implements numbered concurrent parts and explicit completion rather than sequential ranges with implicit commit. It reuses AD's multipart scheduling, authoritative size checks and cleanup without pretending UpToken requests are S3 presigned PUT.

Server-side small uploads use native form POST. Larger streams use multipart v2. Both consume AsyncRead with fixed 64 KiB read buffers; logical part size does not allocate a part-sized buffer or require a temporary file. Failed/canceled multipart streams attempt abort. Provider expiry remains the fallback for process exit or lost initialization responses.

Browser uploads receive only an object-scoped short-lived UpToken. Parts can run concurrently and be retried/recovered; the backend lists actual sizes and ETags before merging. Non-final parts are at least 1 MiB, parts are at most 1 GiB, and an upload has at most 10,000 parts. Native sessions expire after seven days; AD may expire them earlier. File/Blob/version, quota and audit publication follows verified provider completion.

Allow the AD origin in upload CORS for PUT, Authorization and Content-Type. Configure GET/HEAD/Range and exposed response headers for download consumers. Server connectivity does not prove browser CORS.

## Private delivery

Native private URLs sign the final origin/CDN URL with `e` and a final `token` query parameter. Do not replace the signed host/path or append parameters. Unicode, spaces and literal percent signs are encoded; URL expiry and `expires_at` agree.

Signature validity is a policy field like COS/OBS CDN validity. The effective lifetime is the minimum of caller demand, policy setting and target limits. Default `*.kodo-*.qiniucs.com` source domains are capped at 300 seconds; the policy default is 180. Custom origin/CDN delivery uses the policy bound without extending the caller lifetime. Internal streamed GET, HEAD, Range and thumbnails share this rule; this signing TTL does not terminate established streams. Existing V2 settings without the field default to 180 seconds.

For Qiniu CDN, enable private-bucket origin authentication. This implementation uses native `e/token`, not the separate timestamp hotlink-protection scheme; the provider documents incompatible combinations. Verify unsigned, invalid, tampered-path/time and expired requests on both cache HIT and MISS. Authentication-result caches and content caches are different controls.

`relay_stream` overrides delivery while retaining dormant CDN settings. Exact filename/MIME/response-header requirements fall back to relay when native delivery cannot honor them; HTML/SVG isolation still applies. Issued URLs are bearer credentials and cannot be assumed immediately revoked when AD permissions change.

## Upgrade from V1

1. Stop/drain old-version writes and finish/cancel legacy uploads. S3 uploadIds cannot become native sessions. Upgrade Primary/Follower together.
2. Start the application with the existing instance credential-encryption key. Startup decrypts stored AK/SK, enumerates visible native buckets and obtains the default HTTPS source from region `io_src`. A unique random small object in the old S3 namespace verifies exactly one native mapping; it stays under the existing prefix and is removed afterwards. User objects are untouched.
3. After every identity and probe cleanup is verified, one transaction replaces V1 configurations, retaining policy IDs, keys, prefixes, behavior and encrypted credentials while filling the verified native bucket/source. Runtime then uses native protocols only.
4. No backup table is created. Old S3 fields are discarded only after successful verification. Network/permission failures, ambiguous mappings, cleanup failures or concurrent edits stop conversion with original configurations intact; resolve the condition and restart. There is no unconditional blanking or manual re-entry requirement.
5. Verify existing files, preview and Range before new writes. CDN domains remain an explicit delivery choice, not a guess from source discovery.

Upgrade credentials need native bucket enumeration, region/source discovery, old S3 probe write/read/delete and native private reads. Temporarily add missing permissions to restricted subaccounts. Each connection verification has a five-minute deadline. Subsequent restarts with V2 configs do not repeat the probe.

Object-only deferred cleanup snapshots are converted with their encrypted snapshot credentials, preserving temp keys. Executable cleanup with old S3 multipart IDs must finish/cancel first. Completed/canceled records stay historical. Schema-only migrations refuse unverified V1 configurations: first run application startup with the instance encryption key. Converted settings are irreversible; migration down cannot restore old S3 configuration.

Taking over existing AD objects preserves their database references. Objects uploaded independently through the Kodo console are not automatically imported into AD's file/permission/quota records.

## Acceptance

Native cloud tests use `ASTER_TEST_QINIU_KODO_BUCKET`, `ASTER_TEST_QINIU_KODO_ORIGIN`, `ASTER_TEST_QINIU_KODO_ACCESS_KEY`, and `ASTER_TEST_QINIU_KODO_SECRET_KEY`. Optional `ASTER_TEST_QINIU_KODO_BASE_PATH` retains the existing prefix; optional `ASTER_TEST_QINIU_KODO_EXISTING_KEY` probes an existing relative key using read-only stat/Range.

```bash
cargo nextest run --profile ci --test storage native_kodo_cloud_acceptance
```

Missing credentials explicitly skip cloud acceptance. Only randomly named test-owned objects are deleted. Real browser uploads/CORS, CDN HIT/MISS expiry/key rotation and cross-browser recovery require separate acceptance. Publish only anonymized request categories/status/results, never credentials or full signed URLs.

## References

- [Go SDK pinned revision](https://github.com/qiniu/go-sdk/tree/f278ca5c9cc5d4a85f304e4b73f2b0b36437c66e)
- [Multipart v2](https://developer.qiniu.com/kodo/7458/multipartupload)
- [Upload tokens](https://developer.qiniu.com/kodo/1208/upload-token)
- [Private downloads](https://developer.qiniu.com/kodo/1656/download-private)
- [CDN origin authentication](https://developer.qiniu.com/fusion/3930/back-to-the-source-authentication)
- [Authentication compatibility](https://developer.qiniu.com/fusion/13361/fusion-api-error-codes)
