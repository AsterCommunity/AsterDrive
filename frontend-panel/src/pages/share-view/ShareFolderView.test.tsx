import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
	FileBrowserBatchSelectionActions,
	FileBrowserContextValue,
} from "@/components/files/FileBrowserContext";
import { ShareFolderView } from "@/pages/share-view/ShareFolderView";
import { useFileStore } from "@/stores/fileStore";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import type {
	FileListItem,
	FolderContents,
	FolderListItem,
	SharePublicInfo,
} from "@/types/api";

const mockState = vi.hoisted(() => ({
	capturedContextValues: [] as FileBrowserContextValue[],
	translate: (key: string, opts?: Record<string, unknown>) => {
		if (key === "core:selected_count") return `selected:${opts?.count}`;
		return key.replace(/^core:/, "");
	},
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: mockState.translate,
	}),
}));

vi.mock("@/components/common/UserAvatarImage", () => ({
	UserAvatarImage: ({ name }: { name: string }) => (
		<div>{`avatar:${name}`}</div>
	),
}));

vi.mock("@/components/common/ViewToggle", () => ({
	ViewToggle: ({ onChange }: { onChange: (value: "list") => void }) => (
		<button type="button" onClick={() => onChange("list")}>
			view-toggle
		</button>
	),
}));

vi.mock("@/components/common/SortMenu", () => ({
	SortMenu: ({
		onSortBy,
		onSortOrder,
	}: {
		onSortBy: (value: "updated_at") => void;
		onSortOrder: (value: "desc") => void;
	}) => (
		<div>
			<button type="button" onClick={() => onSortBy("updated_at")}>
				sort-menu
			</button>
			<button type="button" onClick={() => onSortOrder("desc")}>
				sort-desc
			</button>
		</div>
	),
}));

vi.mock("@/components/common/ToolbarBar", () => ({
	ToolbarBar: ({
		left,
		right,
	}: {
		left: React.ReactNode;
		right?: React.ReactNode;
	}) => (
		<div>
			<div>{left}</div>
			<div>{right}</div>
		</div>
	),
}));

vi.mock("@/components/files/FileBrowserContext", async (importOriginal) => {
	const actual =
		await importOriginal<
			typeof import("@/components/files/FileBrowserContext")
		>();
	return {
		...actual,
		FileBrowserProvider: ({
			children,
			value,
		}: {
			children: React.ReactNode;
			value: FileBrowserContextValue;
		}) => {
			mockState.capturedContextValues.push(value);
			return (
				<actual.FileBrowserProvider value={value}>
					{children}
				</actual.FileBrowserProvider>
			);
		},
	};
});

vi.mock("@/components/files/FileGrid", async (importOriginal) => {
	const actual = await import("@/components/files/FileBrowserContext");
	return {
		...(await importOriginal<object>()),
		FileGrid: () => {
			const context = actual.useFileBrowserContext();
			return (
				<div data-testid="file-grid">
					<span>{`path:${context.breadcrumbPathIds.join("/")}`}</span>
					<span>{`files:${context.files.length}`}</span>
					<span>{`batch:${context.batchSelectionActions?.count ?? 0}`}</span>
				</div>
			);
		},
	};
});

vi.mock("@/components/files/FileTable", () => ({
	FileTable: () => <div data-testid="file-table" />,
}));

