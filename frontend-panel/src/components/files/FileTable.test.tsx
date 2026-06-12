import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FileTable } from "@/components/files/FileTable";
import { DRAG_SOURCE_MIME } from "@/lib/constants";

const mockState = vi.hoisted(() => ({
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
		selectItems: vi.fn(),
		clearSelection: vi.fn(),
		sortBy: "name",
		sortOrder: "asc",
		setSortBy: vi.fn(),
		setSortOrder: vi.fn(),
	},
	getInvalidInternalDropReason: vi.fn(),
	hasInternalDragData: vi.fn(),
	readInternalDragData: vi.fn(),
	setInternalDragPreview: vi.fn(),
	writeInternalDragData: vi.fn(),
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
	FileBrowserItemContextMenu: ({ children }: { children: React.ReactNode }) =>
		children,
}));

vi.mock("@/components/files/FileTableCells", () => ({
	FileNameCell: ({
		file,
		thumbnailPath,
	}: {
		file: { name: string };
		thumbnailPath?: string;
	}) => <td data-thumbnail-path={thumbnailPath ?? ""}>{file.name}</td>,
	FolderNameCell: ({ folder }: { folder: { name: string } }) => (
		<td>{folder.name}</td>
	),
	FileSizeCell: ({ size }: { size: number }) => <td>{size}</td>,
	FolderSizeCell: () => <td>---</td>,
	UpdatedAtCell: ({ updatedAt }: { updatedAt: string }) => <td>{updatedAt}</td>,
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => (
		<span data-testid="icon" data-name={name} />
	),
}));

vi.mock("@/components/ui/item-checkbox", () => ({
	ItemCheckbox: ({
		checked,
		onChange,
	}: {
		checked: boolean;
		onChange: () => void;
	}) => (
		<button
			type="button"
			data-testid="checkbox"
			aria-label={`checkbox-${checked ? "checked" : "unchecked"}`}
			onClick={onChange}
		/>
	),
}));

vi.mock("@/components/ui/table", () => ({
	Table: ({ children }: { children: React.ReactNode }) => (
		<table>{children}</table>
	),
	TableHeader: ({ children }: { children: React.ReactNode }) => (
		<thead>{children}</thead>
	),
	TableBody: ({ children }: { children: React.ReactNode }) => (
		<tbody>{children}</tbody>
	),
	TableRow: ({
		children,
		onClick,
		onDoubleClick,
		onDragStart,
		onDragOver,
		onDragLeave,
		onDrop,
		draggable,
		className,
		...props
	}: {
		children: React.ReactNode;
		onClick?: () => void;
		onDoubleClick?: () => void;
		onDragStart?: (event: React.DragEvent<HTMLTableRowElement>) => void;
		onDragOver?: (event: React.DragEvent<HTMLTableRowElement>) => void;
		onDragLeave?: () => void;
		onDrop?: (event: React.DragEvent<HTMLTableRowElement>) => void;
		draggable?: boolean;
		className?: string;
	}) => (
		<tr
			data-testid="row"
			data-draggable={String(Boolean(draggable))}
			className={className}
			onClick={onClick}
			onDoubleClick={onDoubleClick}
			onDragStart={onDragStart}
			onDragOver={onDragOver}
			onDragLeave={onDragLeave}
			onDrop={onDrop}
			{...props}
		>
			{children}
		</tr>
	),
	TableHead: ({
		children,
		onClick,
		className,
	}: {
		children?: React.ReactNode;
		onClick?: () => void;
		className?: string;
	}) => (
		<th className={className} onClick={onClick}>
			{children}
		</th>
	),
	TableCell: ({
		children,
		onClick,
		className,
	}: {
		children?: React.ReactNode;
		onClick?: (event: React.MouseEvent<HTMLTableCellElement>) => void;
		className?: string;
	}) => (
		<td
			className={className}
			onClick={onClick}
			onKeyDown={(event) => {
				if (!onClick || (event.key !== "Enter" && event.key !== " ")) {
					return;
				}
				onClick(event as unknown as React.MouseEvent<HTMLTableCellElement>);
			}}
		>
			{children}
		</td>
	),
}));

vi.mock("@/lib/dragDrop", () => ({
	getInvalidInternalDropReason: (...args: unknown[]) =>
		mockState.getInvalidInternalDropReason(...args),
	hasInternalDragData: (...args: unknown[]) =>
		mockState.hasInternalDragData(...args),
	readInternalDragData: (...args: unknown[]) =>
		mockState.readInternalDragData(...args),
	setInternalDragPreview: (...args: unknown[]) =>
		mockState.setInternalDragPreview(...args),
	writeInternalDragData: (...args: unknown[]) =>
		mockState.writeInternalDragData(...args),
}));

