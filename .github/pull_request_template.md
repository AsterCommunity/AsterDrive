# Pull request

## Summary

-

## Test plan

- [ ] Not needed: documentation, metadata, or configuration-only change
- [ ] Backend: `cargo check`
- [ ] Backend: targeted `cargo nextest run --lib <filter>` or `cargo nextest run --test <name> <filter>`
- [ ] Frontend: `bun run check`
- [ ] Frontend: `bun run build`
- [ ] Manual UI check for user-facing changes

## Notes for reviewers

- Database migrations:
- Runtime configuration changes:
- Deployment or upgrade notes:

## External provider / connector / plugin contract (when applicable)

- Evidence IDs (`provider:<id>/<topic>`) and official URLs:
- API/signature version and exact SDK/transport pins; evidence reviewed/attempted on:
- Credential scopes and object/control-plane permission boundaries:
- Request method/endpoint/headers/signing and response/status/error/limit changes:
- Origin API, upload endpoint, bucket-bound CNAME and CDN/custom download domain boundaries:
- Independent signing vectors / local provider contract tests run:
- Real-provider/browser/CDN acceptance record, or explicitly **not run**:
- [ ] `make storage-docs-check`; `make storage-provider-links` (attach unverified failures)
- [ ] Bilingual tutorials and evidence updated before driver/connector/plugin contract changes
