import { createDebouncedTask } from "@/lib/debounce";
import { logger } from "@/lib/logger";
import { authService } from "@/services/authService";
import type { UpdatePreferencesRequest } from "@/types/api";

let pending: UpdatePreferencesRequest = {};
const syncTask = createDebouncedTask(500);

export function queuePreferenceSync(patch: UpdatePreferencesRequest): void {
	Object.assign(pending, patch);
	syncTask.schedule(async () => {
		const payload = pending;
		pending = {};
		try {
			await authService.updatePreferences(payload);
		} catch (e) {
			logger.warn("preference sync failed, localStorage as fallback", e);
		}
	});
}

export function cancelPreferenceSync(): void {
	pending = {};
	syncTask.cancel();
}
