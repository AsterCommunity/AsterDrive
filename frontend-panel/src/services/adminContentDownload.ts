import { startBrowserDownload } from "@/lib/authenticatedDownload";
import { adminContentAccessService } from "@/services/adminContentAccessService";
import { useAuthStore } from "@/stores/authStore";

/** Init is the audited access boundary for both presigned and streamed attachments. */
export async function startAdminContentDownload(
	accessId: string | undefined,
	fileId: number,
	revisionId?: number,
) {
	await useAuthStore.getState().ensureFreshSession();
	const initialized = await adminContentAccessService.initDownload(fileId, {
		...(accessId === undefined ? {} : { access_id: accessId }),
		...(revisionId === undefined ? {} : { revision_id: revisionId }),
	});
	startBrowserDownload(initialized.resource.request.url, initialized.name);
}
