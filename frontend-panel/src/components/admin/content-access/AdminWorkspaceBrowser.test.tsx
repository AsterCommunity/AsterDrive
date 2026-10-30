import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter, useLocation, useNavigate } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useFileStore } from "@/stores/fileStore";
import type { AdminWorkspaceContents } from "@/types/api";
import { AdminWorkspaceBrowser } from "./AdminWorkspaceBrowser";

const mocks = vi.hoisted(() => ({
	list: vi.fn(),
	trash: vi.fn(),
	download: vi.fn(),
	error: vi.fn(),
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
	useTranslation: () => ({
		t: (key: string) => key,
		i18n: { language: "en", t: (key: string) => key },
	}),
}));
vi.mock("./AdminContentAccessGate", () => ({
	AdminContentAccessGate: ({
		children,
		scopeKind,
		targetId,
	}: {
		children: (id: string, end: () => void) => ReactNode;
		scopeKind: string;
		targetId: number;
	}) => (
		<div data-testid="scope">
			{scopeKind}:{targetId}
			{children(`${scopeKind}:${targetId}`, () => {})}
		</div>
	),
}));
vi.mock("@/components/files/FileTypeIcon", () => ({
	FileTypeIcon: () => null,
}));
const empty: AdminWorkspaceContents = {
	contents: {
		files: [],
		folders: [],
		files_total: 0,
		folders_total: 0,
		next_file_cursor: null,
	},
	ancestors: [],
	deleted: false,
};
function Probe() {
	const navigate = useNavigate();
	const location = useLocation();
	return (
		<>
			<p data-testid="url">{location.search}</p>
			<button type="button" onClick={() => navigate("/admin/users/7/files")}>
				navigate-root
			</button>
		</>
	);
}
function mount(
	url = "/admin/users/7/files",
	scopeKind: "personal" | "team" = "personal",
) {
	return render(
		<MemoryRouter initialEntries={[url]}>
			<Probe />
			<AdminWorkspaceBrowser scopeKind={scopeKind} targetId={7} />
		</MemoryRouter>,
	);
}
describe("administrator read-only workspace", () => {
	beforeEach(() => {
		vi.resetAllMocks();
		useFileStore.getState().setViewMode("list");
		mocks.list.mockResolvedValue(empty);
	});
	it("restores folder and pagination state from URL without creating mutation entry points", async () => {
		mount(
			"/admin/users/7/files?folderId=19&folderOffset=50&fileAfterId=9&fileAfterValue=z&sortBy=size&sortOrder=desc",
		);
		expect(await screen.findByText("content_empty")).toBeInTheDocument();
		expect(mocks.list).toHaveBeenCalledWith(
			expect.objectContaining({
				access_id: "personal:7",
				folder_id: 19,
				deleted: false,
				folder_offset: 50,
				file_after_id: 9,
				file_after_value: "z",
				sort_by: "size",
				sort_order: "desc",
			}),
		);
		expect(
			screen.queryByRole("button", { name: /upload|rename|delete|share/i }),
		).not.toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "navigate-root" }));
		await waitFor(() =>
			expect(mocks.list).toHaveBeenLastCalledWith(
				expect.objectContaining({
					folder_id: undefined,
					folder_offset: 0,
					file_after_id: undefined,
				}),
			),
		);
		expect(screen.getByTestId("url")).not.toHaveTextContent("folderId");
	});
	it("keeps team scope distinct and offers directory navigation with bounded pagination", async () => {
		mocks.list.mockResolvedValue({
			...empty,
			contents: {
				...empty.contents,
				folders_total: 60,
				folders: [
					{
						id: 19,
						name: "nested",
						icon: { kind: "default" },
						updated_at: "2026-10-30T00:00:00Z",
						is_shared: false,
						lock_state: { state: "unlocked" },
					},
				],
			},
		});
		mount("/admin/teams/7/files", "team");
		await screen.findByText("nested");
		expect(mocks.list).toHaveBeenCalledWith(
			expect.objectContaining({ access_id: "team:7" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "content_next_folders" }),
		);
		await waitFor(() =>
			expect(mocks.list).toHaveBeenLastCalledWith(
				expect.objectContaining({ folder_offset: 50 }),
			),
		);
		fireEvent.click(screen.getByText("nested"));
		await waitFor(() =>
			expect(mocks.list).toHaveBeenLastCalledWith(
				expect.objectContaining({ folder_id: 19, folder_offset: 0 }),
			),
		);
	});
	it("drops stale responses after navigating to another folder", async () => {
		let release: (data: AdminWorkspaceContents) => void = () => {};
		mocks.list.mockReturnValueOnce(
			new Promise<AdminWorkspaceContents>((resolve) => {
				release = resolve;
			}),
		);
		mount("/admin/users/7/files?folderId=19");
		fireEvent.click(screen.getByRole("button", { name: "navigate-root" }));
		await screen.findByText("content_empty");
		release({
			...empty,
			ancestors: [{ id: 19, name: "stale-folder", icon: { kind: "default" } }],
		});
		await waitFor(() =>
			expect(screen.queryByText("stale-folder")).not.toBeInTheDocument(),
		);
	});
	it("shows inaccessible resources as an error without retaining prior directory data", async () => {
		mocks.list.mockRejectedValue(new Error("scope denied"));
		mount();
		expect(await screen.findByRole("alert")).toHaveTextContent(
			"content_workspace_unavailable",
		);
		expect(screen.queryByRole("table")).not.toBeInTheDocument();
	});
});

