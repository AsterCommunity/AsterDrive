import { act, renderHook } from "@testing-library/react";
import type { TFunction } from "i18next";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import type { FileListItem } from "@/types/api";
import { useFileBrowserPageState } from "./useFileBrowserPageState";

const t = ((key: string) => key) as unknown as TFunction;

function createOptions() {
	return {
		displayFiles: [] as FileListItem[],
		displayFolders: [],
		loadPreviewApps: vi.fn(async () => {}),
		navigationTarget: {
			folderId: null,
			workspaceKey: "personal",
		},
		navigateTo: vi.fn(async () => {}),
		previewAppsLoaded: true,
		refresh: vi.fn(async () => {}),
		t,
	};
}

function wrapper({ children }: { children: React.ReactNode }) {
	return <MemoryRouter>{children}</MemoryRouter>;
}

describe("useFileBrowserPageState", () => {
	it("opens an auto preview when image navigation arrives without current preview state", () => {
		const options = createOptions();
		const file = {
			id: 42,
			mime_type: "image/png",
			name: "next.png",
		} as FileListItem;
		const { result } = renderHook(() => useFileBrowserPageState(options), {
			wrapper,
		});

		act(() => {
			result.current.navigatePreviewFile(file);
		});

		expect(result.current.previewState).toEqual({
			file,
			openMode: "auto",
		});
	});

	it("keeps the current open mode when navigating an existing preview", () => {
		const options = createOptions();
		const firstFile = {
			id: 41,
			mime_type: "image/png",
			name: "first.png",
		} as FileListItem;
		const nextFile = {
			id: 42,
			mime_type: "image/png",
			name: "next.png",
		} as FileListItem;
		const { result } = renderHook(() => useFileBrowserPageState(options), {
			wrapper,
		});

		act(() => {
			result.current.openPreview(firstFile, "direct");
		});
		act(() => {
			result.current.navigatePreviewFile(nextFile);
		});

		expect(result.current.previewState).toEqual({
			file: nextFile,
			openMode: "direct",
		});
	});
});
