import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FileGrid } from "@/components/files/FileGrid";

const mockState = vi.hoisted(() => ({
	contextMenuItems: [] as string[],
	browserContext: {
		breadcrumbPathIds: [] as number[],
		browserOpenMode: "single_click" as "single_click" | "double_click",
		fadingFileIds: undefined as Set<number> | undefined,
		fadingFolderIds: undefined as Set<number> | undefined,
		files: [] as Array<Record<string, unknown>>,
		folders: [] as Array<Record<string, unknown>>,
		getThumbnailPath: undefined as
			| ((file: { id: number; name: string }) => string)
			| undefined,
		onFileClick: vi.fn(),
		onFolderOpen: vi.fn(),
		onMoveToFolder: vi.fn(),
		readOnly: false,
		selectionEnabled: undefined as boolean | undefined,
	},
	store: {
		selectedFileIds: new Set<number>(),
		selectedFolderIds: new Set<number>(),
		selectOnlyFile: vi.fn(),
		selectOnlyFolder: vi.fn(),
		toggleFileSelection: vi.fn(),
		toggleFolderSelection: vi.fn(),
	},
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => `translated:${key}`,
	}),
}));

vi.mock("@/components/files/FileBrowserContext", () => ({
	useFileBrowserContext: () => mockState.browserContext,
}));

vi.mock("@/stores/fileStore", () => ({
	useFileStore: Object.assign(
		(selector: (state: typeof mockState.store) => unknown) =>
			selector(mockState.store),
		{
			getState: () => mockState.store,
		},
	),
}));

vi.mock("@/components/files/FileBrowserItemContextMenu", () => ({
	FileBrowserItemActionMenu: ({
		item,
	}: {
		item: { name: string };
		isFolder: boolean;
	}) => <button type="button">actions:{item.name}</button>,
	FileBrowserItemContextMenu: ({
		children,
		item,
	}: {
		children: React.ReactNode;
		item: { name: string };
	}) => {
		mockState.contextMenuItems.push(item.name);
		return <div>{children}</div>;
	},
}));

vi.mock("@/components/files/FileCard", () => ({
	FileCard: ({
		item,
		isFolder,
		selected,
		onSelect,
		onClick,
		onDoubleClick,
		dragData,
		draggable,
		resolveDragData,
		selectable,
		targetPathIds,
		fading,
		thumbnailPath,
		actionMenu,
		alwaysShowActionMenu,
	}: {
		item: { name: string };
		isFolder: boolean;
		selected: boolean;
		onSelect?: () => void;
		onClick: () => void;
		onDoubleClick?: () => void;
		dragData?: { fileIds: number[]; folderIds: number[] };
		draggable?: boolean;
		resolveDragData?: () => { fileIds: number[]; folderIds: number[] };
		selectable?: boolean;
		targetPathIds?: number[];
		fading?: boolean;
		thumbnailPath?: string;
		actionMenu?: React.ReactNode;
		alwaysShowActionMenu?: boolean;
	}) => {
		const computedDragData = resolveDragData?.() ?? dragData;
		return (
			<div
				data-testid={isFolder ? "folder-card" : "file-card"}
				data-selected={String(selected)}
				data-drag-file-ids={computedDragData?.fileIds.join(",") ?? ""}
				data-drag-folder-ids={computedDragData?.folderIds.join(",") ?? ""}
				data-target-path-ids={targetPathIds?.join(",") ?? ""}
				data-fading={String(Boolean(fading))}
				data-draggable={String(draggable ?? true)}
				data-selectable={String(selectable ?? true)}
				data-thumbnail-path={thumbnailPath ?? ""}
				data-always-show-action-menu={String(Boolean(alwaysShowActionMenu))}
			>
				<button type="button" onClick={onClick}>
					open:{item.name}
				</button>
				<button type="button" onClick={onDoubleClick}>
					open-double:{item.name}
				</button>
				{onSelect ? (
					<button type="button" onClick={onSelect}>
						select:{item.name}
					</button>
				) : null}
				{actionMenu}
			</div>
		);
	},
}));

