import { describe, expect, test } from "bun:test";

import { redactProviderLog } from "./redact-webdav-provider-range-log.mjs";
import { classifyProviderArtifact } from "./validate-webdav-provider-range-artifact.mjs";

function completedArtifact() {
	return {
		schema_version: 1,
		status: "completed",
		provider: "local",
		generated_at: "2026-08-07T00:00:00Z",
		git_revision: "0123456789abcdef",
		git_dirty: false,
		fixture: { payload_bytes: 1024, range_bytes: 16 },
		sampling: { warmups: 1, samples: 2, read_buffer_bytes: 64 },
		machine: {
			build_profile: "optimized",
			os: "linux",
			architecture: "x86_64",
		},
		scenarios: {
			full_get: {
				summary: {
					ttfb_ms: { p95: 0.5 },
					throughput_bytes_per_second: { p50: 1024 },
				},
			},
		},
	};
}

function skippedArtifact() {
	return {
		...completedArtifact(),
		status: "skipped",
		skip_reason: "provider fixture is not configured",
		scenarios: {},
	};
}

describe("classifyProviderArtifact", () => {
	test("accepts completed artifacts through the baseline validator", () => {
		expect(classifyProviderArtifact(completedArtifact())).toBe("completed");
	});

	test("keeps valid skipped artifacts out of the completed path", () => {
		expect(classifyProviderArtifact(skippedArtifact())).toBe("skipped");
	});

	test("rejects malformed and unsupported artifacts", () => {
		const malformedSkip = skippedArtifact();
		malformedSkip.scenarios.full_get = completedArtifact().scenarios.full_get;
		expect(() => classifyProviderArtifact(malformedSkip)).toThrow(
			"skipped artifact.scenarios must be empty",
		);
		expect(() =>
			classifyProviderArtifact({ schema_version: 1, status: "partial" }),
		).toThrow("unsupported provider artifact status");
	});
});

describe("redactProviderLog", () => {
	test("redacts each configured value without treating it as a pattern", () => {
		const log = [
			"authorization: Bearer token.with[regex]",
			"password=s3cr3t",
			"token.with[regex] appears twice: token.with[regex]",
		].join("\n");
		const redacted = redactProviderLog(log, [
			"token.with[regex]",
			"s3cr3t",
			"",
		]);
		expect(redacted).not.toContain("token.with[regex]");
		expect(redacted).not.toContain("s3cr3t");
		expect(redacted.match(/\[REDACTED\]/g)).toHaveLength(4);
	});
});
