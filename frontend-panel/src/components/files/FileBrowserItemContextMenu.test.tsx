import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	FileBrowserItemActionMenu,
	FileBrowserItemContextMenu,
} from "@/components/files/FileBrowserItemContextMenu";

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
			onManageTags?: () => void;
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
		onFolderPolicy: undefined as
			| ((folder: { id: number; name: string }) => void)
			| undefined,
		onFolderOpen: vi.fn(),
		onGoToLocation: vi.fn(),
		onInfo: vi.fn(),
		onManageTags: vi.fn(),
		onMove: vi.fn(),
		onRename: vi.fn(),
		onShare: vi.fn(),
		onToggleLock: vi.fn(),
		onVersions: vi.fn(),
		readOnly: false,
		selectionEnabled: undefined as boolean | undefined,
	},
	store: {
		selectedFileIds: new Set<number>(),
		selectedFolderIds: new Set<number>(),
	},
}));

vi.mock("@/components/files/FileBrowserContext", () => ({
	useFileBrowserContext: () => mockState.browserContext,
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/stores/fileStore", () => ({
	useFileStore: (selector: (state: typeof mockState.store) => unknown) =>
		selector(mockState.store),
}));

vi.mock("@/components/files/FileContextMenu", () => ({
	FileContextDropdownMenu: ({
		onOpen,
		trigger,
	}: {
		onOpen?: () => void;
		trigger: React.ReactNode;
	}) => (
		<div>
			<div data-testid="dropdown-trigger">{trigger}</div>
			{onOpen && (
				<button type="button" onClick={onOpen}>
					dropdown-open
				</button>
			)}
		</div>
	),
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
		onFolderPolicy,
		onGoToLocation,
		onInfo,
		onManageTags,
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
		onFolderPolicy?: () => void;
		onGoToLocation?: () => void;
		onInfo?: () => void;
		onManageTags?: () => void;
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
			{onFolderPolicy && (
				<button type="button" onClick={onFolderPolicy}>
					policy
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
			{onManageTags && (
				<button type="button" onClick={onManageTags}>
					tags
				</button>
			)}
			{onMove && (
				<button type="button" onClick={onMove}>
					move
				</button>
			)}
			{onGoToLocation && (
				<button type="button" onClick={onGoToLocation}>
					location
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

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		onClick,
		onDoubleClick,
		onKeyDown,
		onPointerDown,
		...props
	}: React.ButtonHTMLAttributes<HTMLButtonElement>) => (
		<button
			type="button"
			onClick={onClick}
			onDoubleClick={onDoubleClick}
			onKeyDown={onKeyDown}
			onPointerDown={onPointerDown}
			{...props}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span aria-hidden="true">{name}</span>,
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
		mockState.browserContext.onFolderPolicy = undefined;
		mockState.browserContext.onFolderOpen.mockReset();
		mockState.browserContext.onGoToLocation.mockReset();
		mockState.browserContext.onInfo.mockReset();
		mockState.browserContext.onManageTags.mockReset();
		mockState.browserContext.onMove.mockReset();
		mockState.browserContext.onRename.mockReset();
		mockState.browserContext.onShare.mockReset();
		mockState.browserContext.onToggleLock.mockReset();
		mockState.browserContext.onVersions.mockReset();
		mockState.browserContext.readOnly = false;
		mockState.browserContext.selectionEnabled = undefined;
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
		fireEvent.click(screen.getByRole("button", { name: "tags" }));
		fireEvent.click(screen.getByRole("button", { name: "move" }));
		expect(screen.queryByRole("button", { name: "policy" })).toBeNull();
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
		expect(mockState.browserContext.onManageTags).toHaveBeenCalledWith(
			"folder",
			1,
		);
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

	it("maps the optional folder policy action only for folders", () => {
		const onFolderPolicy = vi.fn();
		mockState.browserContext.onFolderPolicy = onFolderPolicy;

		render(
			<FileBrowserItemContextMenu
				item={{ id: 1, name: "Docs", is_locked: false } as never}
				isFolder
			>
				<div>folder</div>
			</FileBrowserItemContextMenu>,
		);

		fireEvent.click(screen.getByRole("button", { name: "policy" }));

		expect(onFolderPolicy).toHaveBeenCalledWith(
			expect.objectContaining({ id: 1, name: "Docs" }),
		);
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
		fireEvent.click(screen.getByRole("button", { name: "tags" }));
		fireEvent.click(screen.getByRole("button", { name: "move" }));
		fireEvent.click(screen.getByRole("button", { name: "location" }));
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
		expect(mockState.browserContext.onManageTags).toHaveBeenCalledWith(
			"file",
			2,
		);
		expect(mockState.browserContext.onMove).toHaveBeenCalledWith("file", 2);
		expect(mockState.browserContext.onGoToLocation).toHaveBeenCalledWith(
			expect.objectContaining({ id: 2 }),
		);
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

	it("does not expose archive extraction for 7z files", () => {
		render(
			<FileBrowserItemContextMenu
				item={{ id: 3, name: "bundle.7z", is_locked: false } as never}
				isFolder={false}
			>
				<div>file</div>
			</FileBrowserItemContextMenu>,
		);

		expect(screen.queryByRole("button", { name: "extract" })).toBeNull();
	});

	it("does not expose archive extraction for unsupported file names", () => {
		render(
			<FileBrowserItemContextMenu
				item={{ id: 4, name: "notes.txt", is_locked: false } as never}
				isFolder={false}
			>
				<div>file</div>
			</FileBrowserItemContextMenu>,
		);

		expect(
			screen.queryByRole("button", { name: "extract" }),
		).not.toBeInTheDocument();
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
			onManageTags: vi.fn(),
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
		fireEvent.click(screen.getByRole("button", { name: "tags" }));
		fireEvent.click(screen.getByRole("button", { name: "move" }));
		fireEvent.click(screen.getByRole("button", { name: "delete" }));

		expect(batchActions.downloadAction.onClick).toHaveBeenCalledTimes(1);
		expect(batchActions.onArchiveCompress).toHaveBeenCalledTimes(1);
		expect(batchActions.onCopy).toHaveBeenCalledTimes(1);
		expect(batchActions.onManageTags).toHaveBeenCalledTimes(1);
		expect(batchActions.onMove).toHaveBeenCalledTimes(1);
		expect(batchActions.onDelete).toHaveBeenCalledTimes(1);
		expect(mockState.browserContext.onCopy).not.toHaveBeenCalled();
	});

	it("does not use batch actions when the current view disables selection", () => {
		const batchActions = {
			count: 3,
			onCopy: vi.fn(),
			onDelete: vi.fn(),
			onMove: vi.fn(),
		};
		mockState.browserContext.batchSelectionActions = batchActions;
		mockState.browserContext.selectionEnabled = false;
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

		expect(screen.queryByText("selection:3")).not.toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "open" }));

		expect(mockState.browserContext.onFileOpen).toHaveBeenCalledWith(
			expect.objectContaining({ id: 2 }),
		);
		expect(batchActions.onCopy).not.toHaveBeenCalled();
	});

	it("limits read-only folder menus to opening the folder", () => {
		mockState.browserContext.readOnly = true;

		render(
			<FileBrowserItemContextMenu
				item={{ id: 1, name: "Docs", is_locked: false } as never}
				isFolder
			>
				<div>folder</div>
			</FileBrowserItemContextMenu>,
		);

		fireEvent.click(screen.getByRole("button", { name: "open" }));

		expect(mockState.browserContext.onFolderOpen).toHaveBeenCalledWith(
			1,
			"Docs",
		);
		expect(screen.queryByRole("button", { name: "archive" })).toBeNull();
		expect(screen.queryByRole("button", { name: "share-page" })).toBeNull();
		expect(screen.queryByRole("button", { name: "copy" })).toBeNull();
		expect(screen.queryByRole("button", { name: "move" })).toBeNull();
		expect(screen.queryByRole("button", { name: "delete" })).toBeNull();
	});

	it("limits read-only file menus to opening and downloading the file", () => {
		mockState.browserContext.readOnly = true;

		render(
			<FileBrowserItemContextMenu
				item={{ id: 2, name: "bundle.zip", is_locked: true } as never}
				isFolder={false}
			>
				<div>file</div>
			</FileBrowserItemContextMenu>,
		);

		fireEvent.click(screen.getByRole("button", { name: "open" }));
		fireEvent.click(screen.getByRole("button", { name: "download" }));

		expect(mockState.browserContext.onFileOpen).toHaveBeenCalledWith(
			expect.objectContaining({ id: 2 }),
		);
		expect(mockState.browserContext.onDownload).toHaveBeenCalledWith(
			2,
			"bundle.zip",
		);
		expect(
			screen.queryByRole("button", { name: "open-method" }),
		).not.toBeInTheDocument();
		expect(screen.queryByRole("button", { name: "extract" })).toBeNull();
		expect(screen.queryByRole("button", { name: "compress" })).toBeNull();
		expect(screen.queryByRole("button", { name: "share-page" })).toBeNull();
		expect(screen.queryByRole("button", { name: "share-direct" })).toBeNull();
		expect(screen.queryByRole("button", { name: "delete" })).toBeNull();
		expect(screen.queryByRole("button", { name: "versions" })).toBeNull();
	});

	it("falls back to regular item actions when the current item is not selected", () => {
		mockState.browserContext.batchSelectionActions = {
			count: 2,
			onCopy: vi.fn(),
			onDelete: vi.fn(),
			onMove: vi.fn(),
		};
		mockState.store.selectedFileIds = new Set([3]);
		mockState.store.selectedFolderIds = new Set([1]);

		render(
			<FileBrowserItemContextMenu
				item={{ id: 2, name: "bundle.zip", is_locked: false } as never}
				isFolder={false}
			>
				<div>file</div>
			</FileBrowserItemContextMenu>,
		);

		expect(screen.queryByText("selection:2")).not.toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "open" }));

		expect(mockState.browserContext.onFileOpen).toHaveBeenCalledWith(
			expect.objectContaining({ id: 2 }),
		);
	});

	it("renders an action menu trigger that stops item row events", () => {
		render(
			<FileBrowserItemActionMenu
				item={{ id: 2, name: "bundle.zip", is_locked: false } as never}
				isFolder={false}
			/>,
		);

		const trigger = screen.getByRole("button", { name: "more_actions" });
		const pointerEvent = new Event("pointerdown", {
			bubbles: true,
			cancelable: true,
		});
		const clickEvent = new MouseEvent("click", {
			bubbles: true,
			cancelable: true,
		});
		const doubleClickEvent = new MouseEvent("dblclick", {
			bubbles: true,
			cancelable: true,
		});
		const keyEvent = new KeyboardEvent("keydown", {
			bubbles: true,
			cancelable: true,
			key: "Enter",
		});
		const pointerStop = vi.spyOn(pointerEvent, "stopPropagation");
		const clickStop = vi.spyOn(clickEvent, "stopPropagation");
		const doubleClickStop = vi.spyOn(doubleClickEvent, "stopPropagation");
		const keyStop = vi.spyOn(keyEvent, "stopPropagation");

		trigger.dispatchEvent(pointerEvent);
		trigger.dispatchEvent(clickEvent);
		trigger.dispatchEvent(doubleClickEvent);
		trigger.dispatchEvent(keyEvent);

		expect(pointerStop).toHaveBeenCalledTimes(1);
		expect(clickStop).toHaveBeenCalledTimes(1);
		expect(doubleClickStop).toHaveBeenCalledTimes(1);
		expect(keyStop).toHaveBeenCalledTimes(1);
	});
});
