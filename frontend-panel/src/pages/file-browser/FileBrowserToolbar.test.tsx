import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FileBrowserToolbar } from "@/pages/file-browser/FileBrowserToolbar";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) =>
			key === "core:selected_count" ? `selected:${options?.count}` : key,
	}),
}));

vi.mock("@/components/common/SortMenu", () => ({
	SortMenu: (props: {
		onSortBy: (value: "updated_at") => void;
		onSortOrder: (value: "desc") => void;
	}) => (
		<div>
			<button type="button" onClick={() => props.onSortBy("updated_at")}>
				sort-by-updated
			</button>
			<button type="button" onClick={() => props.onSortOrder("desc")}>
				sort-order-desc
			</button>
		</div>
	),
}));

vi.mock("@/components/common/ToolbarBar", () => ({
	ToolbarBar: (props: { left?: React.ReactNode; right?: React.ReactNode }) => (
		<div>
			<div>{props.left}</div>
			<div>{props.right}</div>
		</div>
	),
}));

vi.mock("@/components/common/ViewToggle", () => ({
	ViewToggle: (props: { onChange: (value: "grid" | "list") => void }) => (
		<button type="button" onClick={() => props.onChange("list")}>
			view-list
		</button>
	),
}));

vi.mock("@/components/ui/breadcrumb", () => ({
	Breadcrumb: (props: { children: React.ReactNode }) => (
		<div>{props.children}</div>
	),
	BreadcrumbEllipsis: () => <span>ellipsis</span>,
	BreadcrumbItem: (props: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={props.className}>{props.children}</div>,
	BreadcrumbLink: (props: {
		children: React.ReactNode;
		className?: string;
		onClick?: () => void;
		onDragLeave?: (event: unknown) => void;
		onDragOver?: (event: unknown) => void;
		onDrop?: (event: unknown) => void;
	}) => (
		<button
			type="button"
			className={props.className}
			onClick={props.onClick}
			onDragLeave={props.onDragLeave as never}
			onDragOver={props.onDragOver as never}
			onDrop={props.onDrop as never}
		>
			{props.children}
		</button>
	),
	BreadcrumbList: (props: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={props.className}>{props.children}</div>,
	BreadcrumbPage: (props: {
		children: React.ReactNode;
		className?: string;
	}) => <span className={props.className}>{props.children}</span>,
	BreadcrumbSeparator: (props: { className?: string }) => (
		<span className={props.className}>/</span>
	),
}));

vi.mock("@/components/ui/dropdown-menu", () => ({
	DropdownMenu: (props: { children: React.ReactNode }) => (
		<div>{props.children}</div>
	),
	DropdownMenuContent: (props: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={props.className}>{props.children}</div>,
	DropdownMenuItem: (props: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
	}) => (
		<button type="button" disabled={props.disabled} onClick={props.onClick}>
			{props.children}
		</button>
	),
	DropdownMenuSeparator: () => <hr />,
	DropdownMenuTrigger: (props: {
		children?: React.ReactNode;
		render?: React.ReactNode;
	}) => <div>{props.render ?? props.children}</div>,
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: () => <span>icon</span>,
}));

function renderToolbar(
	overrides: Partial<React.ComponentProps<typeof FileBrowserToolbar>> = {},
) {
	const handlers = {
		onBreadcrumbDragLeave: vi.fn(),
		onBreadcrumbDragOver: vi.fn(),
		onBreadcrumbDrop: vi.fn().mockResolvedValue(undefined),
		onCreateFile: vi.fn(),
		onCreateFolder: vi.fn(),
		onNavigateToFolder: vi.fn(),
		onOfflineDownload: vi.fn(),
		onRefresh: vi.fn(),
		onSetSortBy: vi.fn(),
		onSetSortOrder: vi.fn(),
		onSetViewMode: vi.fn(),
		onTriggerFileUpload: vi.fn(),
		onTriggerFolderUpload: vi.fn(),
	};
	const props = {
		breadcrumb: [
			{ id: null, name: "Root" },
			{ id: 2, name: "Docs" },
			{ id: 3, name: "Workspace" },
			{ id: 4, name: "Final" },
		],
		dragOverBreadcrumbIndex: null,
		isCompactBreadcrumb: true,
		isRootFolder: false,
		isSearching: false,
		searchQuery: null,
		selectionToolbar: null,
		sortBy: "name",
		sortOrder: "asc",
		uploadReady: true,
		viewMode: "grid",
		...handlers,
		...overrides,
	} satisfies React.ComponentProps<typeof FileBrowserToolbar>;

	const result = render(<FileBrowserToolbar {...props} />);

	return { ...handlers, ...result, props };
}

describe("FileBrowserToolbar", () => {
	it("renders compact breadcrumbs and wires toolbar actions", () => {
		const handlers = renderToolbar();

		fireEvent.click(screen.getByText("Docs"));
		fireEvent.click(screen.getByRole("button", { name: "core:refresh" }));
		fireEvent.click(screen.getByRole("button", { name: "sort-by-updated" }));
		fireEvent.click(screen.getByRole("button", { name: "sort-order-desc" }));
		fireEvent.click(screen.getByRole("button", { name: "view-list" }));
		fireEvent.click(
			screen.getByRole("button", { name: "folder_more_actions" }),
		);
		fireEvent.click(screen.getByText("upload_file"));
		fireEvent.click(screen.getByText("upload_folder"));
		fireEvent.click(screen.getByText("new_folder"));
		fireEvent.click(screen.getByText("new_file"));
		fireEvent.click(screen.getByText("tasks:offline_download_action"));

		expect(
			screen.getByRole("button", { name: "core:more" }),
		).toBeInTheDocument();
		expect(screen.getByText("Final")).toBeInTheDocument();
		expect(handlers.onNavigateToFolder).toHaveBeenCalledWith(2, "Docs");
		expect(handlers.onRefresh).toHaveBeenCalledTimes(1);
		expect(handlers.onSetSortBy).toHaveBeenCalledWith("updated_at");
		expect(handlers.onSetSortOrder).toHaveBeenCalledWith("desc");
		expect(handlers.onSetViewMode).toHaveBeenCalledWith("list");
		expect(handlers.onTriggerFileUpload).toHaveBeenCalledTimes(1);
		expect(handlers.onTriggerFolderUpload).toHaveBeenCalledTimes(1);
		expect(handlers.onCreateFolder).toHaveBeenCalledTimes(1);
		expect(handlers.onCreateFile).toHaveBeenCalledTimes(1);
		expect(handlers.onOfflineDownload).toHaveBeenCalledTimes(1);
	});

	it("shows the search summary instead of breadcrumbs while searching", () => {
		renderToolbar({
			isSearching: true,
			searchQuery: "budget",
		});

		expect(screen.getByText('core:search: "budget"')).toBeInTheDocument();
		expect(screen.queryByText("Root")).not.toBeInTheDocument();
	});

	it("switches to selection actions when items are selected", () => {
		const selectionHandlers = {
			onArchiveCompress: vi.fn(),
			onDownload: vi.fn(),
			onClearSelection: vi.fn(),
			onCopy: vi.fn(),
			onDelete: vi.fn(),
			onManageTags: vi.fn(),
			onMove: vi.fn(),
			onToggleDisplayedSelection: vi.fn(),
		};

		renderToolbar({
			selectionToolbar: {
				count: 3,
				allDisplayedSelected: false,
				downloadAction: {
					kind: "archive",
					onClick: selectionHandlers.onDownload,
				},
				hasDisplayedItems: true,
				onArchiveCompress: selectionHandlers.onArchiveCompress,
				onClearSelection: selectionHandlers.onClearSelection,
				onCopy: selectionHandlers.onCopy,
				onDelete: selectionHandlers.onDelete,
				onManageTags: selectionHandlers.onManageTags,
				onMove: selectionHandlers.onMove,
				onToggleDisplayedSelection:
					selectionHandlers.onToggleDisplayedSelection,
			},
		});

		expect(screen.getAllByText("selected:3")).toHaveLength(2);
		expect(screen.getByText("Final")).toBeInTheDocument();
		expect(
			screen.getByTestId("file-browser-mobile-selection-toolbar"),
		).toBeInTheDocument();
		expect(
			screen.getAllByRole("button", { name: "selection_more_actions" }),
		).toHaveLength(2);

		fireEvent.click(
			screen.getAllByRole("button", { name: "selection_clear" })[0],
		);
		fireEvent.click(screen.getAllByText("selection_select_all_visible")[0]);
		fireEvent.click(screen.getAllByRole("button", { name: "move_to" })[0]);
		fireEvent.click(screen.getAllByText("copy_to")[0]);
		fireEvent.click(screen.getAllByText("tag_manage")[0]);
		fireEvent.click(screen.getAllByText("tasks:archive_download_action")[0]);
		fireEvent.click(screen.getAllByText("tasks:archive_compress_action")[0]);
		fireEvent.click(screen.getAllByText("core:delete")[0]);

		expect(selectionHandlers.onClearSelection).toHaveBeenCalledTimes(1);
		expect(selectionHandlers.onToggleDisplayedSelection).toHaveBeenCalledTimes(
			1,
		);
		expect(selectionHandlers.onMove).toHaveBeenCalledTimes(1);
		expect(selectionHandlers.onCopy).toHaveBeenCalledTimes(1);
		expect(selectionHandlers.onManageTags).toHaveBeenCalledTimes(1);
		expect(selectionHandlers.onDownload).toHaveBeenCalledTimes(1);
		expect(selectionHandlers.onArchiveCompress).toHaveBeenCalledTimes(1);
		expect(selectionHandlers.onDelete).toHaveBeenCalledTimes(1);
	});

	it("exposes compact mobile selection actions from the bottom toolbar", () => {
		const selectionHandlers = {
			onDownload: vi.fn(),
			onClearSelection: vi.fn(),
			onMove: vi.fn(),
			onToggleDisplayedSelection: vi.fn(),
		};

		renderToolbar({
			selectionToolbar: {
				count: 2,
				allDisplayedSelected: false,
				downloadAction: {
					kind: "archive",
					onClick: selectionHandlers.onDownload,
				},
				hasDisplayedItems: true,
				onClearSelection: selectionHandlers.onClearSelection,
				onCopy: vi.fn(),
				onDelete: vi.fn(),
				onManageTags: vi.fn(),
				onMove: selectionHandlers.onMove,
				onToggleDisplayedSelection:
					selectionHandlers.onToggleDisplayedSelection,
			},
		});

		const mobileToolbar = screen.getByTestId(
			"file-browser-mobile-selection-toolbar",
		);

		fireEvent.click(
			within(mobileToolbar).getByRole("button", { name: "selection_clear" }),
		);
		fireEvent.click(
			within(mobileToolbar).getAllByText("selection_select_all_visible")[0],
		);
		fireEvent.click(
			within(mobileToolbar).getByRole("button", {
				name: "tasks:archive_download_action",
			}),
		);
		fireEvent.click(
			within(mobileToolbar).getByRole("button", { name: "move_to" }),
		);

		expect(selectionHandlers.onClearSelection).toHaveBeenCalledTimes(1);
		expect(selectionHandlers.onToggleDisplayedSelection).toHaveBeenCalledTimes(
			1,
		);
		expect(selectionHandlers.onDownload).toHaveBeenCalledTimes(1);
		expect(selectionHandlers.onMove).toHaveBeenCalledTimes(1);
	});

	it("labels the selection download action as a regular download for a single file", () => {
		const onDownload = vi.fn();
		const onManageTags = vi.fn();

		renderToolbar({
			selectionToolbar: {
				count: 1,
				allDisplayedSelected: false,
				downloadAction: {
					kind: "file",
					onClick: onDownload,
				},
				hasDisplayedItems: true,
				onClearSelection: vi.fn(),
				onCopy: vi.fn(),
				onDelete: vi.fn(),
				onManageTags,
				onMove: vi.fn(),
				onToggleDisplayedSelection: vi.fn(),
			},
		});

		fireEvent.click(screen.getAllByText("download")[0]);
		fireEvent.click(screen.getAllByText("tag_manage")[0]);

		expect(onDownload).toHaveBeenCalledTimes(1);
		expect(onManageTags).toHaveBeenCalledTimes(1);
		expect(
			screen.queryByText("tasks:archive_download_action"),
		).not.toBeInTheDocument();
	});

	it("keeps selection content during fade-out before restoring breadcrumbs", () => {
		vi.useFakeTimers();
		try {
			const selectionToolbar = {
				count: 2,
				allDisplayedSelected: false,
				hasDisplayedItems: true,
				onClearSelection: vi.fn(),
				onCopy: vi.fn(),
				onDelete: vi.fn(),
				onManageTags: vi.fn(),
				onMove: vi.fn(),
				onToggleDisplayedSelection: vi.fn(),
			};
			const { props, rerender } = renderToolbar({ selectionToolbar });

			expect(screen.getAllByText("selected:2")).toHaveLength(2);
			expect(screen.getByText("Final")).toBeInTheDocument();

			rerender(<FileBrowserToolbar {...props} selectionToolbar={null} />);

			expect(screen.getAllByText("selected:2")).toHaveLength(2);
			expect(screen.getByText("Final")).toBeInTheDocument();
			expect(
				screen.getByTestId("file-browser-selection-toolbar"),
			).toHaveAttribute("aria-hidden", "false");

			act(() => {
				vi.advanceTimersByTime(40);
			});

			expect(
				screen.getByTestId("file-browser-selection-toolbar"),
			).toHaveAttribute("aria-hidden", "true");

			act(() => {
				vi.advanceTimersByTime(119);
			});

			expect(screen.getAllByText("selected:2")).toHaveLength(2);
			expect(screen.getByText("Final")).toBeInTheDocument();

			act(() => {
				vi.advanceTimersByTime(120);
			});

			expect(screen.queryByText("selected:2")).not.toBeInTheDocument();
			expect(screen.getByText("Final")).toBeInTheDocument();
		} finally {
			vi.useRealTimers();
		}
	});

	it("does not flash breadcrumbs when selection briefly drops for one render", () => {
		vi.useFakeTimers();
		try {
			const selectionToolbar = {
				count: 2,
				allDisplayedSelected: false,
				hasDisplayedItems: true,
				onClearSelection: vi.fn(),
				onCopy: vi.fn(),
				onDelete: vi.fn(),
				onManageTags: vi.fn(),
				onMove: vi.fn(),
				onToggleDisplayedSelection: vi.fn(),
			};
			const { props, rerender } = renderToolbar({ selectionToolbar });

			rerender(<FileBrowserToolbar {...props} selectionToolbar={null} />);
			expect(screen.getAllByText("selected:2")).toHaveLength(2);
			expect(screen.getByText("Final")).toBeInTheDocument();

			rerender(
				<FileBrowserToolbar {...props} selectionToolbar={selectionToolbar} />,
			);
			expect(screen.getAllByText("selected:2")).toHaveLength(2);
			expect(screen.getByText("Final")).toBeInTheDocument();
		} finally {
			vi.useRealTimers();
		}
	});

	it("keeps selection content stable during rapid deselect and reselect", () => {
		vi.useFakeTimers();
		try {
			const selectionToolbar = {
				count: 2,
				allDisplayedSelected: false,
				hasDisplayedItems: true,
				onClearSelection: vi.fn(),
				onCopy: vi.fn(),
				onDelete: vi.fn(),
				onManageTags: vi.fn(),
				onMove: vi.fn(),
				onToggleDisplayedSelection: vi.fn(),
			};
			const nextSelectionToolbar = {
				...selectionToolbar,
				count: 1,
			};
			const { props, rerender } = renderToolbar({ selectionToolbar });

			rerender(<FileBrowserToolbar {...props} selectionToolbar={null} />);

			act(() => {
				vi.advanceTimersByTime(30);
			});

			expect(screen.getAllByText("selected:2")).toHaveLength(2);
			expect(screen.getByText("Final")).toBeInTheDocument();

			rerender(
				<FileBrowserToolbar
					{...props}
					selectionToolbar={nextSelectionToolbar}
				/>,
			);

			expect(screen.getAllByText("selected:1")).toHaveLength(2);
			expect(screen.getByText("Final")).toBeInTheDocument();

			act(() => {
				vi.advanceTimersByTime(300);
			});

			expect(screen.getAllByText("selected:1")).toHaveLength(2);
			expect(screen.getByText("Final")).toBeInTheDocument();
		} finally {
			vi.useRealTimers();
		}
	});
});
