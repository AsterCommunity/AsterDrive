import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import CategoryBrowserPage from "@/pages/CategoryBrowserPage";
import { useDownloadStore } from "@/stores/downloadStore";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import { useUploadAreaControlsStore } from "@/stores/uploadAreaControlsStore";
import type { FileListItem } from "@/types/api";

const mockState = vi.hoisted(() => ({
	beginLocalStorageDeleteMutation: vi.fn(),
	batchActionOptions: null as null | {
		onArchiveDownload?: (fileIds: number[], folderIds: number[]) => void;
		onDownload: (fileId: number, fileName: string) => void;
	},
	clearSelection: vi.fn(),
	deleteFile: vi.fn(),
	downloadPath: vi.fn(),
	getFile: vi.fn(),
	handleApiError: vi.fn(),
	loadPreviewApps: vi.fn(),
	navigate: vi.fn(),
	search: vi.fn(),
	setPageTitle: vi.fn(),
	setSortBy: vi.fn(),
	setSortOrder: vi.fn(),
	setViewMode: vi.fn(),
	selectItems: vi.fn(),
	streamArchiveDownload: vi.fn(),
	params: {
		category: "photo" as string | undefined,
	},
	previewAppsLoaded: true,
	thumbnailSupport: null,
	workspace: { kind: "personal" as const },
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, string>) =>
			options?.category ? `${key}:${options.category}` : key,
	}),
}));

vi.mock("react-router-dom", () => ({
	Navigate: ({ to }: { to: string }) => <div data-testid="navigate">{to}</div>,
	useNavigate: () => mockState.navigate,
	useParams: () => mockState.params,
}));

vi.mock("sonner", () => ({
	toast: {
		success: vi.fn(),
	},
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: mockState.handleApiError,
}));

vi.mock("@/hooks/usePageTitle", () => ({
	usePageTitle: (title: string) => mockState.setPageTitle(title),
}));

vi.mock("@/lib/authenticatedDownload", () => ({
	startAuthenticatedDownload: vi.fn(),
}));

vi.mock("@/lib/storageMutationCoordinator", async (importOriginal) => {
	const actual =
		await importOriginal<typeof import("@/lib/storageMutationCoordinator")>();
	return {
		...actual,
		beginLocalStorageDeleteMutation: (...args: unknown[]) =>
			mockState.beginLocalStorageDeleteMutation(...args),
	};
});

vi.mock("@/stores/workspaceStore", () => ({
	useWorkspaceStore: (
		selector: (state: { workspace: typeof mockState.workspace }) => unknown,
	) => selector({ workspace: mockState.workspace }),
}));

vi.mock("@/stores/previewAppStore", () => ({
	usePreviewAppStore: (
		selector: (state: {
			isLoaded: boolean;
			load: typeof mockState.loadPreviewApps;
		}) => unknown,
	) =>
		selector({
			isLoaded: mockState.previewAppsLoaded,
			load: mockState.loadPreviewApps,
		}),
}));

vi.mock("@/stores/thumbnailSupportStore", () => ({
	useThumbnailSupportStore: (
		selector: (state: { config: typeof mockState.thumbnailSupport }) => unknown,
	) => selector({ config: mockState.thumbnailSupport }),
}));

vi.mock("@/stores/fileStore", () => ({
	useFileStore: (
		selector: (state: {
			browserOpenMode: "single_click";
			viewMode: "grid";
			sortBy: "name";
			sortOrder: "asc";
			setViewMode: typeof mockState.setViewMode;
			setSortBy: typeof mockState.setSortBy;
			setSortOrder: typeof mockState.setSortOrder;
			clearSelection: typeof mockState.clearSelection;
			selectItems: typeof mockState.selectItems;
		}) => unknown,
	) =>
		selector({
			browserOpenMode: "single_click",
			viewMode: "grid",
			sortBy: "name",
			sortOrder: "asc",
			setViewMode: mockState.setViewMode,
			setSortBy: mockState.setSortBy,
			setSortOrder: mockState.setSortOrder,
			clearSelection: mockState.clearSelection,
			selectItems: mockState.selectItems,
		}),
}));

