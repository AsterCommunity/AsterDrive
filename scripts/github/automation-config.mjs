export const AUTOMATION_LABELS = {
  "Risk: High": {
    color: "B60205",
    description: "Changes a high-risk data, security, protocol, or deployment boundary",
  },
  "CI: Failure": {
    color: "D73A4A",
    description: "Tracks a reproducible default-branch or scheduled CI failure",
  },
  "CI: Infrastructure": {
    color: "5319E7",
    description: "CI failure appears to originate from runner or external infrastructure",
  },
};

export const MANAGED_PR_LABELS = [
  "Rust",
  "TypeScript",
  "Documentation",
  "Dependencies",
  "Scope: Storage",
  "Scope: Admin UI",
  "Scope: Archive",
  "Scope: Files",
  "Scope: Plugins",
  "Scope: Remote Nodes",
  "Scope: Runtime",
  "Scope: Upload",
  "Scope: Versioning",
  "Scope: WebDAV",
  "Risk: High",
];

const RUST_PATHS = [
  "**/*.rs",
  "Cargo.toml",
  "Cargo.lock",
  "rust-toolchain.toml",
  "frontend-panel/package.json",
  "frontend-panel/generated/openapi.json",
  "frontend-panel/src/services/api.generated.ts",
  "scripts/coverage-summary.mjs",
  ".github/workflows/rust.yml",
];

const FRONTEND_PATHS = [
  "frontend-panel/**",
  "scripts/coverage-summary.mjs",
  ".github/workflows/frontend.yml",
];

const E2E_PATHS = [
  "frontend-panel/**",
  "src/**",
  "crates/**",
  "Cargo.toml",
  "Cargo.lock",
  ".github/workflows/frontend-e2e.yml",
];

const AUDIT_PATHS = [
  "**/Cargo.toml",
  "**/Cargo.lock",
  ".cargo/audit.toml",
  ".github/workflows/audit.yml",
];

const DOCS_PATHS = [
  "docs/**",
  "developer-docs/**",
  "crates/aster_drive_storage/src/connector_descriptor.rs",
  "src/storage/connectors/**",
  "tests/storage_connector_docs.rs",
  "Makefile",
  "src/api/api_error_code.rs",
  ".github/workflows/docs-check.yml",
];

const KUBERNETES_PATHS = [
  "deploy/kubernetes/**",
  "deploy/helm/**",
  ".github/workflows/kubernetes.yml",
];

const MULTI_PRIMARY_PATHS = [
  "src/**",
  "crates/**",
  "tests/multi_primary/**",
  "Cargo.toml",
  "Cargo.lock",
  "rust-toolchain.toml",
  ".github/workflows/multi-primary-e2e.yml",
];

const WEBDAV_PATHS = [
  "src/webdav/**",
  "src/services/files/**",
  "src/services/preview/wopi/locks.rs",
  "src/db/repository/lock*.rs",
  "crates/aster_drive_model/src/entities/resource_lock*.rs",
  "crates/aster_drive_model/src/types/resource_lock.rs",
  "crates/aster_drive_migration/src/*resource_locks*.rs",
  "tests/common/**",
  "tests/webdav/**",
  "scripts/ci/webdav-compat/**",
  ".github/actions/setup-litmus/**",
  ".github/workflows/webdav-compatibility.yml",
  "Cargo.toml",
  "Cargo.lock",
  "rust-toolchain.toml",
];

export const PR_WORKFLOWS = [
  {
    name: "Repository Automation",
    paths: [
      ".github/workflows/**",
      "scripts/github/**",
      "developer-docs/zh-CN/contributing/github-automation.md",
    ],
  },
  { name: "Rust CI", paths: RUST_PATHS },
  { name: "Frontend CI", paths: FRONTEND_PATHS },
  { name: "E2E", paths: E2E_PATHS },
  { name: "Security Audit", paths: AUDIT_PATHS },
  { name: "Docs Check", paths: DOCS_PATHS },
  { name: "Kubernetes Manifests", paths: KUBERNETES_PATHS },
  { name: "Multi-Primary E2E", paths: MULTI_PRIMARY_PATHS },
  { name: "WebDAV Compatibility", paths: WEBDAV_PATHS },
];

export const LABEL_RULES = [
  { label: "Rust", paths: ["**/*.rs", "**/Cargo.toml", "**/Cargo.lock", "rust-toolchain.toml"] },
  { label: "TypeScript", paths: ["frontend-panel/**", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.mjs"] },
  { label: "Documentation", paths: ["docs/**", "developer-docs/**", "README.md", "README.zh.md", "CONTRIBUTING.md", "CHANGELOG.md"] },
  { label: "Dependencies", paths: ["**/Cargo.toml", "**/Cargo.lock", "**/package.json", "**/bun.lock", ".github/dependabot.yml"] },
  { label: "Scope: Storage", paths: ["src/storage/**", "src/services/storage_policy/**", "crates/aster_drive_storage/**", "tests/storage_*.rs", "docs/**/storage*", "developer-docs/**/storage*"] },
  { label: "Scope: Admin UI", paths: ["frontend-panel/src/pages/admin/**", "frontend-panel/src/components/admin/**", "frontend-panel/src/features/admin/**"] },
  { label: "Scope: Archive", paths: ["src/services/archive/**", "tests/archive/**", "frontend-panel/**/archive*"] },
  { label: "Scope: Files", paths: ["src/api/routes/files/**", "src/api/routes/folders.rs", "src/services/files/**", "src/services/workspace/**", "frontend-panel/**/file*"] },
  { label: "Scope: Plugins", paths: ["src/plugins/**", "crates/**/plugin*", "developer-docs/**/plugin*"] },
  { label: "Scope: Remote Nodes", paths: ["src/services/remote/**", "src/storage/remote_protocol/**", "src/api/routes/internal_storage.rs", "src/storage/remote_tunnel.rs"] },
  { label: "Scope: Runtime", paths: ["src/runtime/**", "src/services/background_tasks/**", "src/api/routes/background_tasks.rs", "deploy/**", "Dockerfile", "docker-compose.yml"] },
  { label: "Scope: Upload", paths: ["src/services/files/upload/**", "src/api/routes/files/upload*", "frontend-panel/**/upload*"] },
  { label: "Scope: Versioning", paths: ["src/services/files/version*", "src/db/repository/file_version*", "crates/aster_drive_model/**/file_version*"] },
  { label: "Scope: WebDAV", paths: ["src/webdav/**", "tests/webdav/**", "scripts/ci/webdav-compat/**", ".github/actions/setup-litmus/**"] },
  {
    label: "Risk: High",
    paths: [
      "crates/aster_drive_migration/**",
      "src/api/routes/auth/**",
      "src/services/auth/**",
      "src/services/files/upload/**",
      "src/services/files/lock/**",
      "src/services/quota/**",
      "src/webdav/**",
      "src/services/preview/wopi/**",
      "src/api/routes/internal_storage.rs",
      "src/storage/remote_protocol/**",
      "deploy/kubernetes/**",
      "deploy/helm/**",
      ".github/workflows/**",
    ],
  },
];

export const CI_COMMENT_MARKER = "<!-- asterdrive-ci-diagnostics -->";
export const CI_INCIDENT_MARKER_PREFIX = "asterdrive-ci-incident";
export const PR_GATE_NAME = "PR Gate";
