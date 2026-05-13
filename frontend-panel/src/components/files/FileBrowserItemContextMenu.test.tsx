import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FileBrowserItemContextMenu } from "@/components/files/FileBrowserItemContextMenu";

const mockState = vi.hoisted(() => ({
	browserContext: {
		batchSelectionActions: null as {
			count: number;
			downloadAction?: {
				kind: "file" | "archive";
				onClick: () => void;
			};
			onArchiveCompress?: () => void;
			onCopy: () => void;
			onDelete: () => void;
			onMove: () => void;
		} | null,
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
		onInfo: vi.fn(),
		onMove: vi.fn(),
		onRename: vi.fn(),
		onShare: vi.fn(),
		onToggleLock: vi.fn(),
		onVersions: vi.fn(),
	},
	store: {
		selectedFileIds: new Set<number>(),
		selectedFolderIds: new Set<number>(),
	},
}));

vi.mock("@/components/files/FileBrowserContext", () => ({
	useFileBrowserContext: () => mockState.browserContext,
}));

vi.mock("@/stores/fileStore", () => ({
	useFileStore: (selector: (state: typeof mockState.store) => unknown) =>
		selector(mockState.store),
}));

vi.mock("@/components/files/FileContextMenu", () => ({
	FileContextMenu: ({
		children,
		downloadAction,
		onArchiveCompress,
		onArchiveDownload,
		onArchiveExtract,
		onChooseOpenMethod,
		onCopy,
		onDelete,
		onDirectShare,
		onDownload,
		onInfo,
		onMove,
		onOpen,
		onPageShare,
		onRename,
		onToggleLock,
		onVersions,
		selectionCount,
	}: {
		children: React.ReactNode;
		downloadAction?: {
			kind: "file" | "archive";
			onClick: () => void;
		};
		onArchiveCompress?: () => void;
		onArchiveDownload?: () => void;
		onArchiveExtract?: () => void;
		onChooseOpenMethod?: () => void;
		onCopy?: () => void;
		onDelete?: () => void;
		onDirectShare?: () => void;
		onDownload?: () => void;
		onInfo?: () => void;
		onMove?: () => void;
		onOpen?: () => void;
		onPageShare?: () => void;
		onRename?: () => void;
		onToggleLock?: () => void;
		onVersions?: () => void;
		selectionCount?: number;
	}) => (
		<div>
			{children}
			{selectionCount != null && <div>{`selection:${selectionCount}`}</div>}
			{onOpen && (
				<button type="button" onClick={onOpen}>
					open
				</button>
			)}
			{onChooseOpenMethod && (
				<button type="button" onClick={onChooseOpenMethod}>
					open-method
				</button>
			)}
			{onDownload && (
				<button type="button" onClick={onDownload}>
					download
				</button>
			)}
			{onArchiveExtract && (
				<button type="button" onClick={onArchiveExtract}>
					extract
				</button>
			)}
			{onArchiveCompress && (
				<button type="button" onClick={onArchiveCompress}>
					compress
				</button>
			)}
			{downloadAction && (
				<button type="button" onClick={downloadAction.onClick}>
					{`download:${downloadAction.kind}`}
				</button>
			)}
			{onArchiveDownload && (
				<button type="button" onClick={onArchiveDownload}>
					archive
				</button>
			)}
			{onPageShare && (
				<button type="button" onClick={onPageShare}>
					share-page
				</button>
			)}
			{onDirectShare && (
				<button type="button" onClick={onDirectShare}>
					share-direct
				</button>
			)}
			{onCopy && (
				<button type="button" onClick={onCopy}>
					copy
				</button>
			)}
			{onMove && (
				<button type="button" onClick={onMove}>
					move
				</button>
			)}
			{onRename && (
				<button type="button" onClick={onRename}>
					rename
				</button>
			)}
			{onToggleLock && (
				<button type="button" onClick={onToggleLock}>
					lock
				</button>
			)}
			{onDelete && (
				<button type="button" onClick={onDelete}>
					delete
				</button>
			)}
			{onVersions && (
				<button type="button" onClick={onVersions}>
					versions
				</button>
			)}
			{onInfo && (
				<button type="button" onClick={onInfo}>
					info
				</button>
			)}
		</div>
	),
}));

