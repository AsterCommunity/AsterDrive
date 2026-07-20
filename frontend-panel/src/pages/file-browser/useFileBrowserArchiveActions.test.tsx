import { act, renderHook } from "@testing-library/react";
import type { TFunction } from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useFileBrowserArchiveActions } from "@/pages/file-browser/useFileBrowserArchiveActions";
import { useDownloadStore } from "@/stores/downloadStore";
import type { FileListItem, FolderListItem } from "@/types/api";

const mocks = vi.hoisted(() => ({
	streamArchiveDownload: vi.fn(),
	workspace: { kind: "personal" as const },
}));

vi.mock("@/services/batchService", () => ({
	batchService: {
		createArchiveCompressTask: vi.fn(),
		streamArchiveDownload: (...args: unknown[]) =>
			mocks.streamArchiveDownload(...args),
	},
}));

vi.mock("@/services/fileService", () => ({
	fileService: { createArchiveExtractTask: vi.fn() },
}));

vi.mock("@/stores/workspaceStore", () => ({
	useWorkspaceStore: {
		getState: () => ({ workspace: mocks.workspace }),
	},
}));

vi.mock("@/pages/file-browser/fileBrowserLazy", () => ({
	ArchiveTaskNameDialog: { preload: vi.fn() },
}));

vi.mock("@/hooks/useApiError", () => ({ handleApiError: vi.fn() }));

vi.mock("sonner", () => ({ toast: { success: vi.fn() } }));

const files = [
	{
		id: 1,
		name: "one.txt",
		size: 10,
	},
	{
		id: 2,
		name: "two.txt",
		size: 20,
	},
] as unknown as FileListItem[];
const folders = [{ id: 3, name: "docs" }] as unknown as FolderListItem[];

function renderArchiveActions() {
	return renderHook(() =>
		useFileBrowserArchiveActions({
			clearSelection: vi.fn(),
			displayFiles: files,
			displayFolders: folders,
			t: ((key: string) => key) as TFunction,
		}),
	);
}

describe("useFileBrowserArchiveActions", () => {
	beforeEach(() => {
		mocks.streamArchiveDownload.mockReset();
		mocks.streamArchiveDownload.mockResolvedValue(undefined);
		useDownloadStore.setState({ pendingSelection: null, tasks: [] });
	});

	it("ignores empty archive selections", async () => {
		const { result } = renderArchiveActions();

		await act(() => result.current.startArchiveDownload([], []));

		expect(useDownloadStore.getState().pendingSelection).toBeNull();
		expect(mocks.streamArchiveDownload).not.toHaveBeenCalled();
	});

	it("opens the method selector when every selected item is present", async () => {
		const { result } = renderArchiveActions();

		await act(() => result.current.startArchiveDownload([1, 2], [3]));

		expect(useDownloadStore.getState().pendingSelection).toEqual({
			workspace: { kind: "personal" },
			files: [
				{ id: 1, name: "one.txt", size: 10 },
				{ id: 2, name: "two.txt", size: 20 },
			],
			folders: [{ id: 3, name: "docs" }],
		});
		expect(mocks.streamArchiveDownload).not.toHaveBeenCalled();
	});

	it("falls back to the backend archive download for stale selection ids", async () => {
		const { result } = renderArchiveActions();

		await act(() => result.current.startArchiveDownload([1, 99], [3]));

		expect(mocks.streamArchiveDownload).toHaveBeenCalledWith([1, 99], [3]);
		expect(useDownloadStore.getState().pendingSelection).toBeNull();
	});
});
