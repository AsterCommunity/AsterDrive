import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	FOLDER_LIMIT,
	FOLDER_TREE_DRAG_EXPAND_DELAY_MS,
} from "@/lib/constants";

const mockState = vi.hoisted(() => ({
	getInvalidInternalDropReason: vi.fn(),
	handleApiError: vi.fn(),
	hasInternalDragData: vi.fn(),
	listFolder: vi.fn(),
	listRoot: vi.fn(),
	navigate: vi.fn(),
	readInternalDragData: vi.fn(),
	setInternalDragPreview: vi.fn(),
	auth: {
		user: {
			id: 7,
		},
	},
	fileStore: {
		breadcrumb: [{ id: null, name: "Root" }] as Array<{
			id: number | null;
			name: string;
		}>,
		currentFolderId: null as number | null,
		folders: [] as Array<{ id: number; name: string }>,
		lastFolderContents: null as {
			folderId: number | null;
			folders: Array<{ id: number; name: string }>;
			sortBy: "name" | "size" | "created_at" | "updated_at" | "type";
			sortOrder: "asc" | "desc";
			workspaceRevision: number;
		} | null,
		loading: false,
		moveToFolder: vi.fn(),
		sortBy: "name" as "name" | "size" | "created_at" | "updated_at" | "type",
		sortOrder: "asc" as "asc" | "desc",
		workspaceRequestRevision: 0,
	},
	pathname: "/",
	workspace: { kind: "personal" } as
		| { kind: "personal" }
		| { kind: "team"; teamId: number },
	writeInternalDragData: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("react-router-dom", () => ({
	useLocation: () => ({
		pathname: mockState.pathname,
	}),
	useNavigate: () => mockState.navigate,
}));

vi.mock("@/components/common/SkeletonTree", () => ({
	SkeletonTree: ({ count }: { count: number }) => (
		<div>{`skeleton:${count}`}</div>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
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

vi.mock("@/services/fileService", () => ({
	fileService: {
		listFolder: (...args: unknown[]) => mockState.listFolder(...args),
		listRoot: (...args: unknown[]) => mockState.listRoot(...args),
	},
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: <T,>(selector: (state: typeof mockState.auth) => T) =>
		selector(mockState.auth),
}));

vi.mock("@/stores/fileStore", () => {
	const useFileStore = Object.assign(
		<T,>(selector: (state: typeof mockState.fileStore) => T) =>
			selector(mockState.fileStore),
		{
			getState: () => mockState.fileStore,
		},
	);

	return { useFileStore };
});

vi.mock("@/stores/workspaceStore", () => ({
	useWorkspaceStore: <T,>(
		selector: (state: { workspace: typeof mockState.workspace }) => T,
	) => selector(mockState),
}));

function createFolder(id: number, name: string) {
	return {
		created_at: "2026-03-28T00:00:00Z",
		id,
		is_locked: false,
		name,
		updated_at: "2026-03-28T00:00:00Z",
	};
}

function getFolderRow(name: string) {
	const row = screen.getByText(name).closest("[data-folder-tree-row]");
	if (!row) {
		throw new Error(`${name} row not found`);
	}
	return row;
}

interface FolderTreeProps {
	onMoveToFolder?: (
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number | null,
	) => Promise<void> | void;
}

async function renderTree(props?: FolderTreeProps) {
	const { FolderTree } = await import("@/components/folders/FolderTree");
	return render(<FolderTree {...props} />);
}

describe("FolderTree", () => {
	beforeEach(() => {
		vi.resetModules();
		vi.useRealTimers();

		mockState.getInvalidInternalDropReason.mockReset();
		mockState.handleApiError.mockReset();
		mockState.hasInternalDragData.mockReset();
		mockState.listFolder.mockReset();
		mockState.listRoot.mockReset();
		mockState.navigate.mockReset();
		mockState.readInternalDragData.mockReset();
		mockState.setInternalDragPreview.mockReset();
		mockState.writeInternalDragData.mockReset();
		mockState.fileStore.moveToFolder.mockReset();

		mockState.auth.user = { id: 7 };
		mockState.fileStore.breadcrumb = [{ id: null, name: "Root" }];
		mockState.fileStore.currentFolderId = null;
		mockState.fileStore.folders = [];
		mockState.fileStore.lastFolderContents = null;
		mockState.fileStore.loading = false;
		mockState.fileStore.moveToFolder.mockResolvedValue(undefined);
		mockState.fileStore.sortBy = "name";
		mockState.fileStore.sortOrder = "asc";
		mockState.fileStore.workspaceRequestRevision = 0;
		mockState.pathname = "/";
		mockState.workspace = { kind: "personal" };

		mockState.getInvalidInternalDropReason.mockReturnValue(null);
		mockState.hasInternalDragData.mockReturnValue(false);
		mockState.listFolder.mockResolvedValue({ folders: [] });
		mockState.listRoot.mockResolvedValue({ folders: [] });
		mockState.readInternalDragData.mockReturnValue(null);
	});

	it("shows a skeleton while the root folder list is pending", async () => {
		mockState.listRoot.mockImplementationOnce(
			() => new Promise(() => undefined),
		);

		await renderTree();

		expect(screen.getByText("skeleton:4")).toBeInTheDocument();
	});

	it("loads root folders and reuses the cached snapshot on remount", async () => {
		mockState.pathname = "/folder/99";
		mockState.fileStore.folders = [
			createFolder(1, "Alpha"),
			createFolder(2, "Beta"),
		];
		mockState.listRoot.mockResolvedValue({
			folders: [createFolder(1, "Alpha"), createFolder(2, "Beta")],
		});

		const firstView = await renderTree();

		expect(await screen.findByText("Alpha")).toBeInTheDocument();
		expect(screen.getByText("Beta")).toBeInTheDocument();
		expect(mockState.listRoot).toHaveBeenCalledWith({
			file_limit: 0,
			folder_limit: FOLDER_LIMIT,
			sort_by: "name",
			sort_order: "asc",
		});
		expect(mockState.listRoot).toHaveBeenCalledTimes(1);

		firstView.unmount();

		await renderTree();

		expect(screen.getByText("Alpha")).toBeInTheDocument();
		expect(screen.getByText("Beta")).toBeInTheDocument();
		expect(screen.queryByText("skeleton:4")).not.toBeInTheDocument();
		expect(mockState.listRoot).toHaveBeenCalledTimes(1);
	});

	it("reuses the root file listing while the file page is loading", async () => {
		mockState.fileStore.loading = true;
		mockState.listRoot.mockResolvedValue({
			folders: [createFolder(9, "Duplicate Request")],
		});

		const { FolderTree } = await import("@/components/folders/FolderTree");
		const view = render(<FolderTree />);

		expect(screen.getByText("skeleton:4")).toBeInTheDocument();
		expect(mockState.listRoot).not.toHaveBeenCalled();

		mockState.fileStore.loading = false;
		mockState.fileStore.folders = [createFolder(1, "Alpha")];
		mockState.fileStore.lastFolderContents = {
			folderId: null,
			folders: [createFolder(1, "Alpha")],
			sortBy: "name",
			sortOrder: "asc",
			workspaceRevision: 0,
		};
		view.rerender(<FolderTree />);

		expect(await screen.findByText("Alpha")).toBeInTheDocument();
		expect(mockState.listRoot).not.toHaveBeenCalled();
		expect(screen.queryByText("Duplicate Request")).not.toBeInTheDocument();
	});

	it("loads the root list on the sidebar route when no file page request is active", async () => {
		mockState.pathname = "/folder/99";
		mockState.fileStore.folders = [createFolder(1, "Alpha")];
		mockState.listRoot.mockResolvedValue({
			folders: [createFolder(1, "Alpha")],
		});

		await renderTree();

		expect(await screen.findByText("Alpha")).toBeInTheDocument();
		expect(mockState.listRoot).toHaveBeenCalledTimes(1);
	});

	it("collapses and expands the root folder list without navigating", async () => {
		mockState.fileStore.folders = [
			createFolder(1, "Alpha"),
			createFolder(2, "Beta"),
		];
		mockState.fileStore.lastFolderContents = {
			folderId: null,
			folders: [createFolder(1, "Alpha"), createFolder(2, "Beta")],
			sortBy: "name",
			sortOrder: "asc",
			workspaceRevision: 0,
		};

		await renderTree();

		expect(await screen.findByText("Alpha")).toBeInTheDocument();
		const rootRow = screen.getByRole("button", { name: /root/i });
		expect(rootRow).toHaveAttribute("aria-expanded", "true");

		const collapseButton = screen.getByRole("button", {
			name: "collapse_tree",
		});

		fireEvent.keyDown(collapseButton, { key: "Enter" });

		expect(mockState.navigate).not.toHaveBeenCalled();
		expect(rootRow).toHaveAttribute("aria-expanded", "true");
		expect(screen.getByText("Alpha").closest("[aria-hidden]")).toHaveAttribute(
			"aria-hidden",
			"false",
		);

		fireEvent.click(collapseButton);

		expect(mockState.navigate).not.toHaveBeenCalled();
		expect(rootRow).toHaveAttribute("aria-expanded", "false");
		expect(screen.getByText("Alpha")).toBeInTheDocument();
		expect(screen.getByText("Alpha").closest("[aria-hidden]")).toHaveAttribute(
			"aria-hidden",
			"true",
		);

		const expandButton = screen.getByRole("button", { name: "expand_tree" });

		fireEvent.keyDown(expandButton, { key: " " });

		expect(mockState.navigate).not.toHaveBeenCalled();
		expect(rootRow).toHaveAttribute("aria-expanded", "false");
		expect(screen.getByText("Alpha").closest("[aria-hidden]")).toHaveAttribute(
			"aria-hidden",
			"true",
		);

		fireEvent.click(expandButton);

		expect(rootRow).toHaveAttribute("aria-expanded", "true");
		expect(screen.getByText("Alpha")).toBeInTheDocument();
		expect(screen.getByText("Beta")).toBeInTheDocument();
		expect(mockState.navigate).not.toHaveBeenCalled();
	});

	it("uses the current file sorting preferences for folder requests", async () => {
		mockState.pathname = "/folder/99";
		mockState.fileStore.sortBy = "updated_at";
		mockState.fileStore.sortOrder = "desc";
		mockState.listRoot.mockResolvedValue({
			folders: [createFolder(2, "Beta"), createFolder(1, "Alpha")],
		});

		await renderTree();

		await screen.findByText("Beta");
		expect(mockState.listRoot).toHaveBeenCalledWith({
			file_limit: 0,
			folder_limit: FOLDER_LIMIT,
			sort_by: "updated_at",
			sort_order: "desc",
		});
	});

	it("loads children while navigating by click and keyboard", async () => {
		mockState.fileStore.folders = [createFolder(1, "Alpha Root")];
		mockState.fileStore.lastFolderContents = {
			folderId: null,
			folders: [createFolder(1, "Alpha Root")],
			sortBy: "name",
			sortOrder: "asc",
			workspaceRevision: 0,
		};
		mockState.listFolder.mockImplementation(async (id: number) => {
			if (id === 1) {
				return {
					folders: [createFolder(2, "Project Space")],
				};
			}

			return { folders: [] };
		});

		await renderTree();

		await screen.findByText("Alpha Root");

		fireEvent.click(screen.getByRole("button", { name: "Alpha Root" }));

		await waitFor(() => {
			expect(mockState.listFolder).toHaveBeenCalledWith(1, {
				file_limit: 0,
				folder_limit: FOLDER_LIMIT,
				sort_by: "name",
				sort_order: "asc",
			});
		});
		expect(mockState.navigate).toHaveBeenCalledWith(
			"/folder/1?name=Alpha%20Root",
		);
		expect(await screen.findByText("Project Space")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "Project Space" }));

		await waitFor(() => {
			expect(mockState.listFolder).toHaveBeenCalledWith(2, {
				file_limit: 0,
				folder_limit: FOLDER_LIMIT,
				sort_by: "name",
				sort_order: "asc",
			});
		});
		expect(mockState.navigate).toHaveBeenCalledWith(
			"/folder/2?name=Project%20Space",
		);
	});

	it("accepts valid drops on the root target and ignores invalid ones", async () => {
		const onMoveToFolder = vi.fn();
		const dataTransfer = {
			dropEffect: "copy",
			types: ["application/x-asterdrive-move"],
		} as unknown as DataTransfer;

		mockState.fileStore.folders = [createFolder(1, "Alpha")];
		mockState.fileStore.lastFolderContents = {
			folderId: null,
			folders: [createFolder(1, "Alpha")],
			sortBy: "name",
			sortOrder: "asc",
			workspaceRevision: 0,
		};
		mockState.hasInternalDragData.mockReturnValue(true);
		mockState.readInternalDragData.mockReturnValue({
			fileIds: [9],
			folderIds: [8],
		});

		await renderTree({ onMoveToFolder });

		const rootButton = await screen.findByRole("button", { name: /root/i });

		fireEvent.dragOver(rootButton, { dataTransfer });

		expect(dataTransfer.dropEffect).toBe("move");
		expect(rootButton.closest("[data-folder-tree-root-row]")).toHaveClass(
			"ring-2",
		);

		fireEvent.dragLeave(rootButton, { dataTransfer });

		expect(rootButton.closest("[data-folder-tree-root-row]")).not.toHaveClass(
			"ring-2",
		);

		fireEvent.drop(rootButton, { dataTransfer });

		expect(mockState.getInvalidInternalDropReason).toHaveBeenCalledWith(
			{ fileIds: [9], folderIds: [8] },
			null,
			[],
		);
		expect(onMoveToFolder).toHaveBeenCalledWith([9], [8], null);

		mockState.getInvalidInternalDropReason.mockReturnValueOnce("descendant");
		fireEvent.drop(rootButton, { dataTransfer });
		expect(onMoveToFolder).toHaveBeenCalledTimes(1);
	});

	it("ignores root drag events without internal drag data", async () => {
		const dataTransfer = {
			dropEffect: "copy",
			types: [],
		} as unknown as DataTransfer;

		mockState.fileStore.folders = [createFolder(1, "Alpha")];
		mockState.fileStore.lastFolderContents = {
			folderId: null,
			folders: [createFolder(1, "Alpha")],
			sortBy: "name",
			sortOrder: "asc",
			workspaceRevision: 0,
		};
		mockState.hasInternalDragData.mockReturnValue(false);

		await renderTree();

		const rootButton = await screen.findByRole("button", { name: /root/i });
		fireEvent.dragOver(rootButton, { dataTransfer });

		expect(dataTransfer.dropEffect).toBe("copy");
		expect(rootButton.closest("[data-folder-tree-root-row]")).not.toHaveClass(
			"ring-2",
		);
	});

	it("cancels pending hover expansion when drag leaves the folder row", async () => {
		const dataTransfer = {
			dropEffect: "copy",
			types: ["application/x-asterdrive-move"],
		} as unknown as DataTransfer;

		mockState.fileStore.folders = [createFolder(1, "Alpha")];
		mockState.fileStore.lastFolderContents = {
			folderId: null,
			folders: [createFolder(1, "Alpha")],
			sortBy: "name",
			sortOrder: "asc",
			workspaceRevision: 0,
		};
		mockState.listFolder.mockResolvedValue({
			folders: [createFolder(2, "Beta")],
		});
		mockState.hasInternalDragData.mockReturnValue(true);

		await renderTree();

		await screen.findByText("Alpha");
		const alphaRow = getFolderRow("Alpha");

		vi.useFakeTimers();
		fireEvent.dragOver(alphaRow, { dataTransfer });
		fireEvent.dragLeave(alphaRow, {
			dataTransfer,
			relatedTarget: document.body,
		});
		await vi.advanceTimersByTimeAsync(FOLDER_TREE_DRAG_EXPAND_DELAY_MS);
		vi.useRealTimers();

		expect(mockState.listFolder).not.toHaveBeenCalled();
		expect(screen.queryByText("Beta")).not.toBeInTheDocument();
	});

	it("resets cached folders when the workspace changes", async () => {
		mockState.pathname = "/folder/99";
		mockState.fileStore.folders = [createFolder(1, "Personal Folder")];
		mockState.listRoot.mockResolvedValueOnce({
			folders: [createFolder(1, "Personal Folder")],
		});

		const { FolderTree } = await import("@/components/folders/FolderTree");
		const view = render(<FolderTree />);

		expect(await screen.findByText("Personal Folder")).toBeInTheDocument();
		expect(mockState.listRoot).toHaveBeenCalledTimes(1);

		mockState.workspace = { kind: "team", teamId: 12 };
		mockState.pathname = "/teams/12";
		mockState.fileStore.folders = [createFolder(2, "Team Folder")];
		mockState.listRoot.mockResolvedValueOnce({
			folders: [createFolder(2, "Team Folder")],
		});
		view.rerender(<FolderTree />);

		expect(await screen.findByText("Team Folder")).toBeInTheDocument();
		expect(screen.queryByText("Personal Folder")).not.toBeInTheDocument();
	});

	it("expands a hovered folder after the drag delay and drops into it", async () => {
		const dataTransfer = {
			dropEffect: "copy",
			types: ["application/x-asterdrive-move"],
		} as unknown as DataTransfer;

		mockState.fileStore.folders = [createFolder(1, "Alpha")];
		mockState.fileStore.lastFolderContents = {
			folderId: null,
			folders: [createFolder(1, "Alpha")],
			sortBy: "name",
			sortOrder: "asc",
			workspaceRevision: 0,
		};
		mockState.listFolder.mockResolvedValue({
			folders: [createFolder(2, "Beta")],
		});
		mockState.hasInternalDragData.mockReturnValue(true);
		mockState.readInternalDragData.mockReturnValue({
			fileIds: [3],
			folderIds: [4],
		});

		await renderTree();

		await screen.findByText("Alpha");
		const alphaRow = getFolderRow("Alpha");

		vi.useFakeTimers();

		fireEvent.dragOver(alphaRow, { dataTransfer });

		expect(dataTransfer.dropEffect).toBe("move");
		await vi.advanceTimersByTimeAsync(FOLDER_TREE_DRAG_EXPAND_DELAY_MS);
		await Promise.resolve();
		await Promise.resolve();
		expect(mockState.listFolder).toHaveBeenCalledWith(1, {
			file_limit: 0,
			folder_limit: FOLDER_LIMIT,
			sort_by: "name",
			sort_order: "asc",
		});
		vi.useRealTimers();
		expect(await screen.findByText("Beta")).toBeInTheDocument();

		fireEvent.drop(alphaRow, { dataTransfer });

		expect(mockState.getInvalidInternalDropReason).toHaveBeenCalledWith(
			{ fileIds: [3], folderIds: [4] },
			1,
			[1],
		);
		expect(mockState.fileStore.moveToFolder).toHaveBeenCalledWith([3], [4], 1);

		mockState.getInvalidInternalDropReason.mockReturnValueOnce("self");
		fireEvent.drop(alphaRow, { dataTransfer });
		expect(mockState.fileStore.moveToFolder).toHaveBeenCalledTimes(1);
	});

	it("refreshes affected parents when a folder storage event is published", async () => {
		const { publishStorageChange } = await import("@/lib/storageChangeBus");
		mockState.fileStore.folders = [createFolder(1, "Alpha")];
		mockState.fileStore.lastFolderContents = {
			folderId: null,
			folders: [createFolder(1, "Alpha")],
			sortBy: "name",
			sortOrder: "asc",
			workspaceRevision: 0,
		};
		mockState.listFolder.mockImplementation(async (id: number) => {
			if (id === 1) {
				return {
					folders: [createFolder(2, "Child")],
				};
			}

			return { folders: [] };
		});

		await renderTree();

		await screen.findByText("Alpha");
		const alphaRow = getFolderRow("Alpha");

		const toggleButton = alphaRow.querySelector("button");
		if (!toggleButton) {
			throw new Error("Toggle button not found");
		}

		fireEvent.click(toggleButton);

		await screen.findByText("Child");

		mockState.listRoot.mockClear();
		mockState.listFolder.mockClear();

		publishStorageChange({
			affected_parent_ids: [1],
			affects_quota: false,
			at: "2026-06-23T00:00:00Z",
			file_ids: [],
			folder_ids: [2],
			kind: "folder.updated",
			root_affected: true,
			storage_delta: null,
			workspace: { kind: "personal" },
		});

		await waitFor(() => {
			expect(mockState.listRoot).toHaveBeenCalledWith({
				file_limit: 0,
				folder_limit: FOLDER_LIMIT,
				sort_by: "name",
				sort_order: "asc",
			});
		});
		expect(mockState.listFolder).toHaveBeenCalledWith(1, {
			file_limit: 0,
			folder_limit: FOLDER_LIMIT,
			sort_by: "name",
			sort_order: "asc",
		});
	});
});
