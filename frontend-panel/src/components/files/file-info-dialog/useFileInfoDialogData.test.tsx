import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiPendingError } from "@/services/http";
import type {
	FileInfo,
	FileListItem,
	FolderInfo,
	FolderListItem,
	MediaMetadataInfo,
} from "@/types/api";
import { useFileInfoDialogData } from "./useFileInfoDialogData";

const mockState = vi.hoisted(() => ({
	getFile: vi.fn(),
	getFolderInfo: vi.fn(),
	getMediaMetadata: vi.fn(),
	listFolder: vi.fn(),
	mediaDataSupportStore: {
		config: {
			enabled: true,
			kinds: {
				audio: {
					enabled: true,
					extensions: ["mp3", "flac"],
					match: "extensions",
				},
				image: {
					enabled: true,
					extensions: ["jpg"],
					match: "extensions",
				},
				video: {
					enabled: true,
					extensions: ["mp4"],
					match: "extensions",
				},
			},
			max_source_bytes: 1024 * 1024 * 1024,
			version: 1,
		},
		isLoaded: true,
		load: vi.fn(async () => {}),
	},
}));

vi.mock("@/services/fileService", () => ({
	fileService: {
		getFile: (...args: unknown[]) => mockState.getFile(...args),
		getFolderInfo: (...args: unknown[]) => mockState.getFolderInfo(...args),
		getMediaMetadata: (...args: unknown[]) =>
			mockState.getMediaMetadata(...args),
		listFolder: (...args: unknown[]) => mockState.listFolder(...args),
	},
}));

vi.mock("@/stores/mediaDataSupportStore", () => ({
	useMediaDataSupportStore: (
		selector: (state: typeof mockState.mediaDataSupportStore) => unknown,
	) => selector(mockState.mediaDataSupportStore),
}));

function fileInfo(overrides: Partial<FileInfo> = {}): FileInfo {
	return {
		blob_id: 88,
		compound_extension: null,
		created_at: "2026-01-01T00:00:00Z",
		created_by_user_id: 1,
		created_by_username: "alice",
		deleted_at: null,
		extension: "mp3",
		file_category: "audio",
		folder_id: null,
		id: 7,
		is_locked: false,
		mime_type: "audio/mpeg",
		name: "Song.mp3",
		owner_user_id: 1,
		size: 128,
		team_id: null,
		updated_at: "2026-01-02T00:00:00Z",
		...overrides,
	};
}

function fileListItem(overrides: Partial<FileListItem> = {}): FileListItem {
	return {
		compound_extension: null,
		extension: "mp3",
		file_category: "audio",
		id: 7,
		is_locked: false,
		is_shared: false,
		mime_type: "audio/mpeg",
		name: "Song.mp3",
		size: 128,
		updated_at: "2026-01-02T00:00:00Z",
		...overrides,
	};
}

function folderInfo(overrides: Partial<FolderInfo> = {}): FolderInfo {
	return {
		created_at: "2026-02-01T00:00:00Z",
		created_by_user_id: 1,
		created_by_username: "alice",
		deleted_at: null,
		id: 3,
		is_locked: false,
		name: "Projects",
		owner_user_id: 1,
		parent_id: null,
		policy_id: null,
		team_id: null,
		updated_at: "2026-02-02T00:00:00Z",
		...overrides,
	};
}

function folderListItem(
	overrides: Partial<FolderListItem> = {},
): FolderListItem {
	return {
		id: 3,
		is_locked: false,
		is_shared: false,
		name: "Projects",
		updated_at: "2026-02-02T00:00:00Z",
		...overrides,
	} as FolderListItem;
}

function audioMetadata(title = "Backend Song"): MediaMetadataInfo {
	return {
		blob_hash: "hash",
		blob_id: 88,
		error: null,
		kind: "audio",
		metadata: {
			has_embedded_picture: false,
			kind: "audio",
			title,
		},
		parser: "lofty",
		parser_version: "1",
		status: "ready",
		updated_at: "2026-01-01T00:00:00Z",
	} as MediaMetadataInfo;
}

