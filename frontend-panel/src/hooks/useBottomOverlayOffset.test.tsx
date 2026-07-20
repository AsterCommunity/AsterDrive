import { renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { useBottomOverlayOffset } from "@/hooks/useBottomOverlayOffset";
import { DOWNLOAD_TASK_STATUS, useDownloadStore } from "@/stores/downloadStore";
import { useUploadAreaControlsStore } from "@/stores/uploadAreaControlsStore";

describe("useBottomOverlayOffset", () => {
	beforeEach(() => {
		useDownloadStore.setState({ tasks: [] });
		useUploadAreaControlsStore.getState().setUploadPanelPresence({
			open: false,
			visible: false,
		});
	});

	it("reserves compact space for download activity", () => {
		useDownloadStore.setState({
			tasks: [
				{
					id: "download-1",
					kind: "file",
					name: "file.txt",
					status: DOWNLOAD_TASK_STATUS.downloading,
					createdAt: 1,
					bytesReceived: 1,
					totalBytes: 2,
					speedBps: null,
					completedItems: 0,
					failedItems: 0,
					totalItems: 1,
					items: [],
				},
			],
		});

		const { result } = renderHook(() => useBottomOverlayOffset(false));
		expect(result.current).toBe("upload-compact");
	});

	it("lets an expanded upload surface reserve the whole activity region", () => {
		useUploadAreaControlsStore.getState().setUploadPanelPresence({
			open: true,
			visible: true,
		});

		const { result } = renderHook(() => useBottomOverlayOffset(true));
		expect(result.current).toBe("expanded");
	});

	it.each([
		{ selectionVisible: false, visible: false, expected: "none" },
		{ selectionVisible: true, visible: false, expected: "selection-compact" },
		{ selectionVisible: false, visible: true, expected: "upload-compact" },
		{ selectionVisible: true, visible: true, expected: "upload-compact" },
	] as const)(
		"returns $expected for selection=$selectionVisible and collapsed upload=$visible",
		({ selectionVisible, visible, expected }) => {
			useUploadAreaControlsStore.getState().setUploadPanelPresence({
				open: false,
				visible,
			});

			const { result } = renderHook(() =>
				useBottomOverlayOffset(selectionVisible),
			);
			expect(result.current).toBe(expected);
		},
	);

	it("prioritizes expanded upload over download activity and selection", () => {
		useDownloadStore.setState({
			tasks: [
				{
					id: "download-1",
					kind: "file",
					name: "file.txt",
					status: DOWNLOAD_TASK_STATUS.downloading,
					createdAt: 1,
					bytesReceived: 0,
					totalBytes: null,
					speedBps: null,
					completedItems: 0,
					failedItems: 0,
					totalItems: 1,
					items: [],
				},
			],
		});
		useUploadAreaControlsStore.getState().setUploadPanelPresence({
			open: true,
			visible: true,
		});

		const { result } = renderHook(() => useBottomOverlayOffset(true));
		expect(result.current).toBe("expanded");
	});
});
