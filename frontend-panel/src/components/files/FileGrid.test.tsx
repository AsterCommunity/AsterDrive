import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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
		selectRangeTo: vi.fn(),
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

const gridItemMock = ({
	item,
	testId,
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
	testId: string;
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
			data-testid={testId}
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
};

vi.mock("@/components/files/FileCard", () => ({
	FileCard: (props: Record<string, unknown>) =>
		gridItemMock({ ...(props as never), testId: "file-card" }),
}));

vi.mock("@/components/files/FolderGridItem", () => ({
	FolderGridItem: (props: Record<string, unknown>) =>
		gridItemMock({ ...(props as never), testId: "folder-card" }),
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
		mockState.store.selectRangeTo.mockReset();
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

	it("applies the D8 entrance classes with the files section delayed", () => {
		mockState.browserContext.files = [{ id: 2, name: "report.pdf" }];
		mockState.browserContext.folders = [{ id: 1, name: "Docs" }];

		render(<FileGrid />);

		const foldersSection = screen.getByText("translated:folders_section")
			.parentElement as HTMLElement;
		const filesSection = screen.getByText("translated:files_section")
			.parentElement as HTMLElement;
		// 文件夹区先入场，文件区带 80ms 错开延迟
		expect(foldersSection).toHaveClass("file-browser-enter");
		expect(foldersSection).not.toHaveClass("file-browser-enter-delayed");
		expect(filesSection).toHaveClass("file-browser-enter");
		expect(filesSection).toHaveClass("file-browser-enter-delayed");
	});

	it("enters without the stagger delay when only one section has content", () => {
		mockState.browserContext.files = [{ id: 2, name: "report.pdf" }];

		render(<FileGrid />);

		const filesSection = screen
			.getByTestId("file-card")
			.closest(".file-browser-enter");
		expect(filesSection).not.toBeNull();
		expect(filesSection).not.toHaveClass("file-browser-enter-delayed");
	});

	it("plays the entrance animation once when content arrives after mount", () => {
		const view = render(<FileGrid />);

		expect(document.querySelector(".file-browser-enter")).toBeNull();

		mockState.browserContext.folders = [{ id: 1, name: "Docs" }];
		// FileGrid 是 memo 组件且测试里的 context 是直读 mock（非真 context，
		// 不能穿透 memo），需要改变 props 引用才会重渲染并走进场 effect
		view.rerender(<FileGrid scrollElement={null} />);

		expect(document.querySelector(".file-browser-enter")).not.toBeNull();

		// 只播一次：类加上后不随后续数据变化移除/重加（CSS 动画不重播）
		mockState.browserContext.files = [{ id: 2, name: "report.pdf" }];
		view.rerender(<FileGrid scrollElement={undefined} />);

		expect(document.querySelector(".file-browser-enter")).not.toBeNull();
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

	it("applies modifier selection instead of opening on Cmd/Ctrl+click and Shift+click", () => {
		mockState.browserContext.files = [{ id: 2, name: "report.pdf" }];
		mockState.browserContext.folders = [{ id: 1, name: "Docs" }];

		render(<FileGrid />);

		fireEvent.click(screen.getByRole("button", { name: "open:report.pdf" }), {
			metaKey: true,
		});
		expect(mockState.store.toggleFileSelection).toHaveBeenCalledWith(2);
		expect(mockState.browserContext.onFileClick).not.toHaveBeenCalled();

		fireEvent.click(screen.getByRole("button", { name: "open:Docs" }), {
			ctrlKey: true,
		});
		expect(mockState.store.toggleFolderSelection).toHaveBeenCalledWith(1);
		expect(mockState.browserContext.onFolderOpen).not.toHaveBeenCalled();

		fireEvent.click(screen.getByRole("button", { name: "open:report.pdf" }), {
			shiftKey: true,
		});
		expect(mockState.store.selectRangeTo).toHaveBeenCalledWith("file", 2);
		expect(mockState.browserContext.onFileClick).not.toHaveBeenCalled();
	});

	it("keeps modifier clicks inert when selection is disabled", () => {
		mockState.browserContext.selectionEnabled = false;
		mockState.browserContext.files = [{ id: 2, name: "report.pdf" }];

		render(<FileGrid />);

		fireEvent.click(screen.getByRole("button", { name: "open:report.pdf" }), {
			metaKey: true,
		});

		expect(mockState.store.toggleFileSelection).not.toHaveBeenCalled();
		expect(mockState.browserContext.onFileClick).toHaveBeenCalledWith(
			expect.objectContaining({ id: 2 }),
		);
	});
});

describe("FileGrid column responsiveness", () => {
	class MockResizeObserver {
		static instances: MockResizeObserver[] = [];
		private callback: ResizeObserverCallback;
		private elements = new Set<Element>();

		constructor(callback: ResizeObserverCallback) {
			this.callback = callback;
			MockResizeObserver.instances.push(this);
		}

		observe(element: Element) {
			this.elements.add(element);
		}
		unobserve(element: Element) {
			this.elements.delete(element);
		}
		disconnect() {
			this.elements.clear();
		}

		trigger(width: number) {
			const entry = { contentRect: { width } } as ResizeObserverEntry;
			for (const element of Array.from(this.elements)) {
				void element;
				this.callback([entry], this as unknown as ResizeObserver);
			}
		}
	}

	function lastObserver() {
		return MockResizeObserver.instances[
			MockResizeObserver.instances.length - 1
		];
	}

	beforeEach(() => {
		MockResizeObserver.instances = [];
		// setup.ts 的 ResizeObserver mock 是 writable 但不可 redefine，
		// 直接赋值替换，跑完恢复
		window.ResizeObserver =
			MockResizeObserver as unknown as typeof ResizeObserver;
	});

	afterEach(() => {
		window.ResizeObserver = undefined as unknown as typeof ResizeObserver;
	});

	it("renders the measured column count into the grid template", () => {
		mockState.browserContext.files = [{ id: 2, name: "report.pdf" }];
		const { container } = render(<FileGrid />);
		const grid = container.querySelector(".grid");

		// 初始未测量：1 列
		expect(grid).toHaveStyle({
			gridTemplateColumns: "repeat(1, minmax(0, 1fr))",
		});

		// 首测同步生效（flushSync，无 1 列闪烁帧）
		act(() => lastObserver().trigger(1560));
		expect(grid).toHaveStyle({
			gridTemplateColumns: "repeat(6, minmax(0, 1fr))",
		});

		// 收窄到手机宽度：2 列
		act(() => lastObserver().trigger(343));
		expect(grid).toHaveStyle({
			gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
		});
	});

	it("routes column changes through view transitions when available, skipping the first measurement", () => {
		const startViewTransition = vi.fn((callback: () => void) => {
			callback();
		});
		Object.defineProperty(document, "startViewTransition", {
			configurable: true,
			value: startViewTransition,
		});
		// matchMedia 由 test/setup.ts 提供（matches: false，即非 reduced-motion）

		mockState.browserContext.files = [{ id: 2, name: "report.pdf" }];
		const { container } = render(<FileGrid />);
		const grid = container.querySelector(".grid");

		act(() => lastObserver().trigger(1560));
		// 首次测量直接同步提交，不做过渡
		expect(startViewTransition).not.toHaveBeenCalled();
		expect(grid).toHaveStyle({
			gridTemplateColumns: "repeat(6, minmax(0, 1fr))",
		});

		// 后续裂列走 View Transitions
		act(() => lastObserver().trigger(1100));
		expect(startViewTransition).toHaveBeenCalledTimes(1);
		expect(grid).toHaveStyle({
			gridTemplateColumns: "repeat(5, minmax(0, 1fr))",
		});
	});
});
