import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname } from "node:path";

const [, , artifactPath, baselinePath, profile] = process.argv;

if (!artifactPath || !baselinePath || !profile) {
	throw new Error(
		"usage: bun tests/performance/update-webdav-provider-range-baseline.mjs <artifact.json> <baseline.json> <profile>",
	);
}

const artifact = JSON.parse(await readFile(artifactPath, "utf8"));
if (artifact.schema_version !== 1 || artifact.status !== "completed") {
	throw new Error("baseline input must be a completed schema-v1 artifact");
}

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
await writeFile(baselinePath, `${JSON.stringify(baseline, null, 2)}\n`);
console.log(
	`updated ${baselinePath}: ${profile}/${artifact.provider}/${artifact.fixture.payload_bytes}/${artifact.fixture.range_bytes}`,
);