describe("useFileInfoDialogData", () => {
	beforeEach(() => {
		mockState.getFile.mockReset();
		mockState.getFolderInfo.mockReset();
		mockState.getMediaMetadata.mockReset();
		mockState.listFolder.mockReset();
		mockState.getFile.mockResolvedValue(fileInfo());
		mockState.getFolderInfo.mockResolvedValue(folderInfo());
		mockState.listFolder.mockResolvedValue({
			files_total: 0,
			folders_total: 0,
		});
		mockState.mediaDataSupportStore.isLoaded = true;
		mockState.mediaDataSupportStore.load.mockReset();
		mockState.mediaDataSupportStore.load.mockResolvedValue(undefined);
		vi.useRealTimers();
	});

	it("uses already detailed file and folder records without refetching details", async () => {
		mockState.listFolder.mockResolvedValueOnce({
			files_total: 4,
			folders_total: 2,
		});
		mockState.getMediaMetadata.mockResolvedValueOnce(audioMetadata());

		const renderedFile = fileInfo();
		const renderedFolder = folderInfo();
		const { result } = renderHook(() =>
			useFileInfoDialogData({
				open: true,
				renderedFile,
				renderedFolder,
			}),
		);

		await waitFor(() => {
			expect(result.current.childCount).toEqual({ files: 4, folders: 2 });
		});
		expect(result.current.resolvedFile).toBe(renderedFile);
		expect(result.current.resolvedFolder).toBe(renderedFolder);
		expect(mockState.getFile).not.toHaveBeenCalled();
		expect(mockState.getFolderInfo).not.toHaveBeenCalled();
		expect(mockState.getMediaMetadata).toHaveBeenCalledWith(7, {
			signal: expect.any(AbortSignal),
		});
		expect(result.current.mediaMetadata).toEqual(audioMetadata());
		expect(result.current.renderedMediaMetadataKind).toBe("audio");
		expect(result.current.canRequestMediaMetadata).toBe(true);
	});

	it("loads list-item details and stores null after failed detail requests", async () => {
		mockState.getFile
			.mockResolvedValueOnce(fileInfo({ name: "Resolved.mp3" }))
			.mockRejectedValueOnce(new Error("file unavailable"));
		mockState.getFolderInfo
			.mockResolvedValueOnce(folderInfo({ name: "Resolved Folder" }))
			.mockRejectedValueOnce(new Error("folder unavailable"));
		mockState.listFolder
			.mockResolvedValueOnce({ files_total: 1, folders_total: 0 })
			.mockRejectedValueOnce(new Error("count unavailable"));
		mockState.getMediaMetadata.mockResolvedValue(audioMetadata());

		const renderedFile = fileListItem();
		const renderedFolder = folderListItem();
		const { rerender, result } = renderHook(
			(props: { open: boolean }) =>
				useFileInfoDialogData({
					open: props.open,
					renderedFile,
					renderedFolder,
				}),
			{ initialProps: { open: true } },
		);

		expect(result.current.fileDetailsLoading).toBe(true);
		expect(result.current.folderDetailsLoading).toBe(true);
		await waitFor(() => {
			expect(result.current.resolvedFile?.name).toBe("Resolved.mp3");
			expect(result.current.resolvedFolder?.name).toBe("Resolved Folder");
		});
		expect(result.current.childCount).toEqual({ files: 1, folders: 0 });

		rerender({ open: false });
		await waitFor(() => {
			expect(result.current.resolvedFile).toBeNull();
			expect(result.current.resolvedFolder).toBeNull();
			expect(result.current.childCount).toBeNull();
		});

		rerender({ open: true });
		await waitFor(() => {
			expect(mockState.getFile).toHaveBeenCalledTimes(2);
			expect(mockState.getFolderInfo).toHaveBeenCalledTimes(2);
			expect(mockState.listFolder).toHaveBeenCalledTimes(2);
		});
		await waitFor(() => {
			expect(result.current.fileDetailsLoading).toBe(false);
			expect(result.current.folderDetailsLoading).toBe(false);
		});
		expect(result.current.resolvedFile).toBeNull();
		expect(result.current.resolvedFolder).toBeNull();
		expect(result.current.childCount).toBeNull();
	});

	it("loads media support before requesting metadata and resets unsupported files", () => {
		mockState.mediaDataSupportStore.isLoaded = false;
		const renderedFile = fileListItem({
			extension: "txt",
			file_category: "document",
			mime_type: "text/plain",
			name: "notes.txt",
		});

		const { result } = renderHook(() =>
			useFileInfoDialogData({
				open: true,
				renderedFile,
			}),
		);

		expect(mockState.mediaDataSupportStore.load).toHaveBeenCalledTimes(1);
		expect(mockState.getMediaMetadata).not.toHaveBeenCalled();
		expect(result.current.canRequestMediaMetadata).toBe(false);
		expect(result.current.renderedMediaMetadataKind).toBeNull();
		expect(result.current.mediaMetadata).toBeNull();
		expect(result.current.mediaMetadataLoading).toBe(false);
	});

	it("retries pending media metadata with clamped delays and stops at the retry cap", async () => {
		vi.useFakeTimers();
		mockState.getMediaMetadata.mockRejectedValue(
			new ApiPendingError("busy", 60),
		);
		const renderedFile = fileListItem();

		const { result } = renderHook(() =>
			useFileInfoDialogData({
				open: true,
				renderedFile,
			}),
		);

		await act(async () => {
			await Promise.resolve();
		});
		expect(result.current.mediaMetadataLoading).toBe(true);
		expect(mockState.getMediaMetadata).toHaveBeenCalledTimes(1);

		for (let i = 0; i < 12; i += 1) {
			await act(async () => {
				await vi.advanceTimersByTimeAsync(30_000);
			});
		}
		await act(async () => {
			await Promise.resolve();
		});

		expect(mockState.getMediaMetadata).toHaveBeenCalledTimes(13);
		expect(result.current.mediaMetadata).toBeNull();
		expect(result.current.mediaMetadataLoading).toBe(false);
	});

	it("uses the fallback retry delay for invalid pending retry-after values", async () => {
		vi.useFakeTimers();
		mockState.getMediaMetadata
			.mockRejectedValueOnce(new ApiPendingError("busy", Number.NaN))
			.mockResolvedValueOnce(audioMetadata("Retried"));
		const renderedFile = fileListItem();

		const { result } = renderHook(() =>
			useFileInfoDialogData({
				open: true,
				renderedFile,
			}),
		);

		await act(async () => {
			await Promise.resolve();
		});
		await act(async () => {
			await vi.advanceTimersByTimeAsync(1_999);
		});
		expect(mockState.getMediaMetadata).toHaveBeenCalledTimes(1);

		await act(async () => {
			await vi.advanceTimersByTimeAsync(1);
		});
		await act(async () => {
			await Promise.resolve();
		});

		expect(result.current.mediaMetadata).toEqual(audioMetadata("Retried"));
	});

	it("aborts metadata requests and clears pending retry timers on unmount", async () => {
		vi.useFakeTimers();
		mockState.getMediaMetadata.mockRejectedValueOnce(
			new ApiPendingError("busy", 2),
		);
		const renderedFile = fileListItem();

		const { unmount } = renderHook(() =>
			useFileInfoDialogData({
				open: true,
				renderedFile,
			}),
		);

		await act(async () => {
			await Promise.resolve();
		});
		const firstCallOptions = mockState.getMediaMetadata.mock.calls[0]?.[1] as
			| { signal: AbortSignal }
			| undefined;

		unmount();
		expect(firstCallOptions?.signal.aborted).toBe(true);

		await act(async () => {
			await vi.advanceTimersByTimeAsync(2_000);
		});
		expect(mockState.getMediaMetadata).toHaveBeenCalledTimes(1);
	});
});