describe("FileBrowserItemContextMenu", () => {
	beforeEach(() => {
		mockState.browserContext.batchSelectionActions = null;
		mockState.browserContext.onArchiveCompress.mockReset();
		mockState.browserContext.onArchiveDownload.mockReset();
		mockState.browserContext.onArchiveExtract.mockReset();
		mockState.browserContext.onCopy.mockReset();
		mockState.browserContext.onDelete.mockReset();
		mockState.browserContext.onDownload.mockReset();
		mockState.browserContext.onFileChooseOpenMethod.mockReset();
		mockState.browserContext.onFileClick.mockReset();
		mockState.browserContext.onFileOpen.mockReset();
		mockState.browserContext.onFolderOpen.mockReset();
		mockState.browserContext.onInfo.mockReset();
		mockState.browserContext.onMove.mockReset();
		mockState.browserContext.onRename.mockReset();
		mockState.browserContext.onShare.mockReset();
		mockState.browserContext.onToggleLock.mockReset();
		mockState.browserContext.onVersions.mockReset();
		mockState.store.selectedFileIds = new Set();
		mockState.store.selectedFolderIds = new Set();
	});

	it("maps folder actions to the shared browser callbacks", () => {
		render(
			<FileBrowserItemContextMenu
				item={{ id: 1, name: "Docs", is_locked: false } as never}
				isFolder
			>
				<div>folder</div>
			</FileBrowserItemContextMenu>,
		);

		fireEvent.click(screen.getByRole("button", { name: "open" }));
		fireEvent.click(screen.getByRole("button", { name: "compress" }));
		fireEvent.click(screen.getByRole("button", { name: "archive" }));
		fireEvent.click(screen.getByRole("button", { name: "share-page" }));
		fireEvent.click(screen.getByRole("button", { name: "copy" }));
		fireEvent.click(screen.getByRole("button", { name: "move" }));
		fireEvent.click(screen.getByRole("button", { name: "rename" }));
		fireEvent.click(screen.getByRole("button", { name: "lock" }));
		fireEvent.click(screen.getByRole("button", { name: "delete" }));
		fireEvent.click(screen.getByRole("button", { name: "info" }));

		expect(mockState.browserContext.onFolderOpen).toHaveBeenCalledWith(
			1,
			"Docs",
		);
		expect(mockState.browserContext.onArchiveCompress).toHaveBeenCalledWith(
			"folder",
			1,
		);
		expect(mockState.browserContext.onArchiveDownload).toHaveBeenCalledWith(1);
		expect(mockState.browserContext.onShare).toHaveBeenCalledWith({
			folderId: 1,
			name: "Docs",
			initialMode: "page",
		});
		expect(mockState.browserContext.onCopy).toHaveBeenCalledWith("folder", 1);
		expect(mockState.browserContext.onMove).toHaveBeenCalledWith("folder", 1);
		expect(mockState.browserContext.onRename).toHaveBeenCalledWith(
			"folder",
			1,
			"Docs",
		);
		expect(mockState.browserContext.onToggleLock).toHaveBeenCalledWith(
			"folder",
			1,
			false,
		);
		expect(mockState.browserContext.onDelete).toHaveBeenCalledWith("folder", 1);
		expect(mockState.browserContext.onInfo).toHaveBeenCalledWith("folder", 1);
	});

	it("maps file actions to the shared browser callbacks", () => {
		render(
			<FileBrowserItemContextMenu
				item={{ id: 2, name: "bundle.zip", is_locked: true } as never}
				isFolder={false}
			>
				<div>file</div>
			</FileBrowserItemContextMenu>,
		);

		fireEvent.click(screen.getByRole("button", { name: "open" }));
		fireEvent.click(screen.getByRole("button", { name: "open-method" }));
		fireEvent.click(screen.getByRole("button", { name: "download" }));
		fireEvent.click(screen.getByRole("button", { name: "extract" }));
		fireEvent.click(screen.getByRole("button", { name: "compress" }));
		fireEvent.click(screen.getByRole("button", { name: "share-page" }));
		fireEvent.click(screen.getByRole("button", { name: "share-direct" }));
		fireEvent.click(screen.getByRole("button", { name: "copy" }));
		fireEvent.click(screen.getByRole("button", { name: "move" }));
		fireEvent.click(screen.getByRole("button", { name: "rename" }));
		fireEvent.click(screen.getByRole("button", { name: "lock" }));
		fireEvent.click(screen.getByRole("button", { name: "delete" }));
		fireEvent.click(screen.getByRole("button", { name: "versions" }));
		fireEvent.click(screen.getByRole("button", { name: "info" }));

		expect(mockState.browserContext.onFileOpen).toHaveBeenCalledWith(
			expect.objectContaining({ id: 2 }),
		);
		expect(
			mockState.browserContext.onFileChooseOpenMethod,
		).toHaveBeenCalledWith(expect.objectContaining({ id: 2 }));
		expect(mockState.browserContext.onDownload).toHaveBeenCalledWith(
			2,
			"bundle.zip",
		);
		expect(mockState.browserContext.onArchiveExtract).toHaveBeenCalledWith(2);
		expect(mockState.browserContext.onArchiveCompress).toHaveBeenCalledWith(
			"file",
			2,
		);
		expect(mockState.browserContext.onShare).toHaveBeenNthCalledWith(1, {
			fileId: 2,
			name: "bundle.zip",
			initialMode: "page",
		});
		expect(mockState.browserContext.onShare).toHaveBeenNthCalledWith(2, {
			fileId: 2,
			name: "bundle.zip",
			initialMode: "direct",
		});
		expect(mockState.browserContext.onCopy).toHaveBeenCalledWith("file", 2);
		expect(mockState.browserContext.onMove).toHaveBeenCalledWith("file", 2);
		expect(mockState.browserContext.onRename).toHaveBeenCalledWith(
			"file",
			2,
			"bundle.zip",
		);
		expect(mockState.browserContext.onToggleLock).toHaveBeenCalledWith(
			"file",
			2,
			true,
		);
		expect(mockState.browserContext.onDelete).toHaveBeenCalledWith("file", 2);
		expect(mockState.browserContext.onVersions).toHaveBeenCalledWith(2);
		expect(mockState.browserContext.onInfo).toHaveBeenCalledWith("file", 2);
	});

	it("uses batch actions when the current item belongs to a multi-selection", () => {
		const batchActions = {
			count: 3,
			downloadAction: {
				kind: "archive" as const,
				onClick: vi.fn(),
			},
			onArchiveCompress: vi.fn(),
			onCopy: vi.fn(),
			onDelete: vi.fn(),
			onMove: vi.fn(),
		};
		mockState.browserContext.batchSelectionActions = batchActions;
		mockState.store.selectedFileIds = new Set([2, 3]);
		mockState.store.selectedFolderIds = new Set([1]);

		render(
			<FileBrowserItemContextMenu
				item={{ id: 2, name: "bundle.zip", is_locked: false } as never}
				isFolder={false}
			>
				<div>file</div>
			</FileBrowserItemContextMenu>,
		);

		expect(screen.getByText("selection:3")).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "open" }),
		).not.toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "download:archive" }));
		fireEvent.click(screen.getByRole("button", { name: "compress" }));
		fireEvent.click(screen.getByRole("button", { name: "copy" }));
		fireEvent.click(screen.getByRole("button", { name: "move" }));
		fireEvent.click(screen.getByRole("button", { name: "delete" }));

		expect(batchActions.downloadAction.onClick).toHaveBeenCalledTimes(1);
		expect(batchActions.onArchiveCompress).toHaveBeenCalledTimes(1);
		expect(batchActions.onCopy).toHaveBeenCalledTimes(1);
		expect(batchActions.onMove).toHaveBeenCalledTimes(1);
		expect(batchActions.onDelete).toHaveBeenCalledTimes(1);
		expect(mockState.browserContext.onCopy).not.toHaveBeenCalled();
	});
});