vi.mock("@/components/ui/breadcrumb", () => ({
	Breadcrumb: ({ children }: { children: React.ReactNode }) => (
		<nav>{children}</nav>
	),
	BreadcrumbItem: ({ children }: { children: React.ReactNode }) => (
		<span>{children}</span>
	),
	BreadcrumbLink: ({
		children,
		onClick,
	}: {
		children: React.ReactNode;
		onClick?: () => void;
	}) => (
		<button type="button" onClick={onClick}>
			{children}
		</button>
	),
	BreadcrumbList: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	BreadcrumbPage: ({ children }: { children: React.ReactNode }) => (
		<span>{children}</span>
	),
	BreadcrumbSeparator: () => <span>/</span>,
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/pages/file-browser/useFileBrowserBatchActions", () => ({
	useFileBrowserBatchActions: ({
		displayFiles,
		displayFolders,
	}: {
		displayFiles: FileListItem[];
		displayFolders: FolderListItem[];
	}) => {
		const selectedFileIds = useFileStore((s) => s.selectedFileIds);
		const selectedFolderIds = useFileStore((s) => s.selectedFolderIds);
		const clearSelection = useFileStore((s) => s.clearSelection);
		const count = selectedFileIds.size + selectedFolderIds.size;
		const actions: FileBrowserBatchSelectionActions | null =
			count > 0
				? {
						count,
						downloadAction: { kind: "archive", onClick: vi.fn() },
					}
				: null;

		return {
			dialogs: null,
			selectionToolbar: actions
				? {
						...actions,
						allDisplayedSelected:
							count === displayFiles.length + displayFolders.length,
						hasDisplayedItems: displayFiles.length + displayFolders.length > 0,
						onClearSelection: clearSelection,
						onDelete: undefined,
						onToggleDisplayedSelection: vi.fn(),
					}
				: null,
		};
	},
}));

vi.mock("@/services/shareService", () => ({
	shareService: {
		streamArchiveDownload: vi.fn(),
	},
}));

vi.mock("@/components/layout/ShareTopBar", () => ({
	ShareTopBar: () => <div data-testid="share-topbar" />,
}));

vi.mock("@/pages/share-view/ShareFolderSidebar", () => ({
	ShareFolderSidebar: () => <aside data-testid="share-folder-sidebar" />,
}));

function createFile(id: number, name = `file-${id}.txt`): FileListItem {
	return {
		created_at: "2026-01-01T00:00:00Z",
		folder_id: null,
		id,
		is_shared: false,
		locked: false,
		mime_type: "text/plain",
		name,
		size: id,
		updated_at: "2026-01-01T00:00:00Z",
	} as FileListItem;
}

function createContents(
	files: FileListItem[] = [createFile(1)],
): FolderContents {
	return {
		files,
		folders: [],
		next_file_cursor: null,
	} as FolderContents;
}

function createInfo(): SharePublicInfo {
	return {
		download_count: 0,
		has_password: false,
		is_expired: false,
		max_downloads: 0,
		name: "Shared Root",
		share_type: "folder",
		shared_by: {
			avatar: null,
			name: "Alice",
		},
		token: "share-token",
		view_count: 0,
	} as SharePublicInfo;
}

function renderFolderView({
	breadcrumb,
	folderContents = createContents(),
}: {
	breadcrumb: Array<{ id: number | null; name: string }>;
	folderContents?: FolderContents;
}) {
	return render(
		<ShareFolderView
			breadcrumb={breadcrumb}
			folderContents={folderContents}
			hasMoreFiles={false}
			info={createInfo()}
			loadingMore={false}
			navigating={false}
			onFileDownload={vi.fn()}
			onFilePreview={vi.fn()}
			onNavigateToFolder={vi.fn()}
			onRefresh={vi.fn()}
			onSortByChange={vi.fn()}
			onSortOrderChange={vi.fn()}
			onViewModeChange={vi.fn()}
			previewElement={null}
			sentinelRef={{ current: null }}
			shareOwnerText="shared-by:Alice"
			sortBy="name"
			sortOrder="asc"
			token="share-token"
			viewMode="grid"
		/>,
	);
}

