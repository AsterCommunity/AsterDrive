# Domain Design and Contracts

These documents explain domain boundaries that cross routes, services, repositories, storage connectors, and internal protocols. They answer why the implementation is split this way and which contracts multiple implementations must share; they are not deployment-facing configuration guides.

## Identity and authentication

- [External authentication module](./external-auth.md): provider descriptors, login flows, account resolution, email verification, and frontend/backend ownership.

## Storage and remote nodes

- [Remote storage target and policy ownership](../../zh-CN/design/remote-storage-target-policy-ownership.md): remote node, storage target, and remote policy ownership. **Chinese source; translation pending.**
- [Storage descriptor and field normalization](./storage-descriptor-normalization-contract.md): backend-authoritative fields, capabilities, and normalization rules.
- [Object naming and OneDrive direct downloads](./storage-object-naming-and-onedrive-direct-download.md): object keys, provider paths, and direct-download boundaries.

## Upload finalization

- [Upload finalization contracts](./upload-finalization-contracts.md): finalization rules shared by relay, multipart, presigned, and provider-resumable paths.

When changing these paths, review the corresponding API, OpenAPI schema, generated frontend types, and focused tests. Do not recreate connector capability matrices in the product layer.