vi.mock("@/hooks/useBlobUrl", () => ({
	useBlobUrl: () => ({ blobUrl: null, error: null, loading: false }),
}));

vi.mock("@/components/layout/AppLayout", () => ({
	AppLayout: ({ children }: { children: ReactNode }) => <main>{children}</main>,
}));
vi.mock("./AdminWorkspaceSidebar", () => ({
	AdminWorkspaceSidebar: () => null,
}));

vi.mock("@tanstack/react-virtual", () => ({
	useVirtualizer: ({ count }: { count: number }) => ({
		getVirtualItems: () =>
			Array.from({ length: count }, (_, index) => ({
				index,
				key: index,
				start: index * 52,
				end: (index + 1) * 52,
				size: 52,
			})),
		getTotalSize: () => count * 52,
		measure: () => {},
		scrollToIndex: () => {},
	}),
}));

it("loads the full recycle bin without folder filtering and shows original locations without mutation actions", async () => {
	mocks.trash.mockResolvedValue({
		files: [
			{
				id: 51,
				name: "deleted.txt",
				size: 12,
				mime_type: "text/plain",
				created_at: "2026-10-30T00:00:00Z",
				updated_at: "2026-10-30T00:00:00Z",
				expires_at: "2026-11-06T00:00:00Z",
				original_path: "/other-directory",
				lock_state: { state: "unlocked" },
			},
		],
		folders: [],
		files_total: 1,
		folders_total: 0,
		next_file_cursor: null,
	});
	const views = mount("/admin/users/7/files?view=trash&folderId=19");
	expect(await screen.findByText("deleted.txt")).toBeInTheDocument();
	expect(screen.getByText("/other-directory")).toBeInTheDocument();
	expect(mocks.trash).toHaveBeenCalledWith(
		expect.objectContaining({ access_id: "personal:7" }),
	);
	expect(mocks.trash.mock.calls.at(-1)?.[0]).not.toHaveProperty("folder_id");
	expect(mocks.list).not.toHaveBeenCalled();
	expect(
		screen.queryByRole("button", { name: /restore|purge|empty_trash/i }),
	).not.toBeInTheDocument();
	fireEvent.click(screen.getByRole("button", { name: "navigate-root" }));
	await waitFor(() =>
		expect(mocks.list).toHaveBeenCalledWith(
			expect.objectContaining({ folder_id: undefined, deleted: false }),
		),
	);
	views.unmount();
});

it("uses the recycle-bin expiry cursor instead of ordinary file sorting cursors", async () => {
	mocks.trash.mockResolvedValue({
		files: [],
		folders: [],
		files_total: 10,
		folders_total: 0,
		next_file_cursor: { expires_at: "2026-11-06T00:00:00Z", id: 51 },
	});
	mount("/admin/users/7/files?view=trash");
	fireEvent.click(
		await screen.findByRole("button", { name: "content_next_files" }),
	);
	await waitFor(() =>
		expect(mocks.trash).toHaveBeenLastCalledWith(
			expect.objectContaining({
				file_after_expires_at: "2026-11-06T00:00:00Z",
				file_after_id: 51,
			}),
		),
	);
});
