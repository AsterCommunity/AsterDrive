import { execFile } from "node:child_process";
import {
	access,
	mkdir,
	readFile,
	rename,
	unlink,
	writeFile,
} from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

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

function requireInteger(value, name, minimum) {
	if (!Number.isInteger(value) || value < minimum) {
		throw new Error(`${name} must be an integer greater than or equal to ${minimum}`);
	}
}

function requireFiniteNumber(value, name, minimum) {
	if (!Number.isFinite(value) || value < minimum) {
		throw new Error(`${name} must be a finite number greater than or equal to ${minimum}`);
	}
}

export function validateArtifactMetadata(artifact) {
	requireRecord(artifact, "artifact");
	if (artifact.schema_version !== 1) {
		throw new Error("artifact must use schema v1");
	}
	requireNonEmptyString(artifact.provider, "artifact.provider");
	requireNonEmptyString(artifact.generated_at, "artifact.generated_at");
	requireNonEmptyString(artifact.git_revision, "artifact.git_revision");
	if (artifact.git_dirty !== false) {
		throw new Error("baseline input must come from a clean Git worktree");
	}

	const fixture = requireRecord(artifact.fixture, "artifact.fixture");
	requireInteger(fixture.payload_bytes, "artifact.fixture.payload_bytes", 1);
	requireInteger(fixture.range_bytes, "artifact.fixture.range_bytes", 2);

	const sampling = requireRecord(artifact.sampling, "artifact.sampling");
	requireInteger(sampling.warmups, "artifact.sampling.warmups", 0);
	requireInteger(sampling.samples, "artifact.sampling.samples", 1);
	requireInteger(
		sampling.read_buffer_bytes,
		"artifact.sampling.read_buffer_bytes",
		1,
	);

	const machine = requireRecord(artifact.machine, "artifact.machine");
	requireNonEmptyString(
		machine.build_profile,
		"artifact.machine.build_profile",
	);
	requireNonEmptyString(machine.os, "artifact.machine.os");
	requireNonEmptyString(machine.architecture, "artifact.machine.architecture");
	return artifact;
}

export function validateArtifact(artifact) {
	validateArtifactMetadata(artifact);
	if (artifact.status !== "completed") {
		throw new Error("baseline input must be a completed schema-v1 artifact");
	}

	const scenarioEntries = Object.entries(
		requireRecord(artifact.scenarios, "artifact.scenarios"),
	);
	if (scenarioEntries.length === 0) {
		throw new Error("artifact.scenarios must contain at least one scenario");
	}
	for (const [name, scenarioValue] of scenarioEntries) {
		const scenario = requireRecord(
			scenarioValue,
			`artifact.scenarios.${name}`,
		);
		const summary = requireRecord(
			scenario.summary,
			`artifact.scenarios.${name}.summary`,
		);
		const ttfb = requireRecord(
			summary.ttfb_ms,
			`artifact.scenarios.${name}.summary.ttfb_ms`,
		);
		const throughput = requireRecord(
			summary.throughput_bytes_per_second,
			`artifact.scenarios.${name}.summary.throughput_bytes_per_second`,
		);
		requireFiniteNumber(
			ttfb.p95,
			`artifact.scenarios.${name}.summary.ttfb_ms.p95`,
			0,
		);
		requireFiniteNumber(
			throughput.p50,
			`artifact.scenarios.${name}.summary.throughput_bytes_per_second.p50`,
			Number.MIN_VALUE,
		);
	}
	return artifact;
}

async function assertCleanBaselineWorktree(baselinePath) {
	let baselineDirectory = dirname(resolve(baselinePath));
	while (true) {
		try {
			await access(baselineDirectory);
			break;
		} catch (error) {
			if (error?.code !== "ENOENT") {
				throw error;
			}
			const parent = dirname(baselineDirectory);
			if (parent === baselineDirectory) {
				throw new Error(
					`baseline path must belong to a Git worktree: ${baselinePath}`,
					{ cause: error },
				);
			}
			baselineDirectory = parent;
		}
	}
	let worktreeRoot;
	try {
		({ stdout: worktreeRoot } = await execFileAsync(
			"git",
			["-C", baselineDirectory, "rev-parse", "--show-toplevel"],
			{ encoding: "utf8" },
		));
	} catch (error) {
		throw new Error(
			`baseline path must belong to a Git worktree: ${baselinePath}`,
			{ cause: error },
		);
	}

	const { stdout: status } = await execFileAsync(
		"git",
		[
			"-C",
			worktreeRoot.trim(),
			"status",
			"--porcelain",
			"--untracked-files=normal",
		],
		{ encoding: "utf8" },
	);
	if (status !== "") {
		throw new Error(
			`baseline target worktree must be clean before update: ${worktreeRoot.trim()}`,
		);
	}
}

