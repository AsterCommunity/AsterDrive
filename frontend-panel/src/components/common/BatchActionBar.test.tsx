import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { BatchActionBar } from "@/components/common/BatchActionBar";
import {
	clearStorageEventEchoes,
	consumeStorageEventEcho,
} from "@/lib/storageEventEcho";

const mockState = vi.hoisted(() => ({
	archiveCompress: vi.fn(),
	archiveDownload: vi.fn(),
	batchCopy: vi.fn(),
	batchDelete: vi.fn(),
	clearSelection: vi.fn(),
	copyToWorkspace: vi.fn(),
	currentWorkspace: { kind: "personal" as const },
	formatBatchToast: vi.fn(),
	handleApiError: vi.fn(),
	moveToFolder: vi.fn(),
	refresh: vi.fn(),
	refreshUser: vi.fn(),
	selectedFileIds: new Set<number>(),
	selectedFolderIds: new Set<number>(),
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) => {
			const normalizedKey = key.replace(/^core:/, "");
			if (normalizedKey === "selected_count") {
				return `selected:${options?.count}`;
			}
			if (normalizedKey === "batch_delete_confirm_title") {
				return `delete-title:${options?.count}`;
			}
			return normalizedKey;
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
		open,
		onConfirm,
		title,
		description,
		confirmLabel,
	}: {
		open: boolean;
		onConfirm: () => void;
		title: string;
		description: string;
		confirmLabel: string;
	}) =>
		open ? (
			<div>
				<div>{title}</div>
				<div>{description}</div>
				<button type="button" onClick={onConfirm}>
					{confirmLabel}
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/files/BatchTargetFolderDialog", () => ({
	BatchTargetFolderDialog: ({
		mode,
		open,
		onConfirm,
	}: {
		mode: "copy" | "move";
		open: boolean;
		onConfirm: (selection: {
			workspace: { kind: "personal" } | { kind: "team"; teamId: number };
			folderId: number | null;
		}) => void;
	}) =>
		open ? (
			<div>
				<div>{`target-dialog:${mode}`}</div>
				<button
					type="button"
					onClick={() =>
						onConfirm({
							workspace: mockState.currentWorkspace,
							folderId: 99,
						})
					}
				>
					confirm-target
				</button>
				<button
					type="button"
					onClick={() =>
						onConfirm({
							workspace: { kind: "team", teamId: 9 },
							folderId: 12,
						})
					}
				>
					confirm-team-target
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
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
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{`icon:${name}`}</span>,
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
	},
	resolveCopyDispatch: ({
		currentWorkspace,
		fileIds,
		folderIds,
		targetFolderId,
		targetWorkspace,
	}: {
		currentWorkspace: { kind: "personal" } | { kind: "team"; teamId: number };
		fileIds: number[];
		folderIds: number[];
		targetFolderId: number | null;
		targetWorkspace: { kind: "personal" } | { kind: "team"; teamId: number };
	}) => {
		const sameWorkspace =
			currentWorkspace.kind === targetWorkspace.kind &&
			(currentWorkspace.kind !== "team" ||
				(currentWorkspace.kind === "team" &&
					targetWorkspace.kind === "team" &&
					currentWorkspace.teamId === targetWorkspace.teamId));
		return sameWorkspace
			? mockState.batchCopy(fileIds, folderIds, targetFolderId)
			: mockState.copyToWorkspace(
					targetWorkspace,
					fileIds,
					folderIds,
					targetFolderId,
				);
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
	useFileStore: (
		selector: (state: {
			breadcrumb: Array<{ id: number | null; name: string }>;
			clearSelection: typeof mockState.clearSelection;
			currentFolderId: number | null;
			moveToFolder: typeof mockState.moveToFolder;
			refresh: typeof mockState.refresh;
			selectedFileIds: Set<number>;
			selectedFolderIds: Set<number>;
		}) => unknown,
	) =>
		selector({
			breadcrumb: [{ id: null, name: "Root" }],
			clearSelection: mockState.clearSelection,
			currentFolderId: 7,
			moveToFolder: mockState.moveToFolder,
			refresh: mockState.refresh,
			selectedFileIds: mockState.selectedFileIds,
			selectedFolderIds: mockState.selectedFolderIds,
		}),
}));

vi.mock("@/stores/workspaceStore", () => ({
	useWorkspaceStore: {
		getState: () => ({ workspace: mockState.currentWorkspace }),
	},
}));

