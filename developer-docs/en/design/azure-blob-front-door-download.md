# Azure Blob Front Door download contract

Issue #689 supports Azure Front Door Standard/Premium with client service SAS and caching disabled. Legacy CDN products, origin SAS injection and anonymous delivery are outside this contract.

## Ownership

- Connector: schema v1 adds `download_delivery`, defaulting to `blob_origin`, and optional `download_base_url`, required for `front_door_client_sas`. Descriptor metadata drives the generic form and payload builders. Serde defaults preserve existing persisted configurations. No database migration or generic OpenAPI type changes are needed.
- Driver: only `DirectDownloadStorageDriver` uses the delivery host. The canonical resource remains `/blob/account/container/full_key`. Blob API operations, uploads, multipart, copies, cleanup and connection tests retain their independent origin endpoint.
- Service and resource handle: retain permission checks, transfer-policy selection, filename/content-type overrides, safe inline fallback and the five-minute download lifetime. Browsers omit credentials and conditional headers. Handle expiry equals the SAS `se` value.
- Frontend: use the existing descriptor form without a provider/domain routing matrix.

## Provider evidence

| Behavior | Microsoft reference |
| --- | --- |
| Standard/Premium forward client SAS; Premium supports private Blob origins | [Storage scenario](https://learn.microsoft.com/en-us/azure/frontdoor/scenario-storage-blobs) |
| Separate Front Door endpoint, Blob origin host and route settings | [Storage integration](https://learn.microsoft.com/en-us/azure/frontdoor/integrate-storage-account) |
| private/no-store prevents caching; query cache keys do not validate SAS on every request | [Front Door caching](https://learn.microsoft.com/en-us/azure/frontdoor/front-door-caching) |
| Service SAS field order, decoded canonical resource and signed rscc/rscd/rsct overrides | [Create service SAS](https://learn.microsoft.com/en-us/rest/api/storageservices/create-service-sas) |

## Routing and security

The delivery URL must be an HTTPS origin without userinfo, a non-default port, a path, query, fragment, backslashes or control characters. Front Door forwards the original `/container/base_path/blob` path to the Blob hostname with an empty origin path and no rewrite or SAS injection. Static website endpoints are rejected. Encode object names per segment: a literal `%2F` becomes `%252F`. Reject dot segments, backslashes and control characters before URL normalization can change the signed object.

Read permission authorizes GET, HEAD and Range. Response overrides are part of the HMAC. Front Door delivery forces `private, no-store`, including when a caller requests public caching. Operators must disable route caching and purge older entries. Cache-hit delivery is unsupported: placing SAS in a cache key does not itself enforce expiration. Private Link never replaces end-user SAS.

The server creates a delivery URL without probing or following it. Browser TLS, redirects and CORS depend on the configured Front Door route. The Azure SDK's backend redirect policy is verified by a local request test. Issued URLs remain bearer credentials until expiry; share revocation cannot immediately revoke them. Account keys and complete SAS URLs must not enter logs, errors or audit fields. Connection tests validate Blob API access, not edge DNS, TLS, origin networking, CORS or cache rules.

## Validation

No Azure account was available for this implementation, so real Azure / Front Door acceptance has not been performed. Record local checks separately from cloud acceptance; local test success does not establish correct cloud configuration.

Local coverage includes independent HMAC verification, Unicode/space/literal-percent paths, response overrides, expiry bounds and malformed URLs; real SDK request capture for Blob routing and redirect refusal; resource-handle authorization and lifecycle; policy create/read/edit and legacy defaults; descriptor-driven form/payload tests and browser create/edit coverage.

Azurite integration tests additionally exercise Blob requests, signed response headers, GET/HEAD/Range, missing or tampered SAS, read-only permissions and rejection of a previously successful URL after expiry. Without Docker, use a local emulator as described in the [testing guide](../testing/index.md#azure-blob-integration-tests). Azurite does not emulate Front Door.

Run `make storage-docs` and review the generated projection. OpenAPI/SDK generation follows the repository workflow; only generic DTO changes should produce schema differences.

Real-provider acceptance remains separate: test a private container and Front Door domain with GET/HEAD/Range and CORS preflight; reject missing, tampered and expired SAS; repeat a previously successful URL after expiry; confirm caching is disabled. Validate DNS, certificates, origin host/path, optional Private Link and independent backend API access. Local mocks cannot establish these Azure infrastructure properties. The English and Chinese Azure Blob user guides contain the operator checklist.
