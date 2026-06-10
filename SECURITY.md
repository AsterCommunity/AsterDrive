# Security Policy

Thank you for helping keep AsterDrive and its users safe. AsterDrive is self-hosted software, so security reports are most useful when they clearly describe the affected version, deployment shape, and practical impact.

## Supported Versions

AsterDrive is still evolving quickly. Security fixes are handled for the active development line and the latest tagged release.

| Version or branch | Security support |
| --- | --- |
| `master` | Supported for current development |
| Latest tagged release | Supported |
| Older releases | Best effort only; upgrade is recommended |

If a vulnerability affects an older release and the fix is low risk, maintainers may provide backport guidance. Otherwise, users should upgrade to the latest release containing the fix.

## Reporting a Vulnerability

Please do not report security vulnerabilities through public GitHub issues, pull requests, discussions, or review comments.

Use one of these private channels instead:

- GitHub Private Vulnerability Reporting, if it is available for this repository
- Email: `report@esaps.net` with a subject starting with `[AsterDrive Security]`

Include as much of the following as you reasonably can:

- affected AsterDrive version, git commit, or container image tag
- deployment type, such as source build, Docker, reverse proxy, SQLite, PostgreSQL, MySQL, local storage, S3 storage, or remote follower storage
- vulnerable component or endpoint, if known
- clear reproduction steps or a minimal proof of concept
- expected impact, such as authentication bypass, privilege escalation, data exposure, arbitrary file access, stored XSS, SSRF, request forgery, unsafe preview behavior, or denial of service
- relevant logs, screenshots, HTTP requests, response snippets, or stack traces
- whether the issue is already public or has been reported elsewhere

Do not include real user data, production secrets, access tokens, private keys, database dumps, or files copied from systems you do not own. Redact sensitive values before sending logs or request examples.

## Response Expectations

Maintainers aim to:

- acknowledge a report within 5 business days
- provide an initial triage result within 14 business days when enough information is available
- keep the reporter updated when a fix is being prepared
- coordinate disclosure timing before publishing details

These are targets, not service-level guarantees. Small self-hosted projects can have uneven maintainer availability, but actionable private reports will be handled in good faith.

## Disclosure Process

The usual process is:

1. The reporter sends a private report.
2. Maintainers confirm whether the behavior is a security issue.
3. Maintainers prepare and test a fix.
4. A patched release, advisory, or mitigation note is published.
5. Public details can be discussed after users have had a reasonable chance to upgrade or apply mitigations.

Please do not publish exploit details, working proof-of-concept code, or broad scanning instructions before a fix or mitigation is available.

## Scope

Security reports are in scope when they affect AsterDrive itself, including:

- authentication, sessions, JWT handling, cookies, password reset, email-change confirmation, and optional Passkey / WebAuthn flows
- authorization boundaries for users, teams, workspaces, shares, direct links, admin APIs, WebDAV accounts, WOPI integration, and background tasks
- unintended file access, path traversal, object storage access, storage policy routing, blob reference handling, trash, version history, archive extraction, or thumbnail generation
- upload and download flows, including chunked uploads, presigned uploads, multipart uploads, remote follower storage, and resume behavior
- browser security issues in the web UI, public share pages, previews, sandboxed file responses, and admin console
- server-side request forgery, request smuggling, unsafe redirects, deserialization, command execution, SQL injection, migration issues, or unsafe dependency usage that is exploitable in AsterDrive
- sensitive information disclosure through logs, error responses, generated files, embedded frontend assets, backups, metrics, health endpoints, or API responses

The following are usually out of scope unless they demonstrate a concrete exploit against AsterDrive:

- vulnerabilities only present in unsupported old releases
- missing HTTPS, weak TLS settings, upload limits, or security headers that are controlled entirely by the operator's reverse proxy
- reports requiring administrator access where the administrator is only able to perform intended administrative actions
- generic dependency CVEs with no reachable or practical impact in AsterDrive
- denial-of-service reports based only on unrealistic traffic volume or resource exhaustion without a specific application flaw
- social engineering, phishing, spam, physical attacks, or attacks against third-party services not controlled by the project
- issues in a user's own deployment, custom reverse proxy, S3 provider, database server, mail server, or operating system configuration

## Research Rules

When testing AsterDrive:

- test only systems you own or where you have explicit permission
- prefer local or disposable test deployments
- keep proof-of-concept payloads minimal and non-destructive
- do not exfiltrate, modify, delete, encrypt, or publish other people's data
- do not attempt persistence, lateral movement, credential theft, cryptomining, spam, or broad Internet scanning
- stop testing and report privately if you discover access to data or capabilities outside your authorization

Good-faith research that follows this policy helps the project. Activity that harms users, damages systems, or exposes private data is not acceptable.

## Security Updates

Security fixes may be released as normal patch releases, GitHub security advisories, release notes, or mitigation guidance, depending on severity and available information.

Users running public or shared deployments should:

- keep AsterDrive updated
- run it behind a reverse proxy with HTTPS
- avoid exposing the backend directly to the public Internet without a proxy
- review storage policy credentials, mail credentials, database credentials, JWT secrets, and WebDAV accounts regularly
- restrict administrator accounts and rotate credentials after suspected compromise

