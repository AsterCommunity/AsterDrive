import { describe, expect, it, vi } from "vitest";
import {
	type FileBrowserItemActionHandlers,
	resolveFileBrowserItemMenuProps,
} from "@/components/files/fileBrowserItemActionResolver";
import type { FileListItem, FolderListItem } from "@/types/api";

function createHandlers(): FileBrowserItemActionHandlers {
	return {
		onArchiveCompress: vi.fn(),
		onArchiveDownload: vi.fn(),
		onArchiveExtract: vi.fn(),
		onCopy: vi.fn(),
		onDelete: vi.fn(),
		onDownload: vi.fn(),
		onFileChooseOpenMethod: vi.fn(),
		onFileClick: vi.fn(),
		onFileOpen: vi.fn(),
		onFolderOpen: vi.fn(),
		onFolderPolicy: vi.fn(),
		onGoToLocation: vi.fn(),
		onInfo: vi.fn(),
		onManageTags: vi.fn(),
		onMove: vi.fn(),
		onRename: vi.fn(),
		onShare: vi.fn(),
		onToggleLock: vi.fn(),
		onVersions: vi.fn(),
	};
}

function emptySelection() {
	return {
		selectedFileIds: new Set<number>(),
		selectedFolderIds: new Set<number>(),
	};
}

