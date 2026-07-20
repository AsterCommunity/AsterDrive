import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import SearchBrowserPage from "@/pages/SearchBrowserPage";
import { useDownloadStore } from "@/stores/downloadStore";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import { useUploadAreaControlsStore } from "@/stores/uploadAreaControlsStore";
import type { FileListItem, FolderListItem } from "@/types/api";

const mockState = vi.hoisted(() => ({
	beginLocalStorageDeleteMutation: vi.fn(),
	batchActionOptions: null as null | {
		onArchiveDownload?: (fileIds: number[], folderIds: number[]) => void;
		onDownload: (fileId: number, fileName: string) => void;
	},
	clearSelection: vi.fn(),
	deleteFile: vi.fn(),
	deleteFolder: vi.fn(),
	downloadPath: vi.fn(),
	getFile: vi.fn(),
	handleApiError: vi.fn(),
	loadPreviewApps: vi.fn(),
	navigate: vi.fn(),
	search: vi.fn(),
	searchParams: new URLSearchParams("q=report&type=all"),
	setPageTitle: vi.fn(),
	setSortBy: vi.fn(),
	setSortOrder: vi.fn(),
	setViewMode: vi.fn(),
	selectItems: vi.fn(),
	streamArchiveDownload: vi.fn(),
	previewAppsLoaded: true,
	thumbnailSupport: null,
	workspace: { kind: "personal" as const },
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("react-router-dom", () => ({
	useNavigate: () => mockState.navigate,
	useSearchParams: () => [mockState.searchParams, vi.fn()],
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
				count: 2,
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
		deleteFolder: mockState.deleteFolder,
		downloadPath: mockState.downloadPath,
		getFile: mockState.getFile,
		setFileLock: vi.fn(),
		setFolderLock: vi.fn(),
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
		currentFolderActions,
		searchQuery,
		selectionToolbar,
	}: {
		currentFolderActions?: "full" | "refresh-only";
		searchQuery: string | null;
		selectionToolbar: unknown;
	}) => (
		<div
			data-testid="toolbar"
			data-current-folder-actions={currentFolderActions ?? "full"}
			data-search-query={searchQuery ?? ""}
			data-selection={String(Boolean(selectionToolbar))}
		/>
	),
}));

vi.mock("@/pages/file-browser/FileBrowserWorkspace", () => ({
	FileBrowserWorkspace: ({
		currentFolderActions,
		fileBrowserContextValue,
		hasMoreFiles,
		bottomOverlayOffset,
	}: {
		currentFolderActions?: "full" | "refresh-only";
		fileBrowserContextValue: {
			files: FileListItem[];
			folders: FolderListItem[];
			onCopy?: unknown;
			onDelete?: (type: "file" | "folder", id: number) => void;
			onFolderOpen: (id: number, name: string) => void;
			onGoToLocation?: (file: FileListItem) => void;
			onMove?: unknown;
		};
		hasMoreFiles: boolean;
		bottomOverlayOffset?: string;
	}) => (
		<div
			data-testid="workspace"
			data-current-folder-actions={currentFolderActions ?? "full"}
			data-copy={String(Boolean(fileBrowserContextValue.onCopy))}
			data-has-more={String(hasMoreFiles)}
			data-bottom-overlay={bottomOverlayOffset ?? "none"}
			data-location={String(Boolean(fileBrowserContextValue.onGoToLocation))}
			data-move={String(Boolean(fileBrowserContextValue.onMove))}
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
						onClick={() => fileBrowserContextValue.onDelete?.("file", file.id)}
					>
						delete {file.name}
					</button>
				</div>
			))}
			{fileBrowserContextValue.folders.map((folder) => (
				<div key={folder.id}>
					<button
						type="button"
						onClick={() =>
							fileBrowserContextValue.onFolderOpen(folder.id, folder.name)
						}
					>
						{folder.name}
					</button>
					<button
						type="button"
						onClick={() =>
							fileBrowserContextValue.onDelete?.("folder", folder.id)
						}
					>
						delete {folder.name}
					</button>
				</div>
			))}
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
		file_category: "document",
		is_locked: false,
		is_shared: false,
		mime_type: "text/plain",
		name,
		size: 1024,
		tags: [],
		updated_at: "2026-06-08T00:00:00Z",
		id,
	};
}

