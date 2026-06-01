export const OFFLINE_DOWNLOAD_ENGINE_REGISTRY_KEY =
	"offline_download_engine_registry_json";
export const OFFLINE_DOWNLOAD_ENGINE_REGISTRY_VERSION = 1;

export type OfflineDownloadEngineKind = "builtin" | "aria2";

export interface OfflineDownloadEngineEditorItem {
	kind: OfflineDownloadEngineKind;
	enabled: boolean;
}

export interface OfflineDownloadEngineEditorConfig {
	version: number;
	engines: OfflineDownloadEngineEditorItem[];
}

export interface OfflineDownloadEngineConfigIssue {
	key: string;
	values?: Record<string, string | number>;
}

const DEFAULT_ENGINE_ORDER: OfflineDownloadEngineKind[] = ["builtin", "aria2"];

export function defaultOfflineDownloadEngineConfig(): OfflineDownloadEngineEditorConfig {
	return {
		version: OFFLINE_DOWNLOAD_ENGINE_REGISTRY_VERSION,
		engines: [
			{ kind: "builtin", enabled: true },
			{ kind: "aria2", enabled: false },
		],
	};
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isEngineKind(value: unknown): value is OfflineDownloadEngineKind {
	return value === "builtin" || value === "aria2";
}

export function parseOfflineDownloadEngineConfig(
	value: string,
): OfflineDownloadEngineEditorConfig {
	const parsed: unknown = value.trim()
		? JSON.parse(value)
		: defaultOfflineDownloadEngineConfig();
	if (!isRecord(parsed)) {
		throw new Error("offline download engine registry must be an object");
	}

	const enginesValue = Array.isArray(parsed.engines) ? parsed.engines : [];
	const engines = enginesValue.flatMap(
		(item): OfflineDownloadEngineEditorItem[] => {
			if (!isRecord(item) || !isEngineKind(item.kind)) {
				return [];
			}
			return [
				{
					kind: item.kind,
					enabled: typeof item.enabled === "boolean" ? item.enabled : true,
				},
			];
		},
	);
	const seen = new Set(engines.map((engine) => engine.kind));
	for (const kind of DEFAULT_ENGINE_ORDER) {
		if (!seen.has(kind)) {
			engines.push({ kind, enabled: kind === "builtin" });
		}
	}

	return {
		version:
			typeof parsed.version === "number"
				? parsed.version
				: OFFLINE_DOWNLOAD_ENGINE_REGISTRY_VERSION,
		engines,
	};
}

export function serializeOfflineDownloadEngineConfig(
	config: OfflineDownloadEngineEditorConfig,
) {
	return JSON.stringify(
		{
			version: OFFLINE_DOWNLOAD_ENGINE_REGISTRY_VERSION,
			engines: config.engines.map((engine) => ({
				kind: engine.kind,
				enabled: engine.enabled,
			})),
		},
		null,
		2,
	);
}

export function getOfflineDownloadEngineConfigIssues(
	config: OfflineDownloadEngineEditorConfig,
): OfflineDownloadEngineConfigIssue[] {
	const issues: OfflineDownloadEngineConfigIssue[] = [];
	if (config.version !== OFFLINE_DOWNLOAD_ENGINE_REGISTRY_VERSION) {
		issues.push({
			key: "offline_download_engine_editor_invalid_version",
			values: { version: OFFLINE_DOWNLOAD_ENGINE_REGISTRY_VERSION },
		});
	}
	const seen = new Set<OfflineDownloadEngineKind>();
	for (const engine of config.engines) {
		if (seen.has(engine.kind)) {
			issues.push({
				key: "offline_download_engine_editor_duplicate_engine",
				values: { kind: engine.kind },
			});
		}
		seen.add(engine.kind);
	}
	return issues;
}

export function getOfflineDownloadEngineConfigIssuesFromString(value: string) {
	try {
		return getOfflineDownloadEngineConfigIssues(
			parseOfflineDownloadEngineConfig(value),
		);
	} catch {
		return [{ key: "offline_download_engine_editor_invalid_json" }];
	}
}