vi.mock("@/pages/file-browser/useFileBrowserBatchActions", () => ({
	useFileBrowserBatchActions: (
		options: NonNullable<typeof mockState.batchActionOptions>,
	) => {
		mockState.batchActionOptions = options;
		return {
			dialogs: null,
			selectionToolbar: {
				allDisplayedSelected: false,
				count: 1,
				downloadAction: undefined,
				hasDisplayedItems: true,
				onArchiveCompress: undefined,
				onClearSelection: vi.fn(),
				onCopy: undefined,
				onDelete: vi.fn(),
				onManageTags: vi.fn(),
				onMove: undefined,
				onToggleDisplayedSelection: vi.fn(),
			},
		};
	},
}));

vi.mock("@/services/searchService", () => ({
	searchService: {
		search: mockState.search,
	},
}));

vi.mock("@/services/batchService", () => ({
	batchService: {
		streamArchiveDownload: mockState.streamArchiveDownload,
	},
}));

vi.mock("@/services/fileService", () => ({
	fileService: {
		deleteFile: mockState.deleteFile,
		downloadPath: mockState.downloadPath,
		getFile: mockState.getFile,
		setFileLock: vi.fn(),
		createPreviewLink: vi.fn(),
		getArchivePreview: vi.fn(),
		createWopiSession: vi.fn(),
	},
}));

vi.mock("@/components/layout/AppLayout", () => ({
	AppLayout: ({ children }: { children: ReactNode }) => (
		<div data-testid="app-layout">{children}</div>
	),
}));

vi.mock("@/pages/file-browser/FileBrowserToolbar", () => ({
	FileBrowserToolbar: ({
		breadcrumb,
		currentFolderActions,
		onRefresh,
		selectionToolbar,
	}: {
		breadcrumb: Array<{ name: string }>;
		currentFolderActions?: "full" | "refresh-only";
		onRefresh: () => void;
		selectionToolbar: unknown;
	}) => (
		<div
			data-testid="toolbar"
			data-current-folder-actions={currentFolderActions ?? "full"}
			data-selection={String(Boolean(selectionToolbar))}
		>
			<span>{breadcrumb[0]?.name}</span>
			<button type="button" onClick={onRefresh}>
				refresh
			</button>
		</div>
	),
}));

vi.mock("@/pages/file-browser/FileBrowserWorkspace", () => ({
	FileBrowserWorkspace: ({
		currentFolderActions,
		emptyDescription,
		emptyTitle,
		fileBrowserContextValue,
		hasMoreFiles,
		infoPanelOpen,
		infoTarget,
		loading,
		bottomOverlayOffset,
		onInfoPanelOpenChange,
		suppressLoadMore,
	}: {
		currentFolderActions?: "full" | "refresh-only";
		emptyDescription?: string;
		emptyTitle?: string;
		fileBrowserContextValue: {
			files: FileListItem[];
			onCopy?: unknown;
			onDelete?: (type: "file" | "folder", id: number) => void;
			onGoToLocation?: (file: FileListItem) => void;
			onInfo?: (type: "file" | "folder", id: number) => void;
			onMove?: unknown;
		};
		hasMoreFiles: boolean;
		infoPanelOpen: boolean;
		infoTarget: { file?: FileListItem } | null;
		loading: boolean;
		bottomOverlayOffset?: string;
		onInfoPanelOpenChange: (open: boolean) => void;
		suppressLoadMore?: boolean;
	}) => (
		<div
			data-testid="workspace"
			data-current-folder-actions={currentFolderActions ?? "full"}
			data-empty-description={emptyDescription ?? ""}
			data-empty-title={emptyTitle ?? ""}
			data-has-more={String(hasMoreFiles)}
			data-loading={String(loading)}
			data-bottom-overlay={bottomOverlayOffset ?? "none"}
			data-suppress-load-more={String(Boolean(suppressLoadMore))}
			data-copy={String(Boolean(fileBrowserContextValue.onCopy))}
			data-info-open={String(infoPanelOpen)}
			data-info-target={infoTarget?.file?.name ?? ""}
			data-move={String(Boolean(fileBrowserContextValue.onMove))}
			data-location={String(Boolean(fileBrowserContextValue.onGoToLocation))}
		>
			{fileBrowserContextValue.files.map((file) => (
				<div key={file.id}>
					<button
						type="button"
						onClick={() => fileBrowserContextValue.onGoToLocation?.(file)}
					>
						{file.name}
					</button>
					<button
						type="button"
						onClick={() => fileBrowserContextValue.onInfo?.("file", file.id)}
					>
						info {file.name}
					</button>
					<button
						type="button"
						onClick={() => fileBrowserContextValue.onDelete?.("file", file.id)}
					>
						delete {file.name}
					</button>
				</div>
			))}
			<button type="button" onClick={() => onInfoPanelOpenChange(false)}>
				close info
			</button>
		</div>
	),
}));

