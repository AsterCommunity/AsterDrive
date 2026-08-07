import { readFile } from "node:fs/promises";

import {
	validateArtifact,
	validateArtifactMetadata,
} from "./update-webdav-provider-range-baseline.mjs";

function requireRecord(value, name) {
	if (typeof value !== "object" || value === null || Array.isArray(value)) {
		throw new Error(`${name} must be an object`);
	}
	return value;
}

function requireNonEmptyString(value, name) {
	if (typeof value !== "string" || value.trim() === "") {
		throw new Error(`${name} must be a non-empty string`);
	}
}

function validateSkippedArtifact(artifact) {
	validateArtifactMetadata(artifact);
	if (artifact.status !== "skipped") {
		throw new Error("skipped input must be a schema-v1 artifact");
	}
	requireNonEmptyString(artifact.skip_reason, "artifact.skip_reason");
	const scenarios = requireRecord(artifact.scenarios, "artifact.scenarios");
	if (Object.keys(scenarios).length !== 0) {
		throw new Error("skipped artifact.scenarios must be empty");
	}
	return artifact;
}

export function classifyProviderArtifact(artifact) {
	requireRecord(artifact, "artifact");
	if (artifact.status === "completed") {
		validateArtifact(artifact);
		return "completed";
	}
	if (artifact.status === "skipped") {
		validateSkippedArtifact(artifact);
		return "skipped";
	}
	throw new Error(`unsupported provider artifact status: ${artifact.status}`);
}

if (import.meta.main) {
	const [, , artifactPath] = process.argv;
	if (!artifactPath) {
		throw new Error(
			"usage: bun tests/performance/validate-webdav-provider-range-artifact.mjs <artifact.json>",
		);
	}
	const artifact = JSON.parse(await readFile(artifactPath, "utf8"));
	console.log(classifyProviderArtifact(artifact));
}
