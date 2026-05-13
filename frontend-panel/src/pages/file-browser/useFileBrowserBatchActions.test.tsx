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
	formatBatchToast: vi.fn(),
	handleApiError: vi.fn(),
	moveToFolder: vi.fn(),
	refresh: vi.fn(),
	refreshUser: vi.fn(),
	selectItems: vi.fn(),
	selectedFileIds: new Set<number>(),
	selectedFolderIds: new Set<number>(),
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
		onConfirm: (targetFolderId: number | null) => Promise<void>;
		open: boolean;
	}) =>
		open ? (
			<button type="button" onClick={() => void onConfirm(99)}>
				{`confirm-target:${mode}`}
			</button>
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
	},
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
	useWorkspaceStore: {
		getState: () => ({ workspace: { kind: "personal" } }),
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
	onArchiveDownload,
	onDownload,
}: {
	onArchiveDownload?: (fileIds: number[], folderIds: number[]) => void;
	onDownload?: (fileId: number, fileName: string) => void;
} = {}) {
	const { dialogs, selectionToolbar } = useFileBrowserBatchActions({
		displayFiles: [
			{ id: 1, name: "alpha.txt" },
			{ id: 2, name: "beta.txt" },
		] as never,
		displayFolders: [{ id: 5, name: "Docs" }] as never,
		onArchiveDownload,
		onDownload,
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
					<button type="button" onClick={selectionToolbar.onDelete}>
						delete-selected
					</button>
					<button type="button" onClick={selectionToolbar.onCopy}>
						copy-selected
					</button>
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
		mockState.formatBatchToast.mockReset();
		mockState.handleApiError.mockReset();
		mockState.moveToFolder.mockReset();
		mockState.refresh.mockReset();
		mockState.refreshUser.mockReset();
		mockState.selectItems.mockReset();
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

		fireEvent.click(screen.getByText("download:archive"));

		await waitFor(() => {
			expect(onArchiveDownload).toHaveBeenCalledWith([1, 2], [5]);
		});
		expect(onDownload).not.toHaveBeenCalled();
		expect(mockState.clearSelection).toHaveBeenCalledTimes(1);
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
				kind: "file.deleted",
				workspace: { kind: "personal" },
				file_ids: [1, 2],
				folder_ids: [],
				affected_parent_ids: [7],
				root_affected: false,
				at: "2026-05-13T00:00:00Z",
			}),
		).toBe(true);
		expect(
			consumeStorageEventEcho({
				kind: "folder.deleted",
				workspace: { kind: "personal" },
				file_ids: [],
				folder_ids: [5],
				affected_parent_ids: [7],
				root_affected: false,
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
		expect(mockState.clearSelection).toHaveBeenCalledTimes(1);
		expect(mockState.refresh).toHaveBeenCalledTimes(1);
	});
});
