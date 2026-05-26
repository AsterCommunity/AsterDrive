function readMetaValue(name: string) {
	if (typeof document === "undefined") return undefined;
	const value = document
		.querySelector(`meta[name="${name}"]`)
		?.getAttribute("content")
		?.trim();

	if (!value) return undefined;
	if (value.startsWith("%") && value.endsWith("%")) return undefined;
	return value;
}

export const config = {
	apiBaseUrl: import.meta.env.VITE_API_BASE_URL ?? "/api/v1",
	appName: "AsterDrive",
	appVersion:
		readMetaValue("asterdrive-version") ??
		(import.meta.env.DEV ? "dev" : "unknown"),
} as const;

export const STORAGE_KEYS = {
	themeMode: "aster-theme-mode",
	colorPreset: "aster-color-preset",
	language: "aster-language",
	displayTimeZone: "aster-display-time-zone",
	viewMode: "aster-view-mode",
	browserOpenMode: "aster-browser-open-mode",
	trashViewMode: "aster-trash-view-mode",
	userSidebarWidth: "aster-user-sidebar-width",
	uploadAutoClearCompleted: "aster-upload-auto-clear-completed",
	uploadFrontendClientId: "aster-upload-frontend-client-id",
	uploadConcurrency: "aster-upload-concurrency",
	sortBy: "aster-sort-by",
	sortOrder: "aster-sort-order",
} as const;