describe("fileBrowserItemActionResolver", () => {
	it("maps writable file actions to item callbacks", () => {
		const handlers = createHandlers();
		const file = { id: 7, name: "bundle.zip", is_locked: true } as FileListItem;
		const props = resolveFileBrowserItemMenuProps({
			handlers,
			isFolder: false,
			item: file,
			selection: emptySelection(),
		});

		props.onOpen?.();
		props.onChooseOpenMethod?.();
		props.onDownload?.();
		props.onArchiveExtract?.();
		props.onArchiveCompress?.();
		props.onPageShare?.();
		props.onDirectShare?.();
		props.onCopy?.();
		props.onGoToLocation?.();
		props.onManageTags?.();
		props.onMove?.();
		props.onRename?.();
		props.onToggleLock?.();
		props.onDelete?.();
		props.onVersions?.();
		props.onInfo?.();

		expect(props).toMatchObject({
			isFolder: false,
			isLocked: true,
		});
		expect(handlers.onFileOpen).toHaveBeenCalledWith(file);
		expect(handlers.onFileChooseOpenMethod).toHaveBeenCalledWith(file);
		expect(handlers.onDownload).toHaveBeenCalledWith(7, "bundle.zip");
		expect(handlers.onArchiveExtract).toHaveBeenCalledWith(7);
		expect(handlers.onArchiveCompress).toHaveBeenCalledWith("file", 7);
		expect(handlers.onShare).toHaveBeenNthCalledWith(1, {
			fileId: 7,
			initialMode: "page",
			name: "bundle.zip",
		});
		expect(handlers.onShare).toHaveBeenNthCalledWith(2, {
			fileId: 7,
			initialMode: "direct",
			name: "bundle.zip",
		});
		expect(handlers.onCopy).toHaveBeenCalledWith("file", 7);
		expect(handlers.onGoToLocation).toHaveBeenCalledWith(file);
		expect(handlers.onManageTags).toHaveBeenCalledWith("file", 7);
		expect(handlers.onMove).toHaveBeenCalledWith("file", 7);
		expect(handlers.onRename).toHaveBeenCalledWith("file", 7, "bundle.zip");
		expect(handlers.onToggleLock).toHaveBeenCalledWith("file", 7, true);
		expect(handlers.onDelete).toHaveBeenCalledWith("file", 7);
		expect(handlers.onVersions).toHaveBeenCalledWith(7);
		expect(handlers.onInfo).toHaveBeenCalledWith("file", 7);
	});

	it("limits read-only file actions to open and download", () => {
		const handlers = createHandlers();
		handlers.onFileOpen = undefined;
		const file = { id: 7, name: "bundle.zip", is_locked: true } as FileListItem;
		const props = resolveFileBrowserItemMenuProps({
			handlers,
			isFolder: false,
			item: file,
			readOnly: true,
			selection: emptySelection(),
		});

		props.onOpen?.();
		props.onDownload?.();

		expect(props.onOpen).toBeTypeOf("function");
		expect(props.onDownload).toBeTypeOf("function");
		expect(props.onArchiveCompress).toBeUndefined();
		expect(props.onDelete).toBeUndefined();
		expect(props.onVersions).toBeUndefined();
		expect(props.isLocked).toBe(false);
		expect(handlers.onFileClick).toHaveBeenCalledWith(file);
		expect(handlers.onDownload).toHaveBeenCalledWith(7, "bundle.zip");
	});

	it("maps writable folder actions and folder-only policy", () => {
		const handlers = createHandlers();
		const folder = { id: 3, name: "Docs", is_locked: false } as FolderListItem;
		const props = resolveFileBrowserItemMenuProps({
			handlers,
			isFolder: true,
			item: folder,
			selection: emptySelection(),
		});

		props.onOpen?.();
		props.onPageShare?.();
		props.onArchiveDownload?.();
		props.onArchiveCompress?.();
		props.onCopy?.();
		props.onManageTags?.();
		props.onMove?.();
		props.onFolderPolicy?.();
		props.onRename?.();
		props.onToggleLock?.();
		props.onDelete?.();
		props.onInfo?.();

		expect(props).toMatchObject({
			isFolder: true,
			isLocked: false,
		});
		expect(props.onDirectShare).toBeUndefined();
		expect(handlers.onFolderOpen).toHaveBeenCalledWith(3, "Docs");
		expect(handlers.onShare).toHaveBeenCalledWith({
			folderId: 3,
			initialMode: "page",
			name: "Docs",
		});
		expect(handlers.onArchiveDownload).toHaveBeenCalledWith(3);
		expect(handlers.onArchiveCompress).toHaveBeenCalledWith("folder", 3);
		expect(handlers.onCopy).toHaveBeenCalledWith("folder", 3);
		expect(handlers.onManageTags).toHaveBeenCalledWith("folder", 3);
		expect(handlers.onMove).toHaveBeenCalledWith("folder", 3);
		expect(handlers.onFolderPolicy).toHaveBeenCalledWith(folder);
		expect(handlers.onRename).toHaveBeenCalledWith("folder", 3, "Docs");
		expect(handlers.onToggleLock).toHaveBeenCalledWith("folder", 3, false);
		expect(handlers.onDelete).toHaveBeenCalledWith("folder", 3);
		expect(handlers.onInfo).toHaveBeenCalledWith("folder", 3);
	});

	it("limits read-only folder actions to open and archive download", () => {
		const handlers = createHandlers();
		const props = resolveFileBrowserItemMenuProps({
			handlers,
			isFolder: true,
			item: { id: 3, name: "Docs", is_locked: true } as FolderListItem,
			readOnly: true,
			selection: emptySelection(),
		});

		props.onOpen?.();
		props.onArchiveDownload?.();

		expect(props.isLocked).toBe(false);
		expect(props.onDelete).toBeUndefined();
		expect(handlers.onFolderOpen).toHaveBeenCalledWith(3, "Docs");
		expect(handlers.onArchiveDownload).toHaveBeenCalledWith(3);
		expect(handlers.onDelete).not.toHaveBeenCalled();
	});

	it("omits optional writable actions when handlers are unavailable", () => {
		const handlers = createHandlers();
		handlers.onArchiveCompress = undefined;
		handlers.onArchiveDownload = undefined;
		handlers.onArchiveExtract = undefined;
		handlers.onCopy = undefined;
		handlers.onFileChooseOpenMethod = undefined;
		handlers.onFileOpen = undefined;
		handlers.onFolderPolicy = undefined;
		handlers.onGoToLocation = undefined;
		handlers.onManageTags = undefined;
		handlers.onMove = undefined;
		handlers.onRename = undefined;
		handlers.onDelete = undefined;
		handlers.onVersions = undefined;

		const fileProps = resolveFileBrowserItemMenuProps({
			handlers,
			isFolder: false,
			item: { id: 7, name: "note.txt", is_locked: false } as FileListItem,
			selection: emptySelection(),
		});
		const folderProps = resolveFileBrowserItemMenuProps({
			handlers,
			isFolder: true,
			item: { id: 3, name: "Docs", is_locked: false } as FolderListItem,
			selection: emptySelection(),
		});

		expect(fileProps.onChooseOpenMethod).toBeUndefined();
		expect(fileProps.onArchiveExtract).toBeUndefined();
		expect(fileProps.onArchiveCompress).toBeUndefined();
		expect(fileProps.onCopy).toBeUndefined();
		expect(fileProps.onGoToLocation).toBeUndefined();
		expect(fileProps.onManageTags).toBeUndefined();
		expect(fileProps.onMove).toBeUndefined();
		expect(fileProps.onRename).toBeUndefined();
		expect(fileProps.onDelete).toBeUndefined();
		expect(fileProps.onVersions).toBeUndefined();
		expect(folderProps.onArchiveDownload).toBeUndefined();
		expect(folderProps.onArchiveCompress).toBeUndefined();
		expect(folderProps.onCopy).toBeUndefined();
		expect(folderProps.onFolderPolicy).toBeUndefined();
	});

	it("uses batch menu only for selected multi-selection items", () => {
		const handlers = createHandlers();
		const batchDelete = vi.fn();
		const batchCopy = vi.fn();
		const props = resolveFileBrowserItemMenuProps({
			batchSelectionActions: {
				count: 2,
				onCopy: batchCopy,
				onDelete: batchDelete,
				onMove: vi.fn(),
			},
			handlers,
			isFolder: false,
			item: { id: 7, name: "bundle.zip" } as FileListItem,
			selection: {
				selectedFileIds: new Set([7, 8]),
				selectedFolderIds: new Set(),
			},
		});

		props.onCopy?.();
		props.onDelete?.();

		expect(props.selectionCount).toBe(2);
		expect(props.isLocked).toBe(false);
		expect(batchCopy).toHaveBeenCalledTimes(1);
		expect(batchDelete).toHaveBeenCalledTimes(1);
		expect(handlers.onCopy).not.toHaveBeenCalled();
	});

	it("falls back to item actions when selection is disabled", () => {
		const handlers = createHandlers();
		const props = resolveFileBrowserItemMenuProps({
			batchSelectionActions: {
				count: 2,
				onCopy: vi.fn(),
				onDelete: vi.fn(),
				onMove: vi.fn(),
			},
			handlers,
			isFolder: false,
			item: { id: 7, name: "bundle.zip" } as FileListItem,
			selection: {
				selectedFileIds: new Set([7, 8]),
				selectedFolderIds: new Set(),
			},
			selectionEnabled: false,
		});

		expect(props.selectionCount).toBeUndefined();
		props.onOpen?.();
		expect(handlers.onFileOpen).toHaveBeenCalledWith(
			expect.objectContaining({ id: 7 }),
		);
	});
});