function createStableProps({
	breadcrumb,
	folderContents = createContents(),
}: {
	breadcrumb: Array<{ id: number | null; name: string }>;
	folderContents?: FolderContents;
}) {
	return {
		breadcrumb,
		folderContents,
		hasMoreFiles: false,
		info: createInfo(),
		loadingMore: false,
		navigating: false,
		onFileDownload: vi.fn(),
		onFilePreview: vi.fn(),
		onNavigateToFolder: vi.fn(),
		onRefresh: vi.fn(),
		onSortByChange: vi.fn(),
		onSortOrderChange: vi.fn(),
		onViewModeChange: vi.fn(),
		previewElement: null,
		sentinelRef: { current: null },
		shareOwnerText: "shared-by:Alice",
		sortBy: "name" as const,
		sortOrder: "asc" as const,
		token: "share-token",
		viewMode: "grid" as const,
	};
}

describe("ShareFolderView", () => {
	beforeEach(() => {
		mockState.capturedContextValues = [];
		useFileStore.setState({
			selectedFileIds: new Set(),
			selectedFolderIds: new Set(),
		});
		useFrontendConfigStore.setState({
			archiveDownloadShareEnabled: true,
			isLoaded: true,
		});
	});

	afterEach(() => {
		useFileStore.setState({
			selectedFileIds: new Set(),
			selectedFolderIds: new Set(),
		});
	});

	it("uses share-specific copy for an empty shared folder", () => {
		renderFolderView({
			breadcrumb: [{ id: null, name: "Shared Root" }],
			folderContents: createContents([]),
		});

		expect(screen.getByText("empty_folder")).toBeInTheDocument();
		expect(screen.getByText("share:empty_folder_desc")).toBeInTheDocument();
		expect(
			screen.queryByText("files:folder_empty_desc"),
		).not.toBeInTheDocument();
	});

	it("hides folder and multi-selection archive downloads when disabled", async () => {
		useFrontendConfigStore.setState({
			archiveDownloadShareEnabled: false,
			isLoaded: true,
		});
		renderFolderView({
			breadcrumb: [{ id: null, name: "Shared Root" }],
			folderContents: createContents([
				createFile(1, "alpha.txt"),
				createFile(2, "beta.txt"),
			]),
		});

		await screen.findByTestId("file-grid");
		expect(mockState.capturedContextValues.at(-1)?.onArchiveDownload).toBe(
			undefined,
		);

		useFileStore.getState().selectItems([1, 2], []);
		await waitFor(() => {
			expect(
				mockState.capturedContextValues.at(-1)?.batchSelectionActions
					?.downloadAction,
			).toBeUndefined();
		});
	});

	it("uses the shared navigation toolbar for refresh, sorting, and view changes", () => {
		const props = createStableProps({
			breadcrumb: [{ id: null, name: "Shared Root" }],
		});
		render(<ShareFolderView {...props} />);

		fireEvent.click(screen.getByRole("button", { name: "refresh" }));
		fireEvent.click(screen.getByRole("button", { name: "sort-menu" }));
		fireEvent.click(screen.getByRole("button", { name: "sort-desc" }));
		fireEvent.click(screen.getByRole("button", { name: "view-toggle" }));

		expect(screen.getByText("Shared Root")).toBeInTheDocument();
		expect(props.onRefresh).toHaveBeenCalledTimes(1);
		expect(props.onSortByChange).toHaveBeenCalledWith("updated_at");
		expect(props.onSortOrderChange).toHaveBeenCalledWith("desc");
		expect(props.onViewModeChange).toHaveBeenCalledWith("list");
	});

	it("selects all displayed share items with Command/Ctrl+A and clears with Escape", async () => {
		renderFolderView({
			breadcrumb: [{ id: null, name: "Shared Root" }],
			folderContents: createContents([
				createFile(1, "alpha.txt"),
				createFile(2, "beta.txt"),
			]),
		});

		fireEvent.keyDown(document, { key: "a", metaKey: true });

		await waitFor(() => {
			expect(useFileStore.getState().selectedFileIds).toEqual(new Set([1, 2]));
		});

		fireEvent.keyDown(document, { key: "Escape" });

		await waitFor(() => {
			expect(useFileStore.getState().selectedFileIds.size).toBe(0);
		});
	});

	it("keeps selection across renders when breadcrumb ids are unchanged", async () => {
		const rootBreadcrumb = [{ id: null, name: "Shared Root" }];
		const contents = createContents([createFile(1, "alpha.txt")]);
		const { rerender } = renderFolderView({
			breadcrumb: rootBreadcrumb,
			folderContents: contents,
		});

		await screen.findByTestId("file-grid");
		useFileStore.getState().selectItems([1], []);
		expect(await screen.findAllByText("selected:1")).toHaveLength(2);
		expect(screen.getByTestId("file-browser-default-toolbar")).toHaveAttribute(
			"aria-hidden",
			"true",
		);
		expect(
			screen.getByTestId("file-browser-mobile-selection-toolbar"),
		).toBeInTheDocument();

		rerender(
			<ShareFolderView
				breadcrumb={[{ id: null, name: "Shared Root" }]}
				folderContents={contents}
				hasMoreFiles={false}
				info={createInfo()}
				loadingMore={false}
				navigating={false}
				onFileDownload={vi.fn()}
				onFilePreview={vi.fn()}
				onNavigateToFolder={vi.fn()}
				onRefresh={vi.fn()}
				onSortByChange={vi.fn()}
				onSortOrderChange={vi.fn()}
				onViewModeChange={vi.fn()}
				previewElement={null}
				sentinelRef={{ current: null }}
				shareOwnerText="shared-by:Alice"
				sortBy="name"
				sortOrder="asc"
				token="share-token"
				viewMode="grid"
			/>,
		);

		expect(await screen.findAllByText("selected:1")).toHaveLength(2);
		expect(useFileStore.getState().selectedFileIds).toEqual(new Set([1]));
	});

	it("clears selection when breadcrumb ids change", async () => {
		const contents = createContents([createFile(1, "alpha.txt")]);
		const { rerender } = renderFolderView({
			breadcrumb: [{ id: null, name: "Shared Root" }],
			folderContents: contents,
		});

		await screen.findByTestId("file-grid");
		useFileStore.getState().selectItems([1], []);
		expect(await screen.findAllByText("selected:1")).toHaveLength(2);

		rerender(
			<ShareFolderView
				breadcrumb={[
					{ id: null, name: "Shared Root" },
					{ id: 10, name: "Nested" },
				]}
				folderContents={contents}
				hasMoreFiles={false}
				info={createInfo()}
				loadingMore={false}
				navigating={false}
				onFileDownload={vi.fn()}
				onFilePreview={vi.fn()}
				onNavigateToFolder={vi.fn()}
				onRefresh={vi.fn()}
				onSortByChange={vi.fn()}
				onSortOrderChange={vi.fn()}
				onViewModeChange={vi.fn()}
				previewElement={null}
				sentinelRef={{ current: null }}
				shareOwnerText="shared-by:Alice"
				sortBy="name"
				sortOrder="asc"
				token="share-token"
				viewMode="grid"
			/>,
		);

		await waitFor(() => {
			expect(useFileStore.getState().selectedFileIds.size).toBe(0);
		});
		await waitFor(() => {
			expect(screen.queryByText("selected:1")).not.toBeInTheDocument();
		});
	});

	it("memoizes the file browser context while visible content is unchanged", async () => {
		const contents = createContents([createFile(1, "alpha.txt")]);
		const props = createStableProps({
			breadcrumb: [{ id: null, name: "Shared Root" }],
			folderContents: contents,
		});
		const { rerender } = render(<ShareFolderView {...props} />);

		await screen.findByTestId("file-grid");
		const initialContext = mockState.capturedContextValues.at(-1);
		expect(initialContext).toBeDefined();

		rerender(
			<ShareFolderView
				{...props}
				breadcrumb={[{ id: null, name: "Shared Root" }]}
			/>,
		);

		expect(mockState.capturedContextValues.at(-1)).toBe(initialContext);
	});
});
