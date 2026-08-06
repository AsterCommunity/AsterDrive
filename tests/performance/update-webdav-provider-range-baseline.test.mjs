import { describe, expect, test } from "bun:test";

import { validateArtifact } from "./update-webdav-provider-range-baseline.mjs";

function validArtifact() {
	return {
		schema_version: 1,
		status: "completed",
		provider: "local",
		generated_at: "2026-08-07T00:00:00Z",
		git_revision: "0123456789abcdef",
		git_dirty: false,
		fixture: {
			payload_bytes: 1024,
			range_bytes: 16,
		},
		sampling: {
			warmups: 1,
			samples: 2,
			read_buffer_bytes: 64,
		},
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

describe("validateArtifact", () => {
	test("accepts a complete clean artifact", () => {
		expect(validateArtifact(validArtifact())).toEqual(validArtifact());
	});

	test("rejects missing fixture metadata", () => {
		const artifact = validArtifact();
		delete artifact.fixture;
		expect(() => validateArtifact(artifact)).toThrow("artifact.fixture");
	});

	test("rejects an empty scenario collection", () => {
		const artifact = validArtifact();
		artifact.scenarios = {};
		expect(() => validateArtifact(artifact)).toThrow(
			"artifact.scenarios must contain at least one scenario",
		);
	});

	test("rejects null metrics", () => {
		const artifact = validArtifact();
		artifact.scenarios.full_get.summary.ttfb_ms.p95 = null;
		expect(() => validateArtifact(artifact)).toThrow(
			"artifact.scenarios.full_get.summary.ttfb_ms.p95",
		);
	});

	test("rejects non-finite metrics", () => {
		const artifact = validArtifact();
		artifact.scenarios.full_get.summary.throughput_bytes_per_second.p50 =
			Number.POSITIVE_INFINITY;
		expect(() => validateArtifact(artifact)).toThrow(
			"artifact.scenarios.full_get.summary.throughput_bytes_per_second.p50",
		);
	});

	test("rejects dirty baseline provenance", () => {
		const artifact = validArtifact();
		artifact.git_dirty = true;
		expect(() => validateArtifact(artifact)).toThrow(
			"baseline input must come from a clean Git worktree",
		);
	});
});