describe("FileGrid", () => {
	beforeEach(() => {
		mockState.contextMenuItems = [];
		mockState.browserContext.breadcrumbPathIds = [];
		mockState.browserContext.browserOpenMode = "single_click";
		mockState.browserContext.fadingFileIds = undefined;
		mockState.browserContext.fadingFolderIds = undefined;
		mockState.browserContext.files = [];
		mockState.browserContext.folders = [];
		mockState.browserContext.getThumbnailPath = undefined;
		mockState.browserContext.onFileClick.mockReset();
		mockState.browserContext.onFolderOpen.mockReset();
		mockState.browserContext.onMoveToFolder.mockReset();
		mockState.browserContext.readOnly = false;
		mockState.browserContext.selectionEnabled = undefined;
		mockState.store.selectedFileIds = new Set();
		mockState.store.selectedFolderIds = new Set();
		mockState.store.selectOnlyFile.mockReset();
		mockState.store.selectOnlyFolder.mockReset();
		mockState.store.toggleFileSelection.mockReset();
		mockState.store.toggleFolderSelection.mockReset();
	});

	it("renders section headers and computed drag metadata", () => {
		mockState.browserContext.breadcrumbPathIds = [10, 11];
		mockState.browserContext.fadingFileIds = new Set([2]);
		mockState.browserContext.fadingFolderIds = new Set([1]);
		mockState.browserContext.files = [{ id: 2, name: "report.pdf" }];
		mockState.browserContext.folders = [{ id: 1, name: "Docs" }];
		mockState.store.selectedFileIds = new Set([2, 3]);
		mockState.store.selectedFolderIds = new Set([1]);

		render(<FileGrid />);

		expect(screen.getByText("translated:folders_section")).toBeInTheDocument();
		expect(screen.getByText("translated:files_section")).toBeInTheDocument();
		expect(screen.getByTestId("folder-card")).toHaveAttribute(
			"data-selected",
			"true",
		);
		expect(screen.getByTestId("folder-card")).toHaveAttribute(
			"data-drag-folder-ids",
			"1",
		);
		expect(screen.getByTestId("folder-card")).toHaveAttribute(
			"data-target-path-ids",
			"10,11,1",
		);
		expect(screen.getByTestId("folder-card")).toHaveAttribute(
			"data-fading",
			"true",
		);
		expect(screen.getByTestId("file-card")).toHaveAttribute(
			"data-drag-file-ids",
			"2,3",
		);
		expect(screen.getByTestId("file-card")).toHaveAttribute(
			"data-drag-folder-ids",
			"1",
		);
		expect(screen.getByTestId("file-card")).toHaveAttribute(
			"data-fading",
			"true",
		);
	});

	it("wires folder and file click and selection handlers", () => {
		mockState.browserContext.files = [{ id: 2, name: "report.pdf" }];
		mockState.browserContext.folders = [{ id: 1, name: "Docs" }];

		render(<FileGrid />);

		fireEvent.click(screen.getByRole("button", { name: "open:Docs" }));
		fireEvent.click(screen.getByRole("button", { name: "select:Docs" }));
		fireEvent.click(screen.getByRole("button", { name: "open:report.pdf" }));
		fireEvent.click(screen.getByRole("button", { name: "select:report.pdf" }));

		expect(mockState.browserContext.onFolderOpen).toHaveBeenCalledWith(
			1,
			"Docs",
		);
		expect(mockState.store.toggleFolderSelection).toHaveBeenCalledWith(1);
		expect(mockState.browserContext.onFileClick).toHaveBeenCalledWith(
			expect.objectContaining({ id: 2 }),
		);
		expect(mockState.store.toggleFileSelection).toHaveBeenCalledWith(2);
	});

	it("renders read-only cards without selection or drag behavior", () => {
		mockState.browserContext.readOnly = true;
		mockState.browserContext.getThumbnailPath = (file) => `/thumb/${file.id}`;
		mockState.browserContext.files = [{ id: 2, name: "report.pdf" }];
		mockState.browserContext.folders = [{ id: 1, name: "Docs" }];
		mockState.store.selectedFileIds = new Set([2]);
		mockState.store.selectedFolderIds = new Set([1]);

		render(<FileGrid />);

		expect(screen.getByTestId("folder-card")).toHaveAttribute(
			"data-selected",
			"false",
		);
		expect(screen.getByTestId("folder-card")).toHaveAttribute(
			"data-draggable",
			"false",
		);
		expect(screen.getByTestId("folder-card")).toHaveAttribute(
			"data-selectable",
			"false",
		);
		expect(screen.getByTestId("file-card")).toHaveAttribute(
			"data-selected",
			"false",
		);
		expect(screen.getByTestId("file-card")).toHaveAttribute(
			"data-draggable",
			"false",
		);
		expect(screen.getByTestId("file-card")).toHaveAttribute(
			"data-selectable",
			"false",
		);
		expect(screen.getByTestId("file-card")).toHaveAttribute(
			"data-thumbnail-path",
			"/thumb/2",
		);
		expect(screen.getByTestId("file-card")).toHaveAttribute(
			"data-always-show-action-menu",
			"true",
		);
		expect(
			screen.queryByRole("button", { name: "actions:Docs" }),
		).not.toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "actions:report.pdf" }),
		).toBeInTheDocument();
		expect(mockState.contextMenuItems).toEqual([]);

		fireEvent.click(screen.getByRole("button", { name: "open:Docs" }));
		fireEvent.click(screen.getByRole("button", { name: "open:report.pdf" }));

		expect(mockState.store.selectOnlyFolder).not.toHaveBeenCalled();
		expect(mockState.store.selectOnlyFile).not.toHaveBeenCalled();
		expect(mockState.browserContext.onFolderOpen).toHaveBeenCalledWith(
			1,
			"Docs",
		);
		expect(mockState.browserContext.onFileClick).toHaveBeenCalledWith(
			expect.objectContaining({ id: 2 }),
		);
	});

	it("does not run selection handlers when selection is disabled", () => {
		mockState.browserContext.browserOpenMode = "double_click";
		mockState.browserContext.selectionEnabled = false;
		mockState.browserContext.files = [{ id: 2, name: "report.pdf" }];
		mockState.browserContext.folders = [{ id: 1, name: "Docs" }];

		render(<FileGrid />);

		expect(screen.queryByRole("button", { name: "select:Docs" })).toBeNull();
		expect(
			screen.queryByRole("button", { name: "select:report.pdf" }),
		).toBeNull();

		fireEvent.click(screen.getByRole("button", { name: "open:Docs" }));
		fireEvent.click(screen.getByRole("button", { name: "open:report.pdf" }));

		expect(mockState.store.selectOnlyFolder).not.toHaveBeenCalled();
		expect(mockState.store.selectOnlyFile).not.toHaveBeenCalled();
		expect(mockState.browserContext.onFolderOpen).toHaveBeenCalledWith(
			1,
			"Docs",
		);
		expect(mockState.browserContext.onFileClick).toHaveBeenCalledWith(
			expect.objectContaining({ id: 2 }),
		);
	});

	it("allows selection in read-only grids when explicitly enabled", () => {
		mockState.browserContext.readOnly = true;
		mockState.browserContext.selectionEnabled = true;
		mockState.browserContext.files = [{ id: 2, name: "report.pdf" }];
		mockState.browserContext.folders = [{ id: 1, name: "Docs" }];
		mockState.store.selectedFileIds = new Set([2]);
		mockState.store.selectedFolderIds = new Set([1]);

		render(<FileGrid />);

		expect(screen.getByTestId("folder-card")).toHaveAttribute(
			"data-selected",
			"true",
		);
		expect(screen.getByTestId("folder-card")).toHaveAttribute(
			"data-selectable",
			"true",
		);
		expect(screen.getByTestId("folder-card")).toHaveAttribute(
			"data-draggable",
			"false",
		);
		expect(screen.getByTestId("file-card")).toHaveAttribute(
			"data-selected",
			"true",
		);
		expect(screen.getByTestId("file-card")).toHaveAttribute(
			"data-selectable",
			"true",
		);
		expect(screen.getByTestId("file-card")).toHaveAttribute(
			"data-draggable",
			"false",
		);
		expect(screen.getByTestId("file-card")).toHaveAttribute(
			"data-always-show-action-menu",
			"true",
		);
		expect(mockState.contextMenuItems).toEqual(["Docs", "report.pdf"]);
	});

	it("selects folders and files on single click and opens them on double click in double-click mode", () => {
		mockState.browserContext.browserOpenMode = "double_click";
		mockState.browserContext.files = [{ id: 2, name: "report.pdf" }];
		mockState.browserContext.folders = [{ id: 1, name: "Docs" }];

		render(<FileGrid />);

		fireEvent.click(screen.getByRole("button", { name: "open:Docs" }));
		fireEvent.click(screen.getByRole("button", { name: "open:report.pdf" }));
		fireEvent.click(screen.getByRole("button", { name: "open-double:Docs" }));
		fireEvent.click(
			screen.getByRole("button", { name: "open-double:report.pdf" }),
		);

		expect(mockState.store.selectOnlyFolder).toHaveBeenCalledWith(1);
		expect(mockState.store.selectOnlyFile).toHaveBeenCalledWith(2);
		expect(mockState.browserContext.onFolderOpen).toHaveBeenCalledWith(
			1,
			"Docs",
		);
		expect(mockState.browserContext.onFileClick).toHaveBeenCalledWith(
			expect.objectContaining({ id: 2 }),
		);
	});
});
