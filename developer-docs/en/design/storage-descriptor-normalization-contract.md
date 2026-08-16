# Storage Descriptor and Field Normalization Contract

This document records the current development contract for AsterDrive storage policy connector descriptors and remote storage target driver descriptors. It is for backend and frontend contributors, not an end-user guide.

## Scope

AsterDrive currently has two related but distinct descriptor surfaces:

- `src/storage/connector_descriptor.rs` and `src/storage/connectors/`: storage policy management forms, connection tests, authorization, policy actions, upload workflows, and connector capabilities.
- `src/services/remote/storage_target/driver.rs`: follower-side remote storage target drivers, fields, and local normalization rules.

Do not collapse these into one universal descriptor. Storage policy descriptors describe primary-side policy behavior and upload/download workflows. Remote storage target descriptors describe follower-side ingress target configuration.

Shared field meanings and pure normalization helpers live in `src/storage/field_contract.rs`. Product-specific descriptor DTOs remain separate.

## Descriptor Rules

- Admin fields, actions, capabilities, and UI helper metadata must come from backend descriptors first. The frontend must not infer connection tests, authorization, upload strategy, native processing, remote binding, or field visibility from a local `driver_type` capability matrix.
- `label_key`, `help_key`, `placeholder`, `required_message_key`, and similar fields are stable localization keys or hint parameters. The frontend owns final localized text, but field presence, required state, sensitivity, and supported actions are backend descriptor concerns.
- `secret: true` or secret field kind means the frontend must render a sensitive input and backend logs / `Debug` output must not expose plaintext. Create flows follow descriptor `required` plus backend validation. Edit flows treat omitted secret fields as "preserve the stored value"; explicit values replace the stored secret after normalization.
- `StorageConnectorFieldScope::PolicyOptions` fields belong to `storage_policy.options`. The frontend must render and normalize them from descriptors, not from a local per-driver field matrix. SFTP `sftp_host_key_fingerprint` is such a field: the backend declares the field, label key, trim behavior, and validation rule; the frontend only displays and submits it.
- Unsupported drivers must produce stable backend errors. Remote storage targets may expose only known registered drivers that the remote capability payload declares. Unknown wire-level driver ids may be preserved, but they are not locally configurable drivers.
- When descriptors, remote capabilities, or capability parsing are missing, the frontend may only fall back conservatively by hiding risky actions or showing an unavailable state. It must not recreate a local capability matrix in that fallback path.
- Action descriptors declare whether an entry point requires a saved policy, authorization credential, or remote-state mutation. Routes and services still perform final validation; hidden buttons are not authorization.
- Structured action `output` is presented transiently only through descriptor-declared `output_fields`. Undeclared fields, type mismatches, and results belonging to another action are ignored. Output is never copied into policy config or retained across actions or dialog sessions.
- Delegated-credential status copy, semantic tone, description, attention guidance, sanitized-reason mapping, lifecycle labels, reauthorization wording, and redirect-URI help belong to `credential_management`. The shared panel must not branch on OneDrive, Microsoft Graph, or another provider ID, and must not render the wire `status_reason` directly.

## Documentation Fact Projection

- Built-in connector identity, display names, `deployment_scope`, `credential_mode`, `capabilities`, and `upload_workflows` remain owned by runtime descriptors and localization. The documentation layer does not define a parallel capability type.
- `tests/storage_connector_docs.rs` reads the same catalog APIs used by administrators, generates `docs/generated/storage-connectors.json`, and updates marked blocks in the backend index, policy catalog, and capability matrix. Generated outputs are committed and reviewed with code.
- Tutorial slugs and “best for” summaries are provider-owned documentation metadata, not runtime capabilities. Every new built-in connector must add both fields and Chinese/English tutorials; the test rejects missing metadata and tutorial paths.
- The capability matrix shows each connector's static ceiling. Policy settings, remote-node transport, and deployment networking may narrow actual availability; provider tutorials explain those conditions.
- Provider names in READMEs, deployment pages, and tutorials are explicitly non-exhaustive examples rather than backend catalogs. Adding a connector does not require global replacement of every contextual example.

