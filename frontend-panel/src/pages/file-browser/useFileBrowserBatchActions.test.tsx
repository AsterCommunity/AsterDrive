import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	clearStorageEventEchoes,
	consumeStorageEventEcho,
} from "@/lib/storageEventEcho";
import { useFileBrowserBatchActions } from "@/pages/file-browser/useFileBrowserBatchActions";

const mockState = vi.hoisted(() => ({
	batchCopy: vi.fn(),
	batchDelete: vi.fn(),
	clearSelection: vi.fn(),
	copyToWorkspace: vi.fn(),
	currentWorkspace: { kind: "personal" as const },
	formatBatchToast: vi.fn(),
	handleApiError: vi.fn(),
	moveToFolder: vi.fn(),
	moveToWorkspace: vi.fn(),
	refresh: vi.fn(),
	refreshUser: vi.fn(),
	selectItems: vi.fn(),
	selectedFileIds: new Set<number>(),
	selectedFolderIds: new Set<number>(),
	singleCopyFile: vi.fn(),
	singleCopyFolder: vi.fn(),
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) => {
			if (key === "batch_delete_confirm_title") {
				return `delete-title:${options?.count}`;
			}
			return key;
		},
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		error: (...args: unknown[]) => mockState.toastError(...args),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/common/ConfirmDialog", () => ({
	ConfirmDialog: ({
		confirmLabel,
		onConfirm,
		open,
		title,
	}: {
		confirmLabel: string;
		onConfirm: () => void;
		open: boolean;
		title: string;
	}) =>
		open ? (
			<div>
				<div>{title}</div>
				<button type="button" onClick={onConfirm}>
					{confirmLabel}
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/files/BatchTargetFolderDialog", () => ({
	BatchTargetFolderDialog: ({
		mode,
		onConfirm,
		open,
	}: {
		mode: "move" | "copy";
		onConfirm: (selection: {
			workspace: { kind: "personal" } | { kind: "team"; teamId: number };
			folderId: number | null;
		}) => Promise<void>;
		open: boolean;
	}) =>
		open ? (
			<div>
				<button
					type="button"
					onClick={() =>
						void onConfirm({
							workspace: mockState.currentWorkspace,
							folderId: 99,
						})
					}
				>
					{`confirm-target:${mode}`}
				</button>
				<button
					type="button"
					onClick={() =>
						void onConfirm({
							workspace: { kind: "team", teamId: 11 },
							folderId: 22,
						})
					}
				>
					{`confirm-team-target:${mode}`}
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/files/TagManagerDialog", () => ({
	TagManagerDialog: ({
		onOpenChange,
		open,
		target,
	}: {
		open: boolean;
		onOpenChange: (open: boolean) => void;
		target: {
			count?: number;
			fileIds?: number[];
			folderIds?: number[];
			onChanged?: () => Promise<void> | void;
		} | null;
	}) =>
		open ? (
			<div data-testid="tag-manager-dialog">
				{`tag-target:${target?.count}:${target?.fileIds?.join(",")}:${target?.folderIds?.join(",")}`}
				<button type="button" onClick={() => onOpenChange(false)}>
					close-tag-manager
				</button>
				<button type="button" onClick={() => void target?.onChanged?.()}>
					refresh-tag-manager
				</button>
			</div>
		) : null,
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/lib/formatBatchToast", () => ({
	formatBatchToast: (...args: unknown[]) => mockState.formatBatchToast(...args),
}));

vi.mock("@/services/batchService", () => ({
	batchService: {
		batchCopy: (...args: unknown[]) => mockState.batchCopy(...args),
		batchDelete: (...args: unknown[]) => mockState.batchDelete(...args),
		copyToWorkspace: (...args: unknown[]) => mockState.copyToWorkspace(...args),
		moveToWorkspace: (...args: unknown[]) => mockState.moveToWorkspace(...args),
	},
	createBatchService: () => ({
		batchMove: (...args: unknown[]) => mockState.moveToFolder(...args),
		moveToWorkspace: (...args: unknown[]) => mockState.moveToWorkspace(...args),
	}),
	singleOperationResult: (promise: Promise<unknown>) =>
		promise.then(() => ({ succeeded: 1, failed: 0, errors: [] })),
	resolveCopyDispatch: ({
		currentWorkspace,
		fileIds,
		folderIds,
		targetFolderId,
		targetWorkspace,
		dispatcher,
	}: {
		currentWorkspace: { kind: "personal" } | { kind: "team"; teamId: number };
		fileIds: number[];
		folderIds: number[];
		targetFolderId: number | null;
		targetWorkspace: { kind: "personal" } | { kind: "team"; teamId: number };
		dispatcher: {
			singleFileCopy: (...args: unknown[]) => unknown;
			singleFolderCopy: (...args: unknown[]) => unknown;
		};
	}) => {
		const sameWorkspace =
			currentWorkspace.kind === targetWorkspace.kind &&
			(currentWorkspace.kind !== "team" ||
				(currentWorkspace.kind === "team" &&
					targetWorkspace.kind === "team" &&
					currentWorkspace.teamId === targetWorkspace.teamId));
		if (!sameWorkspace) {
			return mockState.copyToWorkspace(
				targetWorkspace,
				fileIds,
				folderIds,
				targetFolderId,
			);
		}
		if (fileIds.length === 1 && folderIds.length === 0) {
			return dispatcher.singleFileCopy(fileIds[0], targetFolderId);
		}
		if (folderIds.length === 1 && fileIds.length === 0) {
			return dispatcher.singleFolderCopy(folderIds[0], targetFolderId);
		}
		return mockState.batchCopy(fileIds, folderIds, targetFolderId);
	},
	resolveMoveDispatch: ({
		currentWorkspace,
		targetWorkspace,
		fileIds,
		folderIds,
		targetFolderId,
		dispatcher,
	}: {
		currentWorkspace: { kind: "personal" } | { kind: "team"; teamId: number };
		targetWorkspace: { kind: "personal" } | { kind: "team"; teamId: number };
		fileIds: number[];
		folderIds: number[];
		targetFolderId: number | null;
		dispatcher: {
			batchMove: (...args: unknown[]) => unknown;
			singleFileMove: (...args: unknown[]) => unknown;
			singleFolderMove: (...args: unknown[]) => unknown;
			moveToWorkspace: (...args: unknown[]) => unknown;
		};
	}) =>
		currentWorkspace.kind === targetWorkspace.kind &&
		(currentWorkspace.kind !== "team" ||
			currentWorkspace.teamId === targetWorkspace.teamId)
			? fileIds.length === 1 && folderIds.length === 0
				? dispatcher.singleFileMove(fileIds[0], targetFolderId)
				: folderIds.length === 1 && fileIds.length === 0
					? dispatcher.singleFolderMove(folderIds[0], targetFolderId)
					: dispatcher.batchMove(fileIds, folderIds, targetFolderId)
			: dispatcher.moveToWorkspace(
					targetWorkspace,
					fileIds,
					folderIds,
					targetFolderId,
				),
}));

vi.mock("@/services/fileService", () => ({
	createFileService: () => ({
		copyFile: (...args: unknown[]) => mockState.singleCopyFile(...args),
		copyFolder: (...args: unknown[]) => mockState.singleCopyFolder(...args),
		moveFile: (...args: unknown[]) => mockState.singleCopyFile(...args),
		moveFolder: (...args: unknown[]) => mockState.singleCopyFolder(...args),
	}),
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: {
		getState: () => ({
			refreshUser: mockState.refreshUser,
		}),
	},
}));

vi.mock("@/stores/fileStore", () => ({
	useFileStore: (selector: (state: typeof mockStore) => unknown) =>
		selector(mockStore),
}));

vi.mock("@/stores/workspaceStore", () => ({
	bindWorkspaceService: (
		factory: (workspace: { kind: "personal" }) => unknown,
	) => factory({ kind: "personal" }),
	useWorkspaceStore: {
		getState: () => ({ workspace: mockState.currentWorkspace }),
	},
}));

const mockStore = {
	breadcrumb: [{ id: null, name: "Root" }],
	clearSelection: mockState.clearSelection,
	currentFolderId: 7,
	moveToFolder: mockState.moveToFolder,
	refresh: mockState.refresh,
	selectItems: mockState.selectItems,
	selectedFileIds: mockState.selectedFileIds,
	selectedFolderIds: mockState.selectedFolderIds,
};

function Harness({
	allowCopyMove,
	allowDelete,
	allowTagManagement,
	onArchiveCompress,
	onArchiveDownload,
	onDownload,
	onMoveToFolder,
}: {
	allowCopyMove?: boolean;
	allowDelete?: boolean;
	allowTagManagement?: boolean;
	onArchiveCompress?: (fileIds: number[], folderIds: number[]) => void;
	onArchiveDownload?: (fileIds: number[], folderIds: number[]) => void;
	onDownload?: (fileId: number, fileName: string) => void;
	onMoveToFolder?: (
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number | null,
	) => Promise<unknown> | unknown;
} = {}) {
	const { dialogs, selectionToolbar } = useFileBrowserBatchActions({
		allowCopyMove,
		allowDelete,
		allowTagManagement,
		displayFiles: [
			{ id: 1, name: "alpha.txt" },
			{ id: 2, name: "beta.txt" },
		] as never,
		displayFolders: [{ id: 5, name: "Docs" }] as never,
		onArchiveCompress,
		onArchiveDownload,
		onDownload,
		onMoveToFolder: onMoveToFolder as never,
	});

	return (
		<div>
			{selectionToolbar ? (
				<div>
					<div>{`count:${selectionToolbar.count}`}</div>
					<button
						type="button"
						onClick={selectionToolbar.onToggleDisplayedSelection}
					>
						toggle-shown
					</button>
					{selectionToolbar.onDelete ? (
						<button type="button" onClick={selectionToolbar.onDelete}>
							delete-selected
						</button>
					) : null}
					{selectionToolbar.onCopy ? (
						<button type="button" onClick={selectionToolbar.onCopy}>
							copy-selected
						</button>
					) : null}
					{selectionToolbar.onMove ? (
						<button type="button" onClick={selectionToolbar.onMove}>
							move-selected
						</button>
					) : null}
					{selectionToolbar.onManageTags ? (
						<button type="button" onClick={selectionToolbar.onManageTags}>
							manage-tags
						</button>
					) : null}
					{selectionToolbar.onArchiveCompress ? (
						<button type="button" onClick={selectionToolbar.onArchiveCompress}>
							compress-selected
						</button>
					) : null}
					{selectionToolbar.downloadAction ? (
						<button
							type="button"
							onClick={selectionToolbar.downloadAction.onClick}
						>
							{`download:${selectionToolbar.downloadAction.kind}`}
						</button>
					) : null}
				</div>
			) : (
				<div>no-toolbar</div>
			)}
			{dialogs}
		</div>
	);
}

describe("useFileBrowserBatchActions", () => {
	beforeEach(() => {
		mockState.batchCopy.mockReset();
		mockState.batchDelete.mockReset();
		mockState.clearSelection.mockReset();
		mockState.copyToWorkspace.mockReset();
		mockState.currentWorkspace = { kind: "personal" };
		mockState.formatBatchToast.mockReset();
		mockState.handleApiError.mockReset();
		mockState.moveToFolder.mockReset();
		mockState.moveToWorkspace.mockReset();
		mockState.refresh.mockReset();
		mockState.refreshUser.mockReset();
		mockState.selectItems.mockReset();
		mockState.singleCopyFile.mockReset();
		mockState.singleCopyFolder.mockReset();
		mockState.toastError.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.selectedFileIds = new Set<number>();
		mockState.selectedFolderIds = new Set<number>();
		mockStore.selectedFileIds = mockState.selectedFileIds;
		mockStore.selectedFolderIds = mockState.selectedFolderIds;
		mockState.batchCopy.mockResolvedValue({
			errors: [],
			failed: 0,
			succeeded: 3,
		});
		mockState.batchDelete.mockResolvedValue({
			errors: [],
			failed: 0,
			succeeded: 3,
		});
		mockState.copyToWorkspace.mockResolvedValue({
			errors: [],
			failed: 0,
			succeeded: 3,
		});
		mockState.moveToWorkspace.mockResolvedValue({
			errors: [],
			failed: 0,
			succeeded: 3,
		});
		mockState.singleCopyFile.mockResolvedValue({ id: 1 });
		mockState.singleCopyFolder.mockResolvedValue({ id: 5 });
		mockState.refresh.mockResolvedValue(undefined);
		mockState.refreshUser.mockResolvedValue(undefined);
		mockState.formatBatchToast.mockReturnValue({
			description: "done",
			title: "toast:title",
			variant: "success",
		});
		clearStorageEventEchoes();
	});

	it("does not expose selection actions when nothing is selected", () => {
		render(<Harness />);

		expect(screen.getByText("no-toolbar")).toBeInTheDocument();
	});

	it("selects the displayed file and folder ids from the toolbar state", () => {
		mockState.selectedFileIds = new Set([1]);
		mockStore.selectedFileIds = mockState.selectedFileIds;

		render(<Harness />);

		fireEvent.click(screen.getByText("toggle-shown"));

		expect(mockState.selectItems).toHaveBeenCalledWith([1, 2], [5]);
	});

	it("uses the regular file download action for a single selected file", () => {
		const onArchiveDownload = vi.fn();
		const onDownload = vi.fn();
		mockState.selectedFileIds = new Set([1]);
		mockStore.selectedFileIds = mockState.selectedFileIds;

		render(
			<Harness onArchiveDownload={onArchiveDownload} onDownload={onDownload} />,
		);

		fireEvent.click(screen.getByText("download:file"));

		expect(onDownload).toHaveBeenCalledWith(1, "alpha.txt");
		expect(onArchiveDownload).not.toHaveBeenCalled();
		expect(mockState.clearSelection).toHaveBeenCalledTimes(1);
	});

	it("can hide copy and move actions for category-style views", () => {
		mockState.selectedFileIds = new Set([1]);
		mockStore.selectedFileIds = mockState.selectedFileIds;

		render(<Harness allowCopyMove={false} />);

		expect(screen.getByText("count:1")).toBeInTheDocument();
		expect(screen.queryByText("copy-selected")).not.toBeInTheDocument();
		expect(screen.queryByText("move-selected")).not.toBeInTheDocument();
		expect(screen.getByText("manage-tags")).toBeInTheDocument();
		expect(screen.getByText("delete-selected")).toBeInTheDocument();
	});

	it("can expose download-only selection actions for read-only views", () => {
		const onArchiveDownload = vi.fn();
		const onDownload = vi.fn();
		mockState.selectedFileIds = new Set([1, 2]);
		mockStore.selectedFileIds = mockState.selectedFileIds;

		render(
			<Harness
				allowCopyMove={false}
				allowDelete={false}
				allowTagManagement={false}
				onArchiveDownload={onArchiveDownload}
				onDownload={onDownload}
			/>,
		);

		expect(screen.getByText("download:selection")).toBeInTheDocument();
		expect(screen.queryByText("copy-selected")).not.toBeInTheDocument();
		expect(screen.queryByText("move-selected")).not.toBeInTheDocument();
		expect(screen.queryByText("manage-tags")).not.toBeInTheDocument();
		expect(screen.queryByText("delete-selected")).not.toBeInTheDocument();
	});

	it("updates tag management actions when the permission flag changes", () => {
		mockState.selectedFileIds = new Set([1]);
		mockStore.selectedFileIds = mockState.selectedFileIds;

		const { rerender } = render(<Harness allowTagManagement={false} />);

		expect(screen.queryByText("manage-tags")).not.toBeInTheDocument();

		rerender(<Harness allowTagManagement />);

		expect(screen.getByText("manage-tags")).toBeInTheDocument();
	});

	it("keeps archive download for multiple selected items", async () => {
		const onArchiveDownload = vi.fn().mockResolvedValue(undefined);
		const onDownload = vi.fn();
		mockState.selectedFileIds = new Set([1, 2]);
		mockState.selectedFolderIds = new Set([5]);
		mockStore.selectedFileIds = mockState.selectedFileIds;
		mockStore.selectedFolderIds = mockState.selectedFolderIds;

		render(
			<Harness onArchiveDownload={onArchiveDownload} onDownload={onDownload} />,
		);

		fireEvent.click(screen.getByText("download:selection"));

		await waitFor(() => {
			expect(onArchiveDownload).toHaveBeenCalledWith([1, 2], [5]);
		});
		expect(onDownload).not.toHaveBeenCalled();
		expect(mockState.clearSelection).toHaveBeenCalledTimes(1);
	});

	it("routes archive action failures through handleApiError", async () => {
		const downloadError = new Error("download failed");
		const compressError = new Error("compress failed");
		const onArchiveDownload = vi.fn().mockRejectedValue(downloadError);
		const onArchiveCompress = vi.fn().mockRejectedValue(compressError);
		mockState.selectedFileIds = new Set([1, 2]);
		mockStore.selectedFileIds = mockState.selectedFileIds;

		render(
			<Harness
				onArchiveCompress={onArchiveCompress}
				onArchiveDownload={onArchiveDownload}
			/>,
		);

		fireEvent.click(screen.getByText("download:selection"));
		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(downloadError);
		});
		expect(mockState.clearSelection).not.toHaveBeenCalled();

		fireEvent.click(screen.getByText("compress-selected"));
		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(compressError);
		});
	});

	it("deletes selected items after confirmation", async () => {
		mockState.selectedFileIds = new Set([1, 2]);
		mockState.selectedFolderIds = new Set([5]);
		mockStore.selectedFileIds = mockState.selectedFileIds;
		mockStore.selectedFolderIds = mockState.selectedFolderIds;

		render(<Harness />);

		expect(screen.getByText("count:3")).toBeInTheDocument();
		fireEvent.click(screen.getByText("delete-selected"));
		expect(screen.getByText("delete-title:3")).toBeInTheDocument();
		fireEvent.click(screen.getByText("core:delete"));

		await waitFor(() => {
			expect(mockState.batchDelete).toHaveBeenCalledWith([1, 2], [5]);
		});
		expect(mockState.clearSelection).toHaveBeenCalledTimes(1);
		expect(mockState.refresh).toHaveBeenCalledTimes(1);
		expect(mockState.refreshUser).not.toHaveBeenCalled();
		expect(
			consumeStorageEventEcho({
				kind: "file.trashed",
				workspace: { kind: "personal" },
				file_ids: [1, 2],
				folder_ids: [],
				affected_parent_ids: [7],
				root_affected: false,
				affects_quota: false,
				storage_delta: null,
				at: "2026-05-13T00:00:00Z",
			}),
		).toBe(true);
		expect(
			consumeStorageEventEcho({
				kind: "folder.trashed",
				workspace: { kind: "personal" },
				file_ids: [],
				folder_ids: [5],
				affected_parent_ids: [7],
				root_affected: false,
				affects_quota: false,
				storage_delta: null,
				at: "2026-05-13T00:00:00Z",
			}),
		).toBe(true);
	});

	it("copies selected items to the chosen target folder", async () => {
		mockState.selectedFileIds = new Set([1, 2]);
		mockState.selectedFolderIds = new Set([5]);
		mockStore.selectedFileIds = mockState.selectedFileIds;
		mockStore.selectedFolderIds = mockState.selectedFolderIds;

		render(<Harness />);

		fireEvent.click(screen.getByText("copy-selected"));
		fireEvent.click(screen.getByText("confirm-target:copy"));

		await waitFor(() => {
			expect(mockState.batchCopy).toHaveBeenCalledWith([1, 2], [5], 99);
		});
		expect(mockState.copyToWorkspace).not.toHaveBeenCalled();
		expect(mockState.clearSelection).toHaveBeenCalledTimes(1);
		expect(mockState.refresh).toHaveBeenCalledTimes(1);
	});

	it("copies selected items to a different workspace through workspace transfer", async () => {
		mockState.selectedFileIds = new Set([1, 2]);
		mockState.selectedFolderIds = new Set([5]);
		mockStore.selectedFileIds = mockState.selectedFileIds;
		mockStore.selectedFolderIds = mockState.selectedFolderIds;

		render(<Harness />);

		fireEvent.click(screen.getByText("copy-selected"));
		fireEvent.click(screen.getByText("confirm-team-target:copy"));

		await waitFor(() => {
			expect(mockState.copyToWorkspace).toHaveBeenCalledWith(
				{ kind: "team", teamId: 11 },
				[1, 2],
				[5],
				22,
			);
		});
		expect(mockState.batchCopy).not.toHaveBeenCalled();
		expect(mockState.clearSelection).toHaveBeenCalledTimes(1);
		expect(mockState.refresh).toHaveBeenCalledTimes(1);
	});

	it("moves selected items to a different workspace through workspace transfer", async () => {
		mockState.selectedFileIds = new Set([1, 2]);
		mockState.selectedFolderIds = new Set([5]);
		mockStore.selectedFileIds = mockState.selectedFileIds;
		mockStore.selectedFolderIds = mockState.selectedFolderIds;

		render(<Harness />);

		fireEvent.click(screen.getByText("move-selected"));
		fireEvent.click(screen.getByText("confirm-team-target:move"));

		await waitFor(() => {
			expect(mockState.moveToWorkspace).toHaveBeenCalledWith(
				{ kind: "team", teamId: 11 },
				[1, 2],
				[5],
				22,
			);
		});
		expect(mockState.moveToFolder).not.toHaveBeenCalled();
		expect(mockState.clearSelection).toHaveBeenCalledTimes(1);
	});

	it("copies one folder through the single-folder endpoint instead of batch copy", async () => {
		mockState.selectedFileIds = new Set();
		mockState.selectedFolderIds = new Set([5]);
		mockStore.selectedFileIds = mockState.selectedFileIds;
		mockStore.selectedFolderIds = mockState.selectedFolderIds;

		render(<Harness />);

		fireEvent.click(screen.getByText("copy-selected"));
		fireEvent.click(screen.getByText("confirm-target:copy"));

		await waitFor(() => {
			expect(mockState.singleCopyFolder).toHaveBeenCalledWith(5, 99);
		});
		expect(mockState.batchCopy).not.toHaveBeenCalled();
	});

	it("moves selected items through a custom handler and handles target failures", async () => {
		const moveError = new Error("move failed");
		const onMoveToFolder = vi
			.fn()
			.mockResolvedValueOnce({ errors: [], failed: 0, succeeded: 3 })
			.mockRejectedValueOnce(moveError);
		mockState.selectedFileIds = new Set([1, 2]);
		mockState.selectedFolderIds = new Set([5]);
		mockStore.selectedFileIds = mockState.selectedFileIds;
		mockStore.selectedFolderIds = mockState.selectedFolderIds;

		render(<Harness onMoveToFolder={onMoveToFolder} />);

		fireEvent.click(screen.getByText("move-selected"));
		fireEvent.click(screen.getByText("confirm-target:move"));

		await waitFor(() => {
			expect(onMoveToFolder).toHaveBeenCalledWith([1, 2], [5], 99);
		});
		expect(mockState.clearSelection).toHaveBeenCalledTimes(1);
		expect(mockState.refresh).toHaveBeenCalledTimes(1);

		fireEvent.click(screen.getByText("move-selected"));
		fireEvent.click(screen.getByText("confirm-target:move"));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(moveError);
		});
	});

	it("opens the batch tag manager for selected file and folder ids", () => {
		mockState.selectedFileIds = new Set([1, 2]);
		mockState.selectedFolderIds = new Set([5]);
		mockStore.selectedFileIds = mockState.selectedFileIds;
		mockStore.selectedFolderIds = mockState.selectedFolderIds;

		render(<Harness />);

		fireEvent.click(screen.getByText("manage-tags"));

		expect(screen.getByTestId("tag-manager-dialog")).toHaveTextContent(
			"tag-target:3:1,2:5",
		);
	});

	it("refreshes from the tag manager and closes it when the selection clears", async () => {
		mockState.selectedFileIds = new Set([1, 2]);
		mockState.selectedFolderIds = new Set([5]);
		mockStore.selectedFileIds = mockState.selectedFileIds;
		mockStore.selectedFolderIds = mockState.selectedFolderIds;

		const { rerender } = render(<Harness />);

		fireEvent.click(screen.getByText("manage-tags"));
		fireEvent.click(screen.getByText("refresh-tag-manager"));

		await waitFor(() => {
			expect(mockState.refresh).toHaveBeenCalledTimes(1);
		});

		mockState.selectedFileIds = new Set();
		mockState.selectedFolderIds = new Set();
		mockStore.selectedFileIds = mockState.selectedFileIds;
		mockStore.selectedFolderIds = mockState.selectedFolderIds;
		rerender(<Harness />);

		expect(screen.queryByTestId("tag-manager-dialog")).not.toBeInTheDocument();
		expect(screen.getByText("no-toolbar")).toBeInTheDocument();
	});
});