describe("BatchActionBar", () => {
	beforeEach(() => {
		mockState.archiveCompress.mockReset();
		mockState.archiveCompress.mockResolvedValue(undefined);
		mockState.archiveDownload.mockReset();
		mockState.archiveDownload.mockResolvedValue(undefined);
		mockState.batchCopy.mockReset();
		mockState.batchDelete.mockReset();
		mockState.clearSelection.mockReset();
		mockState.copyToWorkspace.mockReset();
		mockState.currentWorkspace = { kind: "personal" };
		mockState.formatBatchToast.mockReset();
		mockState.handleApiError.mockReset();
		mockState.moveToFolder.mockReset();
		mockState.refresh.mockReset();
		mockState.refreshUser.mockReset();
		mockState.toastError.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.selectedFileIds = new Set<number>([1, 2]);
		mockState.selectedFolderIds = new Set<number>([5]);

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
		mockState.moveToFolder.mockResolvedValue({
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

	it("does not render when nothing is selected", () => {
		mockState.selectedFileIds = new Set();
		mockState.selectedFolderIds = new Set();

		const { container } = render(<BatchActionBar />);

		expect(container).toBeEmptyDOMElement();
	});

	it("deletes selected items, shows the toast, clears selection, and refreshes", async () => {
		render(<BatchActionBar />);

		expect(screen.getByText("selected:3")).toBeInTheDocument();

		fireEvent.click(screen.getByText("delete"));
		expect(screen.getByText("delete-title:3")).toBeInTheDocument();
		fireEvent.click(screen.getAllByText("delete")[1]);

		await waitFor(() => {
			expect(mockState.batchDelete).toHaveBeenCalledWith([1, 2], [5]);
		});
		expect(mockState.formatBatchToast).toHaveBeenCalledWith(
			expect.any(Function),
			"delete",
			expect.objectContaining({ succeeded: 3 }),
		);
		expect(mockState.toastSuccess).toHaveBeenCalledWith("toast:title", {
			description: "done",
		});
		expect(mockState.clearSelection).toHaveBeenCalledTimes(1);
		await waitFor(() => {
			expect(mockState.refresh).toHaveBeenCalledTimes(1);
		});
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

	it("moves selected items to a target folder", async () => {
		render(<BatchActionBar />);

		fireEvent.click(screen.getByText("move_to"));

		expect(screen.getByText("target-dialog:move")).toBeInTheDocument();
		fireEvent.click(screen.getByText("confirm-target"));

		await waitFor(() => {
			expect(mockState.moveToFolder).toHaveBeenCalledWith([1, 2], [5], 99);
		});
		expect(mockState.formatBatchToast).toHaveBeenCalledWith(
			expect.any(Function),
			"move",
			expect.objectContaining({ succeeded: 3 }),
		);
		expect(mockState.toastSuccess).toHaveBeenCalledWith("toast:title", {
			description: "done",
		});
		expect(mockState.clearSelection).not.toHaveBeenCalled();
	});

	it("copies selected items, clears selection, and refreshes afterwards", async () => {
		render(<BatchActionBar />);

		fireEvent.click(screen.getByText("copy_to"));

		expect(screen.getByText("target-dialog:copy")).toBeInTheDocument();
		fireEvent.click(screen.getByText("confirm-target"));

		await waitFor(() => {
			expect(mockState.batchCopy).toHaveBeenCalledWith([1, 2], [5], 99);
			expect(mockState.copyToWorkspace).not.toHaveBeenCalled();
		});
		expect(mockState.clearSelection).toHaveBeenCalledTimes(1);
		expect(mockState.refresh).toHaveBeenCalledTimes(1);
	});

	it("copies selected items to another workspace through workspace transfer", async () => {
		render(<BatchActionBar />);

		fireEvent.click(screen.getByText("copy_to"));
		fireEvent.click(screen.getByText("confirm-team-target"));

		await waitFor(() => {
			expect(mockState.copyToWorkspace).toHaveBeenCalledWith(
				{ kind: "team", teamId: 9 },
				[1, 2],
				[5],
				12,
			);
		});
		expect(mockState.batchCopy).not.toHaveBeenCalled();
		expect(mockState.clearSelection).toHaveBeenCalledTimes(1);
		expect(mockState.refresh).toHaveBeenCalledTimes(1);
	});

	it("routes service failures through handleApiError", async () => {
		const error = new Error("delete failed");
		mockState.batchDelete.mockRejectedValueOnce(error);

		render(<BatchActionBar />);

		fireEvent.click(screen.getByText("delete"));
		fireEvent.click(screen.getAllByText("delete")[1]);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
	});

	it("creates an archive download task and clears the selection after success", async () => {
		render(<BatchActionBar onArchiveDownload={mockState.archiveDownload} />);

		fireEvent.click(screen.getByText("tasks:archive_download_action"));

		await waitFor(() => {
			expect(mockState.archiveDownload).toHaveBeenCalledWith([1, 2], [5]);
		});
		expect(mockState.clearSelection).toHaveBeenCalledTimes(1);
		expect(mockState.handleApiError).not.toHaveBeenCalled();
	});

	it("delegates archive compress without clearing the selection immediately", async () => {
		render(<BatchActionBar onArchiveCompress={mockState.archiveCompress} />);

		fireEvent.click(screen.getByText("tasks:archive_compress_action"));

		await waitFor(() => {
			expect(mockState.archiveCompress).toHaveBeenCalledWith([1, 2], [5]);
		});
		expect(mockState.clearSelection).not.toHaveBeenCalled();
		expect(mockState.handleApiError).not.toHaveBeenCalled();
	});
});