## Normalization Rules

Field normalization belongs in backend use cases or connector/driver-specific pure helpers. It must not be scattered in handlers or frontend components.

- Shared field semantics and normalization helpers live in `src/storage/field_contract.rs`.
- Local remote storage target `base_path` uses `normalize_relative_local_target_path` through the remote target service wrapper: trim whitespace, collapse `.` segments, reject blanks, absolute paths, `..`, Windows prefixes, and backslash escapes, then resolve within `server.follower.remote_storage_target_local_root`.
- Object-storage remote storage target `base_path` is a prefix: trim whitespace and outer `/`; an empty prefix means bucket/container root.
- Storage policy object-storage endpoint/bucket normalization uses `normalize_s3_endpoint_and_bucket` plus connector-specific API error mapping. Non-empty endpoints must be `http://` or `https://` and include a hostname; bucket/container is required.
- Storage policy SFTP endpoint normalization is handled by `parse_sftp_endpoint`: it allows `sftp://host:port`, bare `host`, and `host:port`, with default port `22`; only a real `://` scheme separator triggers URL-scheme validation. Paths, query strings, fragments, and URL credentials are invalid; the remote root must use `base_path`.
- SFTP host key fingerprints live in `storage_policy.options.sftp_host_key_fingerprint`. Unknown or mismatched host keys must fail closed and expose actual / expected fingerprints through structured `SftpHostKeyRejected` context; tests must not parse error text.
- For storage policies, `max_file_size = 0` means no extra policy limit and negative values are invalid at the service boundary. Upload paths still perform final size checks when applying a policy. Remote storage target create DTOs do not accept this field.
- Same-driver edits preserve omitted `access_key` / `secret_key` values. Explicit replacements are trimmed and revalidated.
- When changing a remote storage target driver, old driver-specific fields must not leak into the new driver. Endpoint, bucket, access key, and secret key reset; base path follows the new driver input/default semantics and is normalized again.

## Implementation Boundaries

- Route layers only adapt protocol shape, authenticate/authorize, extract parameters, call services, and map responses. They do not build descriptors or run driver-specific normalization.
- Service layers orchestrate use cases: load context, call normalization helpers, check capabilities, call repositories, and run required side effects.
- `src/storage/connectors/` owns storage policy connector descriptors, connection field normalization, connection tests, authorization, and connector actions.
- `src/services/remote/storage_target/driver.rs` owns remote storage target driver descriptors, driver-field normalization, target-to-policy materialization, and driver build/validate.
- `src/storage/remote_protocol/` handles wire models, signing, path encoding, transport, and response parsing. It does not decide UI fields or policy target selection.

## Tests

When descriptor or normalization behavior changes, add focused unit tests:

- Descriptor tests must cover every built-in driver field, secret marker, action, and key capability.
- Credential-management coverage must include missing, authorized, and reauthorization-required states. Action-output coverage must include draft and saved execution, absent output, undeclared fields, type-mismatched fields, and the generic-success fallback. It must also prove that failed-action output is neither displayed nor persisted, results are not written back to policy config, and no result survives into another action or dialog session.
- Normalization tests must cover trimming, blank values, path escapes, prefix slash trimming, negative storage-policy `max_file_size`, same-driver secret preservation, explicit secret replacement, and driver-change field reset.
- SFTP coverage must include bare host, `host:port`, `sftp://host:port`, wrong schemes, host key fingerprint format, unknown-host-key rejection, and accepted pinned fingerprints.
- For storage policy descriptor behavior, run `cargo nextest run --lib storage::connectors` or a narrower filter.
- When built-in connector identity, localization, or projected documentation facts change, run `make storage-docs`, review the generated diff, then run `make storage-docs-check`.
- For remote storage target normalization, run `cargo nextest run --lib remote::storage_target::tests::<filter>`.
- OpenAPI schema changes require OpenAPI export, frontend SDK regeneration, and review of the generated diff.