export async function updateBaseline(artifactPath, baselinePath, profile) {
	const artifact = validateArtifact(
		JSON.parse(await readFile(artifactPath, "utf8")),
	);
	await assertCleanBaselineWorktree(baselinePath);

	let baseline;
	try {
		baseline = JSON.parse(await readFile(baselinePath, "utf8"));
	} catch (error) {
		if (error?.code !== "ENOENT") {
			throw error;
		}
		baseline = {
			schema_version: 1,
			baseline_version: "webdav-provider-range-v1",
			regression_policy: {
				ttfb_p95_max_ratio: 1.5,
				throughput_p50_min_ratio: 0.7,
			},
			profiles: [],
		};
	}

	if (baseline.schema_version !== 1 || !Array.isArray(baseline.profiles)) {
		throw new Error("baseline file must use the webdav-provider-range schema v1");
	}

	const scenarios = Object.fromEntries(
		Object.entries(artifact.scenarios).map(([name, scenario]) => [
			name,
			{
				ttfb_p95_ms: scenario.summary.ttfb_ms.p95,
				throughput_p50_bytes_per_second:
					scenario.summary.throughput_bytes_per_second.p50,
			},
		]),
	);

	const nextProfile = {
		profile,
		provider: artifact.provider,
		payload_bytes: artifact.fixture.payload_bytes,
		range_bytes: artifact.fixture.range_bytes,
		sampling: artifact.sampling,
		machine: artifact.machine,
		generated_at: artifact.generated_at,
		git_revision: artifact.git_revision,
		git_dirty: artifact.git_dirty,
		scenarios,
	};

	baseline.profiles = baseline.profiles
		.filter(
			(candidate) =>
				candidate.profile !== profile ||
				candidate.provider !== artifact.provider ||
				candidate.payload_bytes !== artifact.fixture.payload_bytes ||
				candidate.range_bytes !== artifact.fixture.range_bytes,
		)
		.concat(nextProfile)
		.sort((left, right) =>
			[
				left.profile,
				left.provider,
				left.payload_bytes,
				left.range_bytes,
			]
				.join(":")
				.localeCompare(
					[
						right.profile,
						right.provider,
						right.payload_bytes,
						right.range_bytes,
					].join(":"),
				),
		);

	await mkdir(dirname(baselinePath), { recursive: true });
	const temporaryBaselinePath = `${baselinePath}.${process.pid}.tmp`;
	try {
		await writeFile(
			temporaryBaselinePath,
			`${JSON.stringify(baseline, null, 2)}\n`,
		);
		await rename(temporaryBaselinePath, baselinePath);
	} catch (error) {
		try {
			await unlink(temporaryBaselinePath);
		} catch (cleanupError) {
			if (cleanupError?.code !== "ENOENT") {
				throw new AggregateError(
					[error, cleanupError],
					"baseline update and temporary-file cleanup both failed",
				);
			}
		}
		throw error;
	}
	console.log(
		`updated ${baselinePath}: ${profile}/${artifact.provider}/${artifact.fixture.payload_bytes}/${artifact.fixture.range_bytes}`,
	);
}

if (import.meta.main) {
	const [, , artifactPath, baselinePath, profile] = process.argv;
	if (!artifactPath || !baselinePath || !profile) {
		throw new Error(
			"usage: bun tests/performance/update-webdav-provider-range-baseline.mjs <artifact.json> <baseline.json> <profile>",
		);
	}
	await updateBaseline(artifactPath, baselinePath, profile);
}
