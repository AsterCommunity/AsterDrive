import { readFile, writeFile } from "node:fs/promises";

export function redactProviderLog(log, secretValues) {
	const values = [...new Set(secretValues.filter((value) => value !== ""))].sort(
		(left, right) => right.length - left.length,
	);
	return values.reduce(
		(redacted, value) => redacted.replaceAll(value, "[REDACTED]"),
		log,
	);
}

export async function redactProviderLogFile(inputPath, outputPath, secretNames) {
	const log = await readFile(inputPath, "utf8");
	const secretValues = secretNames.map((name) => process.env[name] ?? "");
	await writeFile(outputPath, redactProviderLog(log, secretValues));
}

if (import.meta.main) {
	const [, , inputPath, outputPath, ...secretNames] = process.argv;
	if (!inputPath || !outputPath) {
		throw new Error(
			"usage: bun tests/performance/redact-webdav-provider-range-log.mjs <input.log> <output.log> [SECRET_ENV_NAME...]",
		);
	}
	await redactProviderLogFile(inputPath, outputPath, secretNames);
}
