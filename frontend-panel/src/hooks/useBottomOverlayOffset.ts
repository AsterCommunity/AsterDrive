import type { BottomOverlayOffset } from "@/lib/constants";
import { useDownloadStore } from "@/stores/downloadStore";
import { useUploadAreaControlsStore } from "@/stores/uploadAreaControlsStore";

export function useBottomOverlayOffset(selectionVisible = false) {
	const uploadPanelPresence = useUploadAreaControlsStore(
		(state) => state.uploadPanelPresence,
	);
	const hasDownloadActivity = useDownloadStore(
		(state) => state.tasks.length > 0,
	);

	const offset: BottomOverlayOffset = uploadPanelPresence.open
		? "expanded"
		: uploadPanelPresence.visible || hasDownloadActivity
			? "upload-compact"
			: selectionVisible
				? "selection-compact"
				: "none";
	return offset;
}
