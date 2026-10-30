import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import type { ComponentProps, ReactNode } from "react";
import { MemoryRouter, useLocation } from "react-router-dom";
import { beforeEach, expect, it, vi } from "vitest";
import { useFileBrowserContext } from "@/components/files/FileBrowserContext";
import type { FileBrowserNavigationBar } from "@/components/files/FileBrowserNavigationBar";
import type { FileInfoDialog } from "@/components/files/FileInfoDialog";
import type { VersionHistoryDialog } from "@/components/files/VersionHistoryDialog";
import { useFileStore } from "@/stores/fileStore";
import type {
	AdminFolderInfo,
	AdminWorkspaceContents,
	FileInfo,
} from "@/types/api";
import { AdminWorkspaceBrowser } from "./AdminWorkspaceBrowser";

const mocks = vi.hoisted(() => ({
	list: vi.fn(),
	trash: vi.fn(),
	getFile: vi.fn(),
	getFolder: vi.fn(),
	listVersions: vi.fn(),
	download: vi.fn(),
	error: vi.fn(),
	end: vi.fn(),
	thumbnailPath: vi.fn(() => "/scoped-thumbnail"),
}));
vi.mock("@/services/adminContentAccessService", () => ({
	adminContentAccessService: mocks,
}));
vi.mock("@/services/adminContentDownload", () => ({
	startAdminContentDownload: mocks.download,
}));
vi.mock("@/hooks/useApiError", () => ({ handleApiError: mocks.error }));
vi.mock("react-i18next", async (importOriginal) => ({
	...(await importOriginal<typeof import("react-i18next")>()),
	useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("./AdminContentAccessGate", () => ({
	AdminContentAccessGate: ({
		children,
		shell,
	}: {
		children: (id: string, end: () => void) => ReactNode;
		shell: (children: ReactNode) => ReactNode;
	}) => (
		<>
			{shell(<span>gate-shell</span>)}
			{children("grant", mocks.end)}
		</>
	),
}));
vi.mock("@/components/layout/AppLayout", () => ({
	AppLayout: ({
		children,
		sidebarContent,
	}: {
		children: ReactNode;
		sidebarContent: ReactNode;
	}) => (
		<div>
			{sidebarContent}
			{children}
		</div>
	),
}));
vi.mock("./AdminWorkspaceSidebar", () => ({
	AdminWorkspaceSidebar: ({
		onTrash,
		onNavigate,
		onEnd,
	}: {
		onTrash?: () => void;
		onNavigate?: (id?: number) => void;
		onEnd?: () => void;
	}) => (
		<>
			<button type="button" onClick={onTrash}>
				trash
			</button>
			<button type="button" onClick={() => onNavigate?.()}>
				root
			</button>
			<button type="button" onClick={onEnd}>
				end
			</button>
		</>
	),
}));
function BrowserItems({ mode }: { mode: string }) {
	const context = useFileBrowserContext();
	const file = context.files[0];
	return (
		<div data-testid="items" data-mode={mode}>
			<span>{file?.name}</span>
			<span>{context.breadcrumbPathIds.join(",")}</span>
			<button type="button" onClick={() => context.onFolderOpen(19, "folder")}>
				open-folder
			</button>
			<button type="button" onClick={() => context.onInfo?.("folder", 19)}>
				folder-info
			</button>
			<button type="button" onClick={() => file && context.onFileClick(file)}>
				file-click
			</button>
			<button
				type="button"
				onClick={() => file && context.onInfo?.("file", file.id)}
			>
				file-info
			</button>
			<button
				type="button"
				onClick={() => file && context.onVersions?.(file.id)}
			>
				versions
			</button>
			<button
				type="button"
				onClick={() => file && context.onDownload(file.id, file.name)}
			>
				download
			</button>
			<button
				type="button"
				onClick={() => context.sorting?.onSort("size", "desc")}
			>
				table-sort
			</button>
			<span>{file && context.getThumbnailPath?.(file)}</span>
			<span>
				{file && context.getTrashMeta?.("file", file.id)?.originalPath}
			</span>
		</div>
	);
}
vi.mock("@/components/files/FileTable", () => ({
	FileTable: () => <BrowserItems mode="list" />,
}));
vi.mock("@/components/files/FileGrid", () => ({
	FileGrid: () => <BrowserItems mode="grid" />,
}));
vi.mock("@/components/files/FileBrowserNavigationBar", () => ({
	FileBrowserNavigationBar: ({
		onRefresh,
		onViewModeChange,
		breadcrumb,
		sorting,
	}: ComponentProps<typeof FileBrowserNavigationBar>) => (
		<>
			<button type="button" onClick={onRefresh}>
				refresh
			</button>
			<button type="button" onClick={() => onViewModeChange("grid")}>
				grid
			</button>
			<button type="button" onClick={() => breadcrumb.onNavigate(null)}>
				breadcrumb-root
			</button>
			<button type="button" onClick={() => sorting?.onSortBy("type")}>
				sort-by
			</button>
			<button type="button" onClick={() => sorting?.onSortOrder("desc")}>
				sort-order
			</button>
		</>
	),
}));
vi.mock("@/components/files/FileInfoDialog", () => ({
	FileInfoDialog: ({
		file,
		folder,
		actions,
		open,
		loading,
		onOpenChange,
	}: ComponentProps<typeof FileInfoDialog>) =>
		open ? (
			<section aria-label="details">
				<span>{loading ? "detail-loading" : (file?.name ?? folder?.name)}</span>
				{actions}
				<button type="button" onClick={() => onOpenChange(false)}>
					close-info
				</button>
			</section>
		) : null,
}));
vi.mock("@/components/files/VersionHistoryDialog", () => ({
	VersionHistoryDialog: ({
		open,
		fileId,
		readOnlySource,
		onOpenChange,
	}: ComponentProps<typeof VersionHistoryDialog>) =>
		open ? (
			<section aria-label="history">
				<button type="button" onClick={() => void readOnlySource?.download(88)}>
					revision-download
				</button>
				<button
					type="button"
					onClick={() => void readOnlySource?.list(fileId, 50)}
				>
					more-versions
				</button>
				<button type="button" onClick={() => onOpenChange(false)}>
					close-versions
				</button>
			</section>
		) : null,
}));
const file = {
	id: 51,
	name: "target.txt",
	size: 12,
	mime_type: "text/plain",
	updated_at: "2026-10-30T00:00:00Z",
	created_at: "2026-10-30T00:00:00Z",
	lock_state: { state: "unlocked" },
	tags: [],
} as FileInfo;
const page: AdminWorkspaceContents = {
	contents: {
		files: [file],
		folders: [],
		files_total: 1,
		folders_total: 101,
		next_file_cursor: { id: 51, value: "target" },
	},
	ancestors: [{ id: 19, name: "folder", icon: { kind: "default" } }],
	deleted: false,
};
function Probe() {
	const location = useLocation();
	return <p data-testid="url">{location.search}</p>;
}
function mount(search = "") {
	return render(
		<MemoryRouter initialEntries={[`/admin/users/7/files${search}`]}>
			<Probe />
			<AdminWorkspaceBrowser scopeKind="personal" targetId={7} />
		</MemoryRouter>,
	);
}
beforeEach(() => {
	vi.resetAllMocks();
	useFileStore.setState({ viewMode: "list" });
	mocks.list.mockResolvedValue(page);
	mocks.getFile.mockResolvedValue(file);
	mocks.getFolder.mockResolvedValue({
		folder: { id: 19, name: "target-folder" },
		child_count: { files: 0, folders: 0 },
	} as AdminFolderInfo);
	mocks.listVersions.mockResolvedValue({ items: [], nextAfterSequence: null });
	mocks.download.mockResolvedValue(undefined);
	mocks.thumbnailPath.mockReturnValue("/scoped-thumbnail");
	mocks.trash.mockResolvedValue({
		files: [
			{ ...file, expires_at: "2026-11-06T00:00:00Z", original_path: "/old" },
		],
		folders: [],
		files_total: 1,
		folders_total: 0,
		next_file_cursor: null,
	});
});
it("paginates folders in both directions and resets the file cursor independently", async () => {
	mount("?folderOffset=50&fileAfterId=9&fileAfterValue=old");
	await screen.findByTestId("items");
	fireEvent.click(
		screen.getByRole("button", { name: "content_previous_folders" }),
	);
	await waitFor(() =>
		expect(mocks.list).toHaveBeenLastCalledWith(
			expect.objectContaining({ folder_offset: 0, file_after_id: 9 }),
		),
	);
	fireEvent.click(screen.getByRole("button", { name: "content_next_folders" }));
	await waitFor(() =>
		expect(mocks.list).toHaveBeenLastCalledWith(
			expect.objectContaining({ folder_offset: 50 }),
		),
	);
	fireEvent.click(screen.getByRole("button", { name: "content_first_files" }));
	await waitFor(() =>
		expect(mocks.list).toHaveBeenLastCalledWith(
			expect.objectContaining({
				file_after_id: undefined,
				file_after_value: undefined,
			}),
		),
	);
});
it.each(["table-sort", "sort-by", "sort-order"])(
	"uses controlled %s without changing the ordinary workspace sort",
	async (control) => {
		useFileStore.setState({ sortBy: "name", sortOrder: "asc" });
		mount("?folderOffset=50&fileAfterId=9&fileAfterValue=old");
		await screen.findByTestId("items");
		fireEvent.click(screen.getByRole("button", { name: control }));
		await waitFor(() =>
			expect(mocks.list).toHaveBeenLastCalledWith(
				expect.objectContaining({
					folder_offset: 0,
					file_after_id: undefined,
					sort_by:
						control === "table-sort"
							? "size"
							: control === "sort-by"
								? "type"
								: "name",
					sort_order: control === "sort-by" ? "asc" : "desc",
				}),
			),
		);
		expect(useFileStore.getState()).toMatchObject({
			sortBy: "name",
			sortOrder: "asc",
		});
	},
);
it("refreshes, switches view and navigates through the shared breadcrumb", async () => {
	mount("?folderId=19");
	expect(await screen.findByTestId("items")).toHaveTextContent("19");
	const calls = mocks.list.mock.calls.length;
	fireEvent.click(screen.getByRole("button", { name: "refresh" }));
	await waitFor(() => expect(mocks.list).toHaveBeenCalledTimes(calls + 1));
	fireEvent.click(screen.getByRole("button", { name: "grid" }));
	expect(await screen.findByTestId("items")).toHaveAttribute(
		"data-mode",
		"grid",
	);
	fireEvent.click(screen.getByRole("button", { name: "breadcrumb-root" }));
	await waitFor(() =>
		expect(mocks.list).toHaveBeenLastCalledWith(
			expect.objectContaining({ folder_id: undefined }),
		),
	);
});
it("routes list, detail and revision downloads through the same access, reporting download failure", async () => {
	mount();
	await screen.findByTestId("items");
	fireEvent.click(
		screen.getByRole("button", { name: "download", exact: true }),
	);
	await waitFor(() => expect(mocks.download).toHaveBeenCalledWith("grant", 51));
	fireEvent.click(screen.getByRole("button", { name: "file-click" }));
	await screen.findByRole("region", { name: "details" });
	const error = new Error("download init failed");
	mocks.download.mockRejectedValueOnce(error);
	fireEvent.click(
		await screen.findByRole("button", { name: "files:download" }),
	);
	await waitFor(() => expect(mocks.error).toHaveBeenCalledWith(error));
	fireEvent.click(
		screen.getByRole("button", { name: "versions", exact: true }),
	);
	fireEvent.click(await screen.findByRole("button", { name: "more-versions" }));
	expect(mocks.listVersions).toHaveBeenCalledWith("grant", 51, 50);
	fireEvent.click(screen.getByRole("button", { name: "revision-download" }));
	expect(mocks.download).toHaveBeenCalledWith("grant", 51, 88);
	fireEvent.click(screen.getByRole("button", { name: "close-versions" }));
	expect(screen.getByTestId("url")).toHaveTextContent("panel=info");
	fireEvent.click(screen.getByRole("button", { name: "file-info" }));
	expect(screen.getByTestId("url")).toHaveTextContent("fileId=51");
});
it("clears stale navigation on recycle-bin entry and blocks opening deleted folders", async () => {
	mount("?folderId=19&folderOffset=50&fileAfterId=9");
	await screen.findByTestId("items");
	fireEvent.click(
		screen.getAllByRole("button", { name: "trash", exact: true })[1],
	);
	expect(await screen.findByText("/old")).toBeInTheDocument();
	expect(screen.getByTestId("url")).toHaveTextContent("view=trash");
	expect(screen.getByTestId("url")).not.toHaveTextContent("folderId");
	fireEvent.click(screen.getByRole("button", { name: "open-folder" }));
	expect(screen.getByTestId("url")).toHaveTextContent("view=trash");
});
it("reports a folder detail failure and ignores late folder/list failures after unmount", async () => {
	const error = new Error("folder revoked");
	mocks.getFolder.mockRejectedValueOnce(error);
	const view = mount("?infoFolderId=19");
	expect(await screen.findByRole("alert")).toHaveTextContent(
		"content_workspace_unavailable",
	);
	expect(mocks.error).toHaveBeenCalledWith(error);
	view.unmount();
	let reject!: (error: Error) => void;
	mocks.getFolder.mockReturnValueOnce(
		new Promise((_, fail) => {
			reject = fail;
		}),
	);
	mocks.error.mockClear();
	const next = mount("?infoFolderId=19");
	next.unmount();
	await act(async () => reject(new Error("late")));
	expect(mocks.error).not.toHaveBeenCalled();
});
it("ignores late recycle-bin responses after unmount", async () => {
	let resolve!: (value: unknown) => void;
	mocks.trash.mockReturnValueOnce(
		new Promise((done) => {
			resolve = done;
		}),
	);
	useFileStore.setState({ viewMode: "grid" });
	const view = mount("?view=trash");
	view.unmount();
	await act(async () =>
		resolve({
			files: [],
			folders: [],
			files_total: 0,
			folders_total: 0,
			next_file_cursor: null,
		}),
	);
	expect(mocks.error).not.toHaveBeenCalled();
});

it("ignores late directory failure and folder success after the view is disposed", async () => {
	let reject!: (error: Error) => void;
	let resolve!: (value: unknown) => void;
	mocks.list.mockReturnValueOnce(
		new Promise((_, fail) => {
			reject = fail;
		}),
	);
	mocks.getFolder.mockReturnValueOnce(
		new Promise((done) => {
			resolve = done;
		}),
	);
	const view = mount("?infoFolderId=19");
	view.unmount();
	await act(async () => {
		reject(new Error("late listing failure"));
		resolve({
			folder: { id: 19, name: "late-folder" },
			child_count: { files: 0, folders: 0 },
		});
	});
	expect(mocks.error).not.toHaveBeenCalled();
	expect(screen.queryByText("late-folder")).not.toBeInTheDocument();
});
