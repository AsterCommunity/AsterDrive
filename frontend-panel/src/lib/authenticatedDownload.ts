import { resolveApiResourceUrl } from "@/lib/apiUrl";
import { logger } from "@/lib/logger";
import { useAuthStore } from "@/stores/authStore";

function triggerBrowserDownload(path: string) {
	const anchor = document.createElement("a");
	anchor.href = resolveApiResourceUrl(path);
	anchor.download = "";
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
	triggerBrowserDownload(path);
}
