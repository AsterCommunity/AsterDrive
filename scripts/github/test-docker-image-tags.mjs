#!/usr/bin/env node

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const workflow = await readFile(".github/workflows/docker-image.yml", "utf8");

const metadataSteps = [...workflow.matchAll(
  /- name: Extract (?:GHCR|Docker Hub) metadata[\s\S]*?(?=\n      - name:|\n  [a-z][\w-]+:|$)/g,
)].map((match) => match[0]);

assert.equal(
  metadataSteps.length,
  4,
  "full/slim GHCR and Docker Hub metadata must all define the release channels",
);

const releaseEnable = "enable=${{ !contains(github.ref_name, 'alpha') && !contains(github.ref_name, 'beta') && !contains(github.ref_name, 'rc') }}";
const escaped = (value) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
for (const [index, step] of metadataSteps.entries()) {
  assert.match(step, /type=ref,event=tag,suffix=\$\{\{ matrix\.suffix \}\}/, `metadata step ${index + 1} must retain immutable version tags`);
  assert.equal((step.match(/type=raw,value=latest/g) || []).length, 1, `metadata step ${index + 1} must define latest`);
  assert.equal((step.match(/type=raw,value=stable/g) || []).length, 1, `metadata step ${index + 1} must define stable`);
  assert.equal((step.match(/type=raw,value=edge/g) || []).length, 1, `metadata step ${index + 1} must define edge`);
  assert.equal((step.match(new RegExp(escaped(releaseEnable), "g")) || []).length, 2, `metadata step ${index + 1} must gate latest and stable on prerelease refs`);
  assert.match(step, /type=raw,value=edge\$\{\{ matrix\.suffix \}\},enable=true/, `metadata step ${index + 1} must move edge for every release tag`);
}

const releaseCases = [
  ["vX.Y.Z", ["latest", "stable", "edge"]],
  ["vX.Y.Z-alpha.1", ["edge"]],
  ["vX.Y.Z-beta.1", ["edge"]],
  ["vX.Y.Z-rc.1", ["edge"]],
];
for (const [refName, expectedChannels] of releaseCases) {
  const prerelease = /alpha|beta|rc/.test(refName);
  const channels = ["latest", "stable", "edge"].filter(
    (channel) => channel === "edge" || !prerelease,
  );
  assert.deepEqual(channels, expectedChannels, `${refName} channel matrix must match the documented lifecycle`);
}

const variants = [...workflow.matchAll(/\s+- variant: ([^\n]+)\n\s+suffix: "([^"]*)"/g)].map(([, variant, suffix]) => ({ variant, suffix }));
assert.deepEqual(variants, [
  { variant: "default-slim", suffix: "-slim" },
  { variant: "metrics-slim", suffix: "-metrics-slim" },
  { variant: "default", suffix: "" },
  { variant: "metrics", suffix: "-metrics" },
], "manifest jobs must cover default/metrics full and slim variants");
assert.equal((workflow.match(/create_manifest "\$GHCR_TAGS"/g) || []).length, 2, "each manifest job must publish GHCR tags");
assert.equal((workflow.match(/create_manifest "\$DOCKERHUB_TAGS"/g) || []).length, 2, "each manifest job must publish Docker Hub tags");

assert.match(workflow, /docker buildx imagetools create "\$\{args\[@\]\}" \\\n\s+--metadata-file/);
assert.match(workflow, /cosign sign --yes "\$\{REGISTRY_IMAGE_GHCR\}@\$\{GHCR_DIGEST\}"/);
assert.match(workflow, /cosign sign --yes "\$\{REGISTRY_IMAGE_DOCKERHUB\}@\$\{DOCKERHUB_DIGEST\}"/);

process.stdout.write("Docker release channel metadata contract passed for GHCR/Docker Hub full and slim manifests.\n");
