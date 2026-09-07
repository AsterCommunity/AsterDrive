import { beforeEach, describe, expect, it, vi } from "vitest";
import { createClipboardSlice } from "@/stores/fileStore/clipboardSlice";

const mockState = vi.hoisted(() => ({
	batchCopy: vi.fn(),
	batchMove: vi.fn(),
	copyFile: vi.fn(),
	copyFolder: vi.fn(),
	moveFile: vi.fn(),
	moveFolder: vi.fn(),
	refresh: vi.fn(),
}));

vi.mock("@/services/batchService", () => ({
	batchService: {
		batchCopy: (...args: unknown[]) => mockState.batchCopy(...args),
		batchMove: (...args: unknown[]) => mockState.batchMove(...args),
	},
	singleOperationResult: (promise: Promise<unknown>) =>
		promise.then(() => ({ succeeded: 1, failed: 0, errors: [] })),
}));

vi.mock("@/services/fileService", () => ({
	fileService: {
		copyFile: (...args: unknown[]) => mockState.copyFile(...args),
		copyFolder: (...args: unknown[]) => mockState.copyFolder(...args),
		moveFile: (...args: unknown[]) => mockState.moveFile(...args),
		moveFolder: (...args: unknown[]) => mockState.moveFolder(...args),
	},
}));

function createClipboardState(clipboard: unknown) {
	let state: Record<string, unknown> = {
		clipboard,
		clearSelection: vi.fn(),
		currentFolderId: 9,
		refresh: mockState.refresh,
		workspaceRequestRevision: 1,
		sortBy: "name",
		sortOrder: "asc",
	};
	const set = (update: Record<string, unknown>) => {
		state = { ...state, ...update };
	};
	const get = () => state;
	const slice = createClipboardSlice(set as never, get as never);
	return { slice, state };
}

describe("clipboard copy and move dispatch", () => {
	beforeEach(() => {
		mockState.batchCopy.mockReset().mockResolvedValue({
			succeeded: 2,
			failed: 0,
			errors: [],
		});
		mockState.batchMove.mockReset().mockResolvedValue({
			succeeded: 1,
			failed: 0,
			errors: [],
		});
		mockState.copyFile.mockReset().mockResolvedValue({ id: 1 });
		mockState.copyFolder.mockReset().mockResolvedValue({ id: 2 });
		mockState.moveFile.mockReset().mockResolvedValue({ id: 1 });
		mockState.moveFolder.mockReset().mockResolvedValue({ id: 2 });
		mockState.refresh.mockReset().mockResolvedValue(undefined);
	});

	it("uses the single-file copy endpoint for one copied file", async () => {
		const { slice } = createClipboardState({
			fileIds: [1],
			folderIds: [],
			mode: "copy",
		});

		await slice.clipboardPaste();

		expect(mockState.copyFile).toHaveBeenCalledWith(1, 9);
		expect(mockState.batchCopy).not.toHaveBeenCalled();
		expect(mockState.refresh).toHaveBeenCalledWith(9);
	});

	it("uses the single-folder copy endpoint for one copied folder", async () => {
		const { slice } = createClipboardState({
			fileIds: [],
			folderIds: [2],
			mode: "copy",
		});

		await slice.clipboardPaste();

		expect(mockState.copyFolder).toHaveBeenCalledWith(2, 9);
		expect(mockState.batchCopy).not.toHaveBeenCalled();
		expect(mockState.refresh).toHaveBeenCalledWith(9);
	});

	it("uses batch copy for multiple copied resources", async () => {
		const { slice } = createClipboardState({
			fileIds: [1, 2],
			folderIds: [],
			mode: "copy",
		});

		await slice.clipboardPaste();

		expect(mockState.batchCopy).toHaveBeenCalledWith([1, 2], [], 9);
		expect(mockState.copyFile).not.toHaveBeenCalled();
		expect(mockState.refresh).toHaveBeenCalledWith(9);
	});

	it("uses the single-file move endpoint for one cut file", async () => {
		const { slice } = createClipboardState({
			fileIds: [1],
			folderIds: [],
			mode: "cut",
		});

		await slice.clipboardPaste();

		expect(mockState.moveFile).toHaveBeenCalledWith(1, 9);
		expect(mockState.batchMove).not.toHaveBeenCalled();
		expect(mockState.copyFile).not.toHaveBeenCalled();
		expect(mockState.refresh).toHaveBeenCalledWith(9);
	});

	it("uses the single-folder move endpoint for one cut folder", async () => {
		const { slice } = createClipboardState({
			fileIds: [],
			folderIds: [2],
			mode: "cut",
		});

		await slice.clipboardPaste();

		expect(mockState.moveFolder).toHaveBeenCalledWith(2, 9);
		expect(mockState.batchMove).not.toHaveBeenCalled();
		expect(mockState.moveFile).not.toHaveBeenCalled();
		expect(mockState.refresh).toHaveBeenCalledWith(9);
	});

	it("uses batch move for multiple cut resources", async () => {
		const { slice } = createClipboardState({
			fileIds: [1],
			folderIds: [2],
			mode: "cut",
		});

		await slice.clipboardPaste();

		expect(mockState.batchMove).toHaveBeenCalledWith([1], [2], 9);
		expect(mockState.moveFile).not.toHaveBeenCalled();
		expect(mockState.moveFolder).not.toHaveBeenCalled();
		expect(mockState.refresh).toHaveBeenCalledWith(9);
	});
});
