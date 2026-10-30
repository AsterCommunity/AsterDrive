import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter, useLocation } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AdminWorkspaceContents } from "@/types/api";
import { AdminWorkspaceBrowser } from "./AdminWorkspaceBrowser";

const mocks = vi.hoisted(() => ({
	list: vi.fn(),
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
vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
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
	return <p data-testid="url">{useLocation().search}</p>;
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
		mocks.list.mockResolvedValue(empty);
	});
	it("restores folder, deleted and pagination state from URL without creating mutation entry points", async () => {
		mount(
			"/admin/users/7/files?folderId=19&deleted=true&folderOffset=50&fileAfterId=9&fileAfterValue=z&sortBy=size&sortOrder=desc",
		);
		expect(await screen.findByText("content_empty")).toBeInTheDocument();
		expect(mocks.list).toHaveBeenCalledWith(
			expect.objectContaining({
				access_id: "personal:7",
				folder_id: 19,
				deleted: true,
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
		fireEvent.click(screen.getByRole("button", { name: "content_root" }));
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
						lock_state: { locked: false },
					},
				],
			},
		});
		mount("/admin/teams/7/files", "team");
		await screen.findByRole("button", { name: "nested" });
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
		fireEvent.click(screen.getByRole("button", { name: "nested" }));
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
		fireEvent.click(screen.getByRole("button", { name: "content_root" }));
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
