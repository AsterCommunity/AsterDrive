export function ensureZipExtension(name: string) {
	const normalized = name.trim() || "asterdrive-download";
	return normalized.toLowerCase().endsWith(".zip")
		? normalized
		: `${normalized}.zip`;
}