const folder = {
	id: 1,
	name: "Docs",
	updated_at: "2026-01-01",
	is_locked: false,
};

const file = {
	id: 2,
	name: "report.pdf",
	size: 123,
	updated_at: "2026-01-02",
	is_locked: true,
};

describe("FileTable", () => {
	beforeEach(() => {
		mockState.browserContext.breadcrumbPathIds = [];
		mockState.browserContext.browserOpenMode = "single_click";
		mockState.browserContext.fadingFileIds = undefined;
		mockState.browserContext.fadingFolderIds = undefined;
		mockState.browserContext.files = [file];
		mockState.browserContext.folders = [folder];
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
		mockState.store.selectItems.mockReset();
		mockState.store.clearSelection.mockReset();
		mockState.store.sortBy = "name";
		mockState.store.sortOrder = "asc";
		mockState.store.setSortBy.mockReset();
		mockState.store.setSortOrder.mockReset();
		mockState.getInvalidInternalDropReason.mockReset();
		mockState.hasInternalDragData.mockReset();
		mockState.readInternalDragData.mockReset();
		mockState.setInternalDragPreview.mockReset();
		mockState.writeInternalDragData.mockReset();
		mockState.hasInternalDragData.mockReturnValue(false);
		mockState.readInternalDragData.mockReturnValue(null);
		mockState.getInvalidInternalDropReason.mockReturnValue(null);
	});

	it("renders current sort state and updates sorting from header clicks", () => {
		render(<FileTable />);

		expect(screen.getByText("translated:core:name")).toBeInTheDocument();
		expect(screen.getByText("translated:core:size")).toBeInTheDocument();
		expect(screen.getByText("translated:core:date")).toBeInTheDocument();
		expect(screen.getAllByTestId("icon")[0]).toHaveAttribute(
			"data-name",
			"SortAscending",
		);

		fireEvent.click(screen.getByText("translated:core:name"));
		fireEvent.click(screen.getByText("translated:core:size"));

		expect(mockState.store.setSortOrder).toHaveBeenCalledWith("desc");
		expect(mockState.store.setSortBy).toHaveBeenCalledWith("size");
	});

	it("toggles all selections depending on current state", () => {
		mockState.store.selectedFileIds = new Set([2]);
		mockState.store.selectedFolderIds = new Set([1]);

		const { rerender } = render(<FileTable scrollElement={null} />);

		fireEvent.click(screen.getAllByTestId("checkbox")[0]);
		expect(mockState.store.clearSelection).toHaveBeenCalledTimes(1);

		mockState.store.selectedFileIds = new Set();
		mockState.store.selectedFolderIds = new Set();
		rerender(<FileTable scrollElement={document.createElement("div")} />);

		fireEvent.click(screen.getAllByTestId("checkbox")[0]);
		expect(mockState.store.selectItems).toHaveBeenCalledWith([2], [1]);
	});

	it("wires row clicks, selection toggles, and drag start metadata", () => {
		const dataTransfer = { types: [] } as unknown as DataTransfer;

		render(<FileTable />);

		const rows = screen.getAllByTestId("row");
		expect(rows[1]).toHaveClass("select-none");
		expect(rows[2]).toHaveClass("select-none");
		fireEvent.click(rows[1]);
		fireEvent.click(rows[2]);
		fireEvent.click(screen.getAllByTestId("checkbox")[1]);
		fireEvent.click(screen.getAllByTestId("checkbox")[2]);
		fireEvent.dragStart(rows[1], { dataTransfer });

		expect(mockState.browserContext.onFolderOpen).toHaveBeenCalledWith(
			1,
			"Docs",
		);
		expect(mockState.browserContext.onFileClick).toHaveBeenCalledWith(
			expect.objectContaining({ id: 2 }),
		);
		expect(mockState.store.toggleFolderSelection).toHaveBeenCalledWith(1);
		expect(mockState.store.toggleFileSelection).toHaveBeenCalledWith(2);
		expect(mockState.writeInternalDragData).toHaveBeenCalledWith(dataTransfer, {
			fileIds: [],
			folderIds: [1],
		});
		expect(mockState.setInternalDragPreview).toHaveBeenCalledWith(
			expect.anything(),
			{
				variant: "list-row",
				itemCount: 1,
			},
		);
	});

	it("selects folders and files on single click and opens them on double click in double-click mode", () => {
		mockState.browserContext.browserOpenMode = "double_click";

		render(<FileTable />);

		const rows = screen.getAllByTestId("row");
		fireEvent.click(rows[1]);
		fireEvent.click(rows[2]);
		fireEvent.doubleClick(rows[1]);
		fireEvent.doubleClick(rows[2]);

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

	it("renders read-only rows without selection, sorting, or drag behavior", () => {
		mockState.browserContext.readOnly = true;
		mockState.browserContext.browserOpenMode = "double_click";
		mockState.browserContext.getThumbnailPath = (item) => `/thumb/${item.id}`;
		mockState.store.selectedFileIds = new Set([2]);
		mockState.store.selectedFolderIds = new Set([1]);

		render(<FileTable />);

		expect(screen.queryAllByTestId("checkbox")).toHaveLength(0);
		expect(screen.queryByTestId("icon")).not.toBeInTheDocument();
		expect(screen.getByText("report.pdf")).toHaveAttribute(
			"data-thumbnail-path",
			"/thumb/2",
		);

		fireEvent.click(screen.getByText("translated:core:name"));
		expect(mockState.store.setSortOrder).not.toHaveBeenCalled();
		expect(mockState.store.setSortBy).not.toHaveBeenCalled();

		const rows = screen.getAllByTestId("row");
		expect(rows[1]).toHaveAttribute("data-draggable", "false");
		expect(rows[2]).toHaveAttribute("data-draggable", "false");
		fireEvent.click(rows[1]);
		fireEvent.click(rows[2]);

		expect(mockState.store.selectOnlyFolder).not.toHaveBeenCalled();
		expect(mockState.store.selectOnlyFile).not.toHaveBeenCalled();
		expect(mockState.browserContext.onFolderOpen).toHaveBeenCalledWith(
			1,
			"Docs",
		);
		expect(mockState.browserContext.onFileClick).toHaveBeenCalledWith(
			expect.objectContaining({ id: 2 }),
		);
		expect(
			screen.queryByRole("button", { name: "actions:Docs" }),
		).not.toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "actions:report.pdf" }),
		).toBeInTheDocument();
	});

	it("allows selection in read-only tables when explicitly enabled", () => {
		mockState.browserContext.readOnly = true;
		mockState.browserContext.selectionEnabled = true;
		mockState.browserContext.browserOpenMode = "double_click";
		mockState.store.selectedFileIds = new Set([2]);
		mockState.store.selectedFolderIds = new Set([1]);

		render(<FileTable />);

		expect(screen.queryAllByTestId("checkbox")).toHaveLength(3);
		expect(screen.queryByTestId("icon")).not.toBeInTheDocument();

		const rows = screen.getAllByTestId("row");
		expect(rows[1]).toHaveAttribute("data-state", "selected");
		expect(rows[1]).toHaveAttribute("data-draggable", "false");
		expect(rows[2]).toHaveAttribute("data-state", "selected");
		expect(rows[2]).toHaveAttribute("data-draggable", "false");

		fireEvent.click(screen.getByText("translated:core:name"));
		expect(mockState.store.setSortOrder).not.toHaveBeenCalled();
		expect(mockState.store.setSortBy).not.toHaveBeenCalled();
	});

	it("accepts valid folder drops and ignores invalid ones", () => {
		const dataTransfer = {
			types: ["application/x-asterdrive-move"],
			dropEffect: "copy",
		} as unknown as DataTransfer;
		mockState.browserContext.breadcrumbPathIds = [10];
		mockState.browserContext.files = [];
		mockState.hasInternalDragData.mockReturnValue(true);
		mockState.readInternalDragData.mockReturnValue({
			fileIds: [2],
			folderIds: [3],
		});

		render(<FileTable />);

		const folderRow = screen.getAllByTestId("row")[1];

		fireEvent.dragOver(folderRow, { dataTransfer });
		expect(dataTransfer.dropEffect).toBe("move");
		expect(folderRow).toHaveClass("ring-2", "ring-primary");

		fireEvent.drop(folderRow, { dataTransfer });
		expect(mockState.getInvalidInternalDropReason).toHaveBeenCalledWith(
			{ fileIds: [2], folderIds: [3] },
			1,
			[10, 1],
		);
		expect(mockState.browserContext.onMoveToFolder).toHaveBeenCalledWith(
			[2],
			[3],
			1,
		);

		mockState.getInvalidInternalDropReason.mockReturnValueOnce("descendant");
		fireEvent.drop(folderRow, { dataTransfer });
		expect(mockState.browserContext.onMoveToFolder).toHaveBeenCalledTimes(1);

		const sourceDataTransfer = {
			types: [DRAG_SOURCE_MIME],
		} as unknown as DataTransfer;
		fireEvent.dragOver(folderRow, { dataTransfer: sourceDataTransfer });
		fireEvent.drop(folderRow, { dataTransfer: sourceDataTransfer });
		expect(mockState.browserContext.onMoveToFolder).toHaveBeenCalledTimes(1);
	});
});
