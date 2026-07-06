import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
	FileBrowserBatchSelectionActions,
	FileBrowserContextValue,
} from "@/components/files/FileBrowserContext";
import { ShareFolderView } from "@/pages/share-view/ShareFolderView";
import { useFileStore } from "@/stores/fileStore";
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
	ViewToggle: () => <div>view-toggle</div>,
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
			onViewModeChange={vi.fn()}
			previewElement={null}
			sentinelRef={{ current: null }}
			shareOwnerText="shared-by:Alice"
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
		onViewModeChange: vi.fn(),
		previewElement: null,
		sentinelRef: { current: null },
		shareOwnerText: "shared-by:Alice",
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

	it("keeps selection across renders when breadcrumb ids are unchanged", async () => {
		const rootBreadcrumb = [{ id: null, name: "Shared Root" }];
		const contents = createContents([createFile(1, "alpha.txt")]);
		const { rerender } = renderFolderView({
			breadcrumb: rootBreadcrumb,
			folderContents: contents,
		});

		await screen.findByTestId("file-grid");
		useFileStore.getState().selectItems([1], []);
		await screen.findByText("selected:1");

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
				onViewModeChange={vi.fn()}
				previewElement={null}
				sentinelRef={{ current: null }}
				shareOwnerText="shared-by:Alice"
				token="share-token"
				viewMode="grid"
			/>,
		);

		expect(await screen.findByText("selected:1")).toBeInTheDocument();
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
		await screen.findByText("selected:1");

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
				onViewModeChange={vi.fn()}
				previewElement={null}
				sentinelRef={{ current: null }}
				shareOwnerText="shared-by:Alice"
				token="share-token"
				viewMode="grid"
			/>,
		);

		await waitFor(() => {
			expect(useFileStore.getState().selectedFileIds.size).toBe(0);
		});
		expect(screen.queryByText("selected:1")).not.toBeInTheDocument();
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
