---
title: "Qiniu Kodo"
description: "Native Kodo resumable uploads, private origin reads and Qiniu CDN delivery."
---

See the [provider evidence index](https://drive.docs.astercosm.com/developer/en/reference/storage-provider-evidence/#qiniu) for protocol sources, versions, review status and cloud acceptance boundaries. Validate origin APIs, upload endpoints, CDN and custom domains separately.

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

- **Server relay** streams browser content through AD to Kodo. Policy chunk size and single-request limits select a single upload or recoverable numbered parts.
- **Browser direct** sends concurrent native parts directly to Kodo, with retry and recovery. AK/SK never enter the browser.

Single form uploads support up to 1 GiB. Numbered uploads have at most 10,000 parts, nonfinal parts of at least 1 MiB, and a 1 GiB per-part maximum. Kodo sessions expire after seven days; AD sessions may expire sooner.

Browser networking must reach Kodo upload domains. Allow the AD origin in upload CORS for PUT, Authorization and Content-Type. Download consumers may need GET/HEAD/Range and exposed response headers. Server connectivity does not prove browser CORS.

## Automatic bucket CORS

Choose **Auto configure Kodo CORS** in a storage policy's create or edit page and explicitly accept the same generic confirmation used by COS. The action uses the current draft or saved bucket and AK/SK. Blank credential edits retain saved values; changing the draft bucket does not write to the old bucket. Ordinary saving and connection tests never configure CORS.

Configure **Public site URL** in system settings first. Only those explicit HTTP(S) origins are used, with default ports removed and duplicates eliminated. Missing, wildcard or invalid origins stop before provider requests; the action never falls back to `*`.

**This affects the entire Kodo bucket, including other prefixes. Without custom rules Kodo allows cross-origin access by default; adding the first rule removes CORS response headers for unmatched origins.** If other sites share the bucket, configure their required rules in the Qiniu console first. CORS does not replace object authentication or change public access, hotlink protection or CDN settings.

The native management requests are `GET /corsRules/get/{bucket}` and `POST /corsRules/set/{bucket}` through the fixed UC endpoint `https://uc.qiniuapi.com`, using AK/SK-based `Qiniu` authentication. They never use download, CDN, upload or RS object-management hosts. Credentials require CORS read and set permission on the target bucket; scope subaccount permissions to that bucket. An upload token cannot configure CORS. Errors report HTTP status and filtered RequestId without echoing raw provider messages, AK/SK or management tokens.

Kodo rules have no stable IDs and use the first matching rule in order. AsterDrive preserves every existing rule and its order, and skips writes when the configuration already satisfies its requirements. Otherwise it appends a rule for POST/PUT/GET/HEAD, Authorization/Content-Type/Range and exposed ETag, Content-Length, Content-Range, Content-Disposition, Accept-Ranges, X-Reqid and X-Log. Repeated execution does not keep appending rules. An earlier matching read rule missing required exposed headers stops the action with instructions to adjust that rule in Qiniu; existing rules are never claimed, deleted or reordered.

Kodo permits at most 10 rules. AsterDrive also bounds management JSON reads and writes to 64 KiB as a defensive limit, not a documented provider size limit. Excess limits, unknown fields, malformed responses, permissions or other read failures never cause truncated or empty configuration writes. After writing, the complete ordered configuration is read back; verification failures or differences are reported as failures. Avoid concurrent CORS edits because the management API provides no conditional-write guarantee.

Results include the bucket, allowed origins, change status, preserved rule count and RequestId when available. Success proves configuration and readback, **not browser compatibility on every upload or CDN endpoint**. Qiniu's overview describes bucket rules without specifying every form-upload and multipart v2 ingress; upload endpoints may have fixed CORS behavior, while CDN caching and headers require separate acceptance. Verify browser POST where used, native part PUT preflights with Authorization/Content-Type, JSON acknowledgements, origin GET/HEAD/Range and response-header access. Separately verify CDN authentication, cache HIT/MISS, CORS and `Vary: Origin`; a bucket configuration write does not configure CDN CORS.

## Private delivery

| Download strategy and delivery | Behavior |
| --- | --- |
| Server relay | AD reads and streams from the configured origin; dormant CDN settings do not participate |
| Private URL + origin | AD authorizes, then redirects with 302 to a private origin URL; Range is supported |
| Private URL + CDN | AD authorizes, then redirects with 302 to a private CDN URL; Range is supported |

Both origin and CDN use native provider MIME/cache headers. Attachments retain the current AD filename without rewriting Kodo objects. MIME/cache overrides or Range do not trigger automatic relay. HTML/SVG inline isolation, conditional validation and AD-generated thumbnails retain their own same-origin handling.

Signature validity is 1–3600 seconds, default 180, bounded by the requested lifetime and target limits. Default Kodo source domains have a 300-second cap. Internal reads and thumbnails share these bounds. Expiry does not terminate established transfers or erase browser-cached content.

Enable private-bucket origin authentication in Qiniu CDN. AD uses native private URLs without a separate timestamp hotlink-protection scheme. Verify valid, unsigned, tampered and expired requests on cache HIT/MISS, plus key rotation and browser CORS as needed. Configure CDN MIME/cache policy in the Qiniu console. Signed URLs are short-lived bearer credentials: do not publish or modify them, and do not assume immediate revocation when AD permissions change.

## Upgrade from V1

1. Drain old-version writes and finish/cancel legacy Kodo uploads and multipart cleanup. Old S3 sessions cannot resume through native APIs. Upgrade Primary/Follower together.
2. Keep the existing instance credential-encryption key and start the new application. Startup verifies native bucket/default HTTPS origin online, then converts settings together while preserving IDs, object keys and prefixes.
3. Probe at most 16 candidate buckets concurrently. Exactly one match permits conversion despite unrelated candidates failing permission/network checks. Failure to establish a unique match, enumeration or probe write/cleanup failures, or concurrent edits stop startup with original settings retained. Resolve the condition and retry. Each connection has a five-minute verification limit.
4. Successful conversion is irreversible, without a legacy configuration backup or automatic rollback. Use application startup for online conversion; schema-only migration is insufficient.
5. Verify existing downloads, previews and Range before new writes. Configure CDN separately. Subsequent normal restarts do not repeat conversion.

Upgrade credentials need bucket enumeration, region discovery and temporary probe read/write/delete. Executable cleanup tasks missing encrypted credential snapshots must finish/cancel on the old version first.

Existing AD files retain their objects. Objects uploaded independently through the Kodo console are not automatically imported.

See the [native Kodo developer contract](https://github.com/AsterCommunity/AsterDrive/blob/master/developer-docs/en/design/qiniu-connector-contract.md) for protocol, streaming resource and migration validation details.

## Acceptance

Native cloud tests use `ASTER_TEST_QINIU_KODO_BUCKET`, `ASTER_TEST_QINIU_KODO_ORIGIN`, `ASTER_TEST_QINIU_KODO_ACCESS_KEY`, and `ASTER_TEST_QINIU_KODO_SECRET_KEY`. Optional `ASTER_TEST_QINIU_KODO_BASE_PATH` retains the existing prefix; optional `ASTER_TEST_QINIU_KODO_EXISTING_KEY` probes an existing relative key using read-only stat/Range.

```bash
cargo nextest run --profile ci --test storage native_kodo_cloud_acceptance
```

Missing credentials explicitly skip cloud acceptance. Only randomly named test-owned objects are deleted. Real browser uploads/CORS, CDN HIT/MISS expiry/key rotation and cross-browser recovery require separate acceptance. Publish only anonymized request categories/status/results, never credentials or full signed URLs.

## References

- [Pinned CORS read API](https://github.com/qiniu/go-sdk/blob/f278ca5c9cc5d4a85f304e4b73f2b0b36437c66e/storagev2/apis/api_get_bucket_cors_rules.go), [write request model](https://github.com/qiniu/go-sdk/blob/f278ca5c9cc5d4a85f304e4b73f2b0b36437c66e/storagev2/apis/set_bucket_cors_rules/api.go)
- [Bucket CORS behavior](https://developer.qiniu.com/kodo/8608/dev-cross-domain-resource-sharing), [rule limits and ordering](https://developer.qiniu.com/kodo/8539/set-the-cross-domain-resource-sharing)

- [Download settings and response overrides](https://developer.qiniu.com/kodo/1659/download-setting)
- [Go SDK pinned revision](https://github.com/qiniu/go-sdk/tree/f278ca5c9cc5d4a85f304e4b73f2b0b36437c66e)
- [Multipart v2](https://developer.qiniu.com/kodo/7458/multipartupload)
- [Upload tokens](https://developer.qiniu.com/kodo/1208/upload-token)
- [Private downloads](https://developer.qiniu.com/kodo/1656/download-private)
- [CDN origin authentication](https://developer.qiniu.com/fusion/3930/back-to-the-source-authentication)
- [Authentication compatibility](https://developer.qiniu.com/fusion/13361/fusion-api-error-codes)