vi.mock("@/components/files/TagManagerDialog", () => ({
	TagManagerDialog: () => null,
}));

vi.mock("@/components/files/TagLibraryManagerDialog", () => ({
	TagLibraryManagerDialog: () => null,
}));

vi.mock("@/pages/file-browser/FileBrowserDialogs", () => ({
	FileBrowserDialogs: ({
		onFolderPolicyClose,
	}: {
		onFolderPolicyClose: () => void;
	}) => (
		<button type="button" onClick={onFolderPolicyClose}>
			close folder policy
		</button>
	),
}));

vi.mock("@/components/files/preview/navigation/imagePreviewNavigation", () => ({
	getImagePreviewNavigation: () => ({}),
}));

function fileItem(id: number, name: string): FileListItem {
	return {
		compound_extension: null,
		extension: name.split(".").pop() ?? "",
		file_category: "image",
		is_locked: false,
		is_shared: false,
		mime_type: "image/jpeg",
		name,
		size: 1024,
		tags: [],
		updated_at: "2026-06-08T00:00:00Z",
		id,
	};
}

describe("CategoryBrowserPage", () => {
	beforeEach(() => {
		mockState.beginLocalStorageDeleteMutation.mockReset();
		mockState.batchActionOptions = null;
		mockState.beginLocalStorageDeleteMutation.mockReturnValue({
			rollback: vi.fn(),
		});
		mockState.clearSelection.mockReset();
		mockState.deleteFile.mockReset();
		mockState.deleteFile.mockResolvedValue(undefined);
		mockState.downloadPath.mockReset();
		mockState.downloadPath.mockReturnValue("/files/1/download");
		mockState.getFile.mockReset();
		mockState.handleApiError.mockReset();
		mockState.loadPreviewApps.mockReset();
		mockState.navigate.mockReset();
		mockState.params.category = "photo";
		mockState.previewAppsLoaded = true;
		mockState.search.mockReset();
		mockState.search.mockResolvedValue({
			files: [fileItem(1, "photo.jpg")],
			folders: [],
			total_files: 2,
			total_folders: 0,
		});
		mockState.setPageTitle.mockReset();
		mockState.selectItems.mockReset();
		mockState.streamArchiveDownload.mockReset();
		mockState.workspace = { kind: "personal" };
		useDownloadStore.setState({ pendingSelection: null, tasks: [] });
		useFrontendConfigStore.setState({
			archiveDownloadUserEnabled: true,
			isLoaded: true,
		});
		useUploadAreaControlsStore.getState().setUploadPanelPresence({
			open: false,
			visible: false,
		});
	});

	it("gates multi-file archive selection while preserving single-file download", async () => {
		useFrontendConfigStore.setState({
			archiveDownloadUserEnabled: false,
			isLoaded: true,
		});
		render(<CategoryBrowserPage />);
		await screen.findByText("photo.jpg");

		expect(mockState.batchActionOptions?.onArchiveDownload).toBeUndefined();
		mockState.batchActionOptions?.onDownload(1, "photo.jpg");
		expect(useDownloadStore.getState().pendingSelection).toEqual({
			workspace: { kind: "personal" },
			files: [{ id: 1, name: "photo.jpg", size: 1024 }],
			folders: [],
		});
	});

	it("maps enabled category archive selections with authoritative file sizes", async () => {
		mockState.search.mockResolvedValueOnce({
			files: [fileItem(1, "photo.jpg"), fileItem(2, "cover.jpg")],
			folders: [],
			total_files: 2,
			total_folders: 0,
		});
		render(<CategoryBrowserPage />);
		await screen.findByText("cover.jpg");

		mockState.batchActionOptions?.onArchiveDownload?.([1, 2], []);
		expect(useDownloadStore.getState().pendingSelection).toEqual({
			workspace: { kind: "personal" },
			files: [
				{ id: 1, name: "photo.jpg", size: 1024 },
				{ id: 2, name: "cover.jpg", size: 1024 },
			],
			folders: [],
		});
	});

	it("falls back to backend archive download when selected file ids expired", async () => {
		render(<CategoryBrowserPage />);
		await screen.findByText("photo.jpg");

		await mockState.batchActionOptions?.onArchiveDownload?.([1, 99], []);

		expect(mockState.streamArchiveDownload).toHaveBeenCalledWith([1, 99], []);
		expect(useDownloadStore.getState().pendingSelection).toBeNull();
	});

	it("loads image category files without copy or move actions", async () => {
		render(<CategoryBrowserPage />);

		await waitFor(() => {
			expect(mockState.search).toHaveBeenCalledWith({
				type: "file",
				category: "image",
				sort_by: "name",
				sort_order: "asc",
				limit: 100,
				offset: 0,
			});
		});

		expect(await screen.findByText("photo.jpg")).toBeInTheDocument();
		expect(screen.getByTestId("toolbar")).toHaveAttribute(
			"data-selection",
			"true",
		);
		expect(screen.getByTestId("workspace")).toHaveAttribute(
			"data-bottom-overlay",
			"selection-compact",
		);
		expect(screen.getByTestId("toolbar")).toHaveAttribute(
			"data-current-folder-actions",
			"refresh-only",
		);
		expect(screen.getByTestId("workspace")).toHaveAttribute(
			"data-current-folder-actions",
			"refresh-only",
		);
		expect(screen.getByTestId("workspace")).toHaveAttribute(
			"data-copy",
			"false",
		);
		expect(screen.getByTestId("workspace")).toHaveAttribute(
			"data-move",
			"false",
		);
		expect(screen.getByTestId("workspace")).toHaveAttribute(
			"data-location",
			"true",
		);
		expect(screen.getByTestId("workspace")).toHaveAttribute(
			"data-suppress-load-more",
			"false",
		);
		expect(screen.getByTestId("workspace")).toHaveAttribute(
			"data-has-more",
			"true",
		);
		expect(screen.getByTestId("workspace")).toHaveAttribute(
			"data-empty-title",
			"search:category_view_empty:search:category_image",
		);
		expect(screen.getByTestId("workspace")).toHaveAttribute(
			"data-empty-description",
			"search:category_view_empty_desc:search:category_image",
		);
		expect(mockState.clearSelection).toHaveBeenCalledTimes(1);
	});

	it("uses category-specific empty copy when no files match", async () => {
		mockState.search.mockResolvedValue({
			files: [],
			folders: [],
			total_files: 0,
			total_folders: 0,
		});

		render(<CategoryBrowserPage />);

		await waitFor(() => {
			expect(screen.getByTestId("workspace")).toHaveAttribute(
				"data-loading",
				"false",
			);
		});

		expect(screen.getByTestId("workspace")).toHaveAttribute(
			"data-empty-title",
			"search:category_view_empty:search:category_image",
		);
		expect(screen.getByTestId("workspace")).toHaveAttribute(
			"data-empty-description",
			"search:category_view_empty_desc:search:category_image",
		);
	});

	it("keeps the unused folder policy close callback harmless", () => {
		render(<CategoryBrowserPage />);

		fireEvent.click(
			screen.getByRole("button", { name: "close folder policy" }),
		);

		expect(
			screen.getByRole("button", { name: "close folder policy" }),
		).toBeInTheDocument();
	});

	it("selects all visible category files with Command+A", async () => {
		render(<CategoryBrowserPage />);

		await screen.findByText("photo.jpg");
		await waitFor(() => {
			expect(screen.getByTestId("workspace")).toHaveAttribute(
				"data-loading",
				"false",
			);
		});

		fireEvent.keyDown(document, {
			cancelable: true,
			key: "a",
			metaKey: true,
		});

		expect(mockState.selectItems).toHaveBeenCalledWith([1], []);
	});

	it("navigates to a result file location from the category context action", async () => {
		mockState.getFile.mockResolvedValue({ folder_id: 42 });

		render(<CategoryBrowserPage />);

		fireEvent.click(await screen.findByRole("button", { name: "photo.jpg" }));

		await waitFor(() => {
			expect(mockState.getFile).toHaveBeenCalledWith(1);
		});
		expect(mockState.navigate).toHaveBeenCalledWith("/folder/42", {
			viewTransition: false,
		});
	});

	it("opens the file info panel from category file actions", async () => {
		render(<CategoryBrowserPage />);

		fireEvent.click(await screen.findByText("info photo.jpg"));

		expect(screen.getByTestId("workspace")).toHaveAttribute(
			"data-info-open",
			"true",
		);
		expect(screen.getByTestId("workspace")).toHaveAttribute(
			"data-info-target",
			"photo.jpg",
		);

		fireEvent.click(screen.getByText("close info"));

		expect(screen.getByTestId("workspace")).toHaveAttribute(
			"data-info-open",
			"false",
		);
	});

	it("redirects unknown category routes to the workspace root", () => {
		mockState.params.category = "unknown";

		render(<CategoryBrowserPage />);

		expect(screen.getByTestId("navigate")).toHaveTextContent("/");
		expect(mockState.search).not.toHaveBeenCalled();
	});

	it("clears stale selection when the category route changes", async () => {
		const { rerender } = render(<CategoryBrowserPage />);
		await screen.findByText("photo.jpg");
		expect(mockState.clearSelection).toHaveBeenCalledTimes(1);

		mockState.params.category = "video";
		rerender(<CategoryBrowserPage />);

		expect(mockState.clearSelection).toHaveBeenCalledTimes(2);
	});

	it("reloads category results after file tag storage events", async () => {
		const { publishStorageChange } = await import("@/lib/storageChangeBus");
		render(<CategoryBrowserPage />);

		await waitFor(() => {
			expect(mockState.search).toHaveBeenCalledTimes(1);
		});

		publishStorageChange({
			affected_parent_ids: [7],
			affects_quota: false,
			at: "2026-06-10T00:00:00Z",
			file_ids: [1],
			folder_ids: [],
			kind: "tag.assignment_changed",
			root_affected: false,
			storage_delta: null,
			workspace: { kind: "personal" },
		});

		await waitFor(() => {
			expect(mockState.search).toHaveBeenCalledTimes(2);
		});
	});

	it("ignores folder-only tag events on category results", async () => {
		const { publishStorageChange } = await import("@/lib/storageChangeBus");
		render(<CategoryBrowserPage />);

		await waitFor(() => {
			expect(mockState.search).toHaveBeenCalledTimes(1);
		});

		publishStorageChange({
			affected_parent_ids: [7],
			affects_quota: false,
			at: "2026-06-10T00:00:00Z",
			file_ids: [],
			folder_ids: [9],
			kind: "tag.updated",
			root_affected: false,
			storage_delta: null,
			workspace: { kind: "personal" },
		});

		expect(mockState.search).toHaveBeenCalledTimes(1);
	});

	it("records local delete mutations for category file results", async () => {
		render(<CategoryBrowserPage />);

		fireEvent.click(
			await screen.findByRole("button", { name: "delete photo.jpg" }),
		);

		await waitFor(() => {
			expect(mockState.deleteFile).toHaveBeenCalledWith(1);
		});
		expect(mockState.beginLocalStorageDeleteMutation).toHaveBeenCalledWith({
			workspace: { kind: "personal" },
			fileIds: [1],
		});
	});

	it("rolls back local delete mutation records when category deletion fails", async () => {
		const rollback = vi.fn();
		const failure = new Error("delete failed");
		mockState.beginLocalStorageDeleteMutation.mockReturnValue({ rollback });
		mockState.deleteFile.mockRejectedValueOnce(failure);

		render(<CategoryBrowserPage />);

		fireEvent.click(
			await screen.findByRole("button", { name: "delete photo.jpg" }),
		);

		await waitFor(() => {
			expect(mockState.deleteFile).toHaveBeenCalledWith(1);
		});
		expect(rollback).toHaveBeenCalledTimes(1);
		expect(mockState.handleApiError).toHaveBeenCalledWith(failure);
	});
});
