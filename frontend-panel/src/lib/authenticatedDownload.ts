import { resolveApiResourceUrl } from "@/lib/apiUrl";
import { logger } from "@/lib/logger";
import { useAuthStore } from "@/stores/authStore";

/** Trigger a browser download without navigating the current page. */
export function startBrowserDownload(path: string, filename?: string) {
	const anchor = document.createElement("a");
	anchor.href = resolveApiResourceUrl(path);
	if (filename) anchor.download = filename;
	anchor.rel = "noopener";
	document.body.append(anchor);
	anchor.click();
	anchor.remove();
}

/**
 * Ensures the session is fresh before triggering the download. Refresh failures
 * are logged and rethrown so callers can surface the failure and no download is
 * started with a stale token.
 */
export async function startAuthenticatedDownload(path: string): Promise<void> {
	try {
		await useAuthStore.getState().ensureFreshSession();
	} catch (error) {
		logger.error("authenticated download session refresh failed", path, error);
		throw error;
	}
	startBrowserDownload(path);
}
