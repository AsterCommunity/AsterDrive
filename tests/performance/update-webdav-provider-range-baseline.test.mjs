import { describe, expect, test } from "bun:test";
import { execFile } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";

import {
	updateBaseline,
	validateArtifact,
} from "./update-webdav-provider-range-baseline.mjs";

const execFileAsync = promisify(execFile);

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

async function createBaselineRepository({ createBaseline = true } = {}) {
	const root = await mkdtemp(join(tmpdir(), "asterdrive-range-baseline-test-"));
	const repository = join(root, "repository");
	const baselinePath = join(
		repository,
		"tests/performance/baselines/webdav-provider-range-v1.json",
	);
	const artifactPath = join(root, "artifact.json");
	await mkdir(repository, { recursive: true });
	const originalBaseline = `${JSON.stringify(
		{
			schema_version: 1,
			baseline_version: "webdav-provider-range-v1",
			regression_policy: {
				ttfb_p95_max_ratio: 1.5,
				throughput_p50_min_ratio: 0.7,
			},
			profiles: [],
		},
		null,
		2,
	)}\n`;
	if (createBaseline) {
		await mkdir(join(repository, "tests/performance/baselines"), {
			recursive: true,
		});
		await writeFile(baselinePath, originalBaseline);
	}
	await writeFile(artifactPath, `${JSON.stringify(validArtifact())}\n`);
	await execFileAsync("git", ["init", "--quiet", repository]);
	await execFileAsync("git", [
		"-C",
		repository,
		"config",
		"user.email",
		"benchmark-test@asterdrive.invalid",
	]);
	await execFileAsync("git", [
		"-C",
		repository,
		"config",
		"user.name",
		"AsterDrive Benchmark Test",
	]);
	await execFileAsync("git", ["-C", repository, "add", "."]);
	await execFileAsync("git", [
		"-C",
		repository,
		"commit",
		"--quiet",
		"--allow-empty",
		"-m",
		"test baseline fixture",
	]);
	return { root, repository, artifactPath, baselinePath, originalBaseline };
}

describe("updateBaseline", () => {
	test("updates a baseline in a clean target worktree", async () => {
		const fixture = await createBaselineRepository();
		try {
			await updateBaseline(
				fixture.artifactPath,
				fixture.baselinePath,
				"test-profile",
			);
			const baseline = JSON.parse(await readFile(fixture.baselinePath, "utf8"));
			expect(baseline.profiles).toHaveLength(1);
			expect(baseline.profiles[0].profile).toBe("test-profile");
		} finally {
			await rm(fixture.root, { recursive: true, force: true });
		}
	});

	test("rejects a dirty target worktree without changing the baseline", async () => {
		const fixture = await createBaselineRepository();
		try {
			await writeFile(join(fixture.repository, "uncommitted.txt"), "dirty\n");

			await expect(
				updateBaseline(
					fixture.artifactPath,
					fixture.baselinePath,
					"test-profile",
				),
			).rejects.toThrow("baseline target worktree must be clean before update");
			expect(await readFile(fixture.baselinePath, "utf8")).toBe(
				fixture.originalBaseline,
			);
		} finally {
			await rm(fixture.root, { recursive: true, force: true });
		}
	});

	test("creates a baseline when its target parent directories do not exist", async () => {
		const fixture = await createBaselineRepository({ createBaseline: false });
		const baselinePath = join(
			fixture.repository,
			"new/performance/baselines/webdav-provider-range-v1.json",
		);
		try {
			await updateBaseline(
				fixture.artifactPath,
				baselinePath,
				"test-profile",
			);
			const baseline = JSON.parse(await readFile(baselinePath, "utf8"));
			expect(baseline.profiles).toHaveLength(1);
			expect(baseline.profiles[0].profile).toBe("test-profile");
		} finally {
			await rm(fixture.root, { recursive: true, force: true });
		}
	});
});
