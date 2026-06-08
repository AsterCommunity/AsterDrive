import { act, renderHook } from "@testing-library/react";
import type { TFunction } from "i18next";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FileListItem, FolderListItem } from "@/types/api";
import { useFileBrowserPageState } from "./useFileBrowserPageState";

const mockState = vi.hoisted(() => ({
	renamePreload: vi.fn(),
	sharePreload: vi.fn(),
	targetPreload: vi.fn(),
	versionPreload: vi.fn(),
}));

vi.mock("@/pages/file-browser/fileBrowserLazy", () => ({
	BatchTargetFolderDialog: { preload: mockState.targetPreload },
	RenameDialog: { preload: mockState.renamePreload },
	ShareDialog: { preload: mockState.sharePreload },
	VersionHistoryDialog: { preload: mockState.versionPreload },
}));

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
	beforeEach(() => {
		mockState.renamePreload.mockReset();
		mockState.sharePreload.mockReset();
		mockState.targetPreload.mockReset();
		mockState.versionPreload.mockReset();
	});

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

	it("opens tag management for files and refreshes after tag changes", async () => {
		const options = createOptions();
		const refresh = vi.fn(async () => {});
		options.refresh = refresh;
		options.displayFiles = [
			{
				id: 7,
				mime_type: "text/plain",
				name: "report.txt",
				tags: [{ id: 3, name: "Reviewed", color: "#2563eb" }],
			} as FileListItem,
		];
		const { result } = renderHook(() => useFileBrowserPageState(options), {
			wrapper,
		});

		act(() => {
			result.current.handleManageTags("file", 7);
		});

		expect(result.current.tagManagerOpen).toBe(true);
		expect(result.current.tagManagerTarget).toMatchObject({
			mode: "entity",
			entityId: 7,
			entityType: "file",
			initialTags: [{ id: 3, name: "Reviewed", color: "#2563eb" }],
			name: "report.txt",
		});

		if (result.current.tagManagerTarget?.mode !== "entity") {
			throw new Error("expected entity tag manager target");
		}
		await result.current.tagManagerTarget.onChanged?.();

		expect(refresh).toHaveBeenCalledTimes(1);
	});

	it("opens tag management for folders and ignores missing targets", () => {
		const options = createOptions();
		options.displayFolders = [
			{
				id: 5,
				name: "Docs",
				tags: [{ id: 4, name: "Folder Tag", color: "#16a34a" }],
			} as FolderListItem,
		];
		const { result } = renderHook(() => useFileBrowserPageState(options), {
			wrapper,
		});

		act(() => {
			result.current.handleManageTags("file", 999);
		});

		expect(result.current.tagManagerOpen).toBe(false);
		expect(result.current.tagManagerTarget).toBeNull();

		act(() => {
			result.current.handleManageTags("folder", 5);
		});

		expect(result.current.tagManagerOpen).toBe(true);
		expect(result.current.tagManagerTarget).toMatchObject({
			mode: "entity",
			entityId: 5,
			entityType: "folder",
			initialTags: [{ id: 4, name: "Folder Tag", color: "#16a34a" }],
			name: "Docs",
		});
	});

	it("updates an open info target when the displayed item refreshes", () => {
		const options = createOptions();
		options.displayFiles = [
			{ id: 7, mime_type: "text/plain", name: "old.txt" } as FileListItem,
		];
		const { result, rerender } = renderHook(
			(nextOptions) => useFileBrowserPageState(nextOptions),
			{
				initialProps: options,
				wrapper,
			},
		);

		act(() => {
			result.current.handleInfo("file", 7);
		});

		expect(result.current.infoTarget?.file?.name).toBe("old.txt");

		const nextOptions = {
			...options,
			displayFiles: [
				{ id: 7, mime_type: "text/plain", name: "new.txt" } as FileListItem,
			],
		};
		rerender(nextOptions);

		expect(result.current.infoTarget?.file?.name).toBe("new.txt");
	});
});