function folderItem(id: number, name: string): FolderListItem {
	return {
		id,
		is_locked: false,
		is_shared: false,
		name,
		tags: [],
		updated_at: "2026-06-08T00:00:00Z",
	};
}

describe("SearchBrowserPage", () => {
	beforeEach(() => {
		mockState.beginLocalStorageDeleteMutation.mockReset();
		mockState.batchActionOptions = null;
		mockState.beginLocalStorageDeleteMutation.mockReturnValue({
			rollback: vi.fn(),
		});
		mockState.clearSelection.mockReset();
		mockState.deleteFile.mockReset();
		mockState.deleteFile.mockResolvedValue(undefined);
		mockState.deleteFolder.mockReset();
		mockState.deleteFolder.mockResolvedValue(undefined);
		mockState.downloadPath.mockReset();
		mockState.downloadPath.mockReturnValue("/files/1/download");
		mockState.getFile.mockReset();
		mockState.handleApiError.mockReset();
		mockState.loadPreviewApps.mockReset();
		mockState.navigate.mockReset();
		mockState.search.mockReset();
		mockState.search.mockResolvedValue({
			files: [fileItem(1, "report.txt")],
			folders: [folderItem(2, "Reports")],
			total_files: 2,
			total_folders: 1,
		});
		mockState.searchParams = new URLSearchParams("q=report&type=all");
		mockState.setPageTitle.mockReset();
		mockState.selectItems.mockReset();
		mockState.streamArchiveDownload.mockReset();
		mockState.workspace = { kind: "personal" };
		useDownloadStore.setState({ tasks: [] });
		useFrontendConfigStore.setState({
			archiveDownloadUserEnabled: true,
			isLoaded: true,
		});
		useUploadAreaControlsStore.getState().setUploadPanelPresence({
			open: false,
			visible: false,
		});
	});

	it("gates search archive downloads while preserving direct single-file selection", async () => {
		useFrontendConfigStore.setState({
			archiveDownloadUserEnabled: false,
			isLoaded: true,
		});
		render(<SearchBrowserPage />);
		await screen.findByText("report.txt");

		expect(mockState.batchActionOptions?.onArchiveDownload).toBeUndefined();
		mockState.batchActionOptions?.onDownload(1, "report.txt");
		expect(useDownloadStore.getState().pendingSelection).toEqual({
			workspace: { kind: "personal" },
			files: [{ id: 1, name: "report.txt", size: 1024 }],
			folders: [],
		});
	});

	it("maps enabled search archive selections across files and folders", async () => {
		render(<SearchBrowserPage />);
		await screen.findByText("report.txt");

		mockState.batchActionOptions?.onArchiveDownload?.([1], [2]);
		expect(useDownloadStore.getState().pendingSelection).toEqual({
			workspace: { kind: "personal" },
			files: [{ id: 1, name: "report.txt", size: 1024 }],
			folders: [{ id: 2, name: "Reports" }],
		});
	});

	it("loads search results through the file-browser surface", async () => {
		render(<SearchBrowserPage />);

		await waitFor(() => {
			expect(mockState.search).toHaveBeenCalledWith({
				q: "report",
				type: "all",
				sort_by: "name",
				sort_order: "asc",
				limit: 100,
				offset: 0,
			});
		});

		expect(await screen.findByText("report.txt")).toBeInTheDocument();
		expect(screen.getByText("Reports")).toBeInTheDocument();
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
			"data-has-more",
			"true",
		);
	});

	it("keeps the unused folder policy close callback harmless", () => {
		render(<SearchBrowserPage />);

		fireEvent.click(
			screen.getByRole("button", { name: "close folder policy" }),
		);

		expect(
			screen.getByRole("button", { name: "close folder policy" }),
		).toBeInTheDocument();
	});

	it("selects all visible search results with Command+A", async () => {
		render(<SearchBrowserPage />);

		await screen.findByText("report.txt");

		fireEvent.keyDown(document, {
			cancelable: true,
			key: "a",
			metaKey: true,
		});

		expect(mockState.selectItems).toHaveBeenCalledWith([1], [2]);
	});

	it("can go to a result file location and open folder results", async () => {
		mockState.getFile.mockResolvedValue({ folder_id: 42 });

		render(<SearchBrowserPage />);

		fireEvent.click(await screen.findByRole("button", { name: "report.txt" }));
		await waitFor(() => {
			expect(mockState.getFile).toHaveBeenCalledWith(1);
		});
		expect(mockState.navigate).toHaveBeenCalledWith("/folder/42", {
			viewTransition: false,
		});

		fireEvent.click(screen.getByRole("button", { name: "Reports" }));
		expect(mockState.navigate).toHaveBeenCalledWith("/folder/2?name=Reports", {
			viewTransition: false,
		});
	});

	it("reloads virtual search results after tag storage events", async () => {
		const { publishStorageChange } = await import("@/lib/storageChangeBus");
		render(<SearchBrowserPage />);

		await waitFor(() => {
			expect(mockState.search).toHaveBeenCalledTimes(1);
		});

		publishStorageChange({
			affected_parent_ids: [7],
			affects_quota: false,
			at: "2026-06-10T00:00:00Z",
			file_ids: [1],
			folder_ids: [],
			kind: "tag.updated",
			root_affected: false,
			storage_delta: null,
			workspace: { kind: "personal" },
		});

		await waitFor(() => {
			expect(mockState.search).toHaveBeenCalledTimes(2);
		});
	});

	it("ignores tag creation events that are not bound to any result", async () => {
		const { publishStorageChange } = await import("@/lib/storageChangeBus");
		render(<SearchBrowserPage />);

		await waitFor(() => {
			expect(mockState.search).toHaveBeenCalledTimes(1);
		});

		publishStorageChange({
			affected_parent_ids: [],
			affects_quota: false,
			at: "2026-06-10T00:00:00Z",
			file_ids: [],
			folder_ids: [],
			kind: "tag.created",
			root_affected: false,
			storage_delta: null,
			workspace: { kind: "personal" },
		});

		expect(mockState.search).toHaveBeenCalledTimes(1);
	});

	it("records local delete mutations for file search results", async () => {
		render(<SearchBrowserPage />);

		fireEvent.click(
			await screen.findByRole("button", { name: "delete report.txt" }),
		);

		await waitFor(() => {
			expect(mockState.deleteFile).toHaveBeenCalledWith(1);
		});
		expect(mockState.beginLocalStorageDeleteMutation).toHaveBeenCalledWith({
			workspace: { kind: "personal" },
			fileIds: [1],
			folderIds: [],
		});
	});

	it("rolls back local delete mutation records when folder deletion fails", async () => {
		const rollback = vi.fn();
		const failure = new Error("delete failed");
		mockState.beginLocalStorageDeleteMutation.mockReturnValue({ rollback });
		mockState.deleteFolder.mockRejectedValueOnce(failure);

		render(<SearchBrowserPage />);

		fireEvent.click(
			await screen.findByRole("button", { name: "delete Reports" }),
		);

		await waitFor(() => {
			expect(mockState.deleteFolder).toHaveBeenCalledWith(2);
		});
		expect(mockState.beginLocalStorageDeleteMutation).toHaveBeenCalledWith({
			workspace: { kind: "personal" },
			fileIds: [],
			folderIds: [2],
		});
		expect(rollback).toHaveBeenCalledTimes(1);
		expect(mockState.handleApiError).toHaveBeenCalledWith(failure);
	});
});
