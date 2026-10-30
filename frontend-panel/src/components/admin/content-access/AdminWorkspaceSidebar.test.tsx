import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AdminWorkspaceSidebar } from "./AdminWorkspaceSidebar";

const mocks = vi.hoisted(() => ({ list: vi.fn(), error: vi.fn() }));
vi.mock("@/services/adminContentAccessService", () => ({
	adminContentAccessService: mocks,
}));
vi.mock("@/hooks/useApiError", () => ({ handleApiError: mocks.error }));
vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("@/hooks/useBlobUrl", () => ({
	useBlobUrl: () => ({ blobUrl: null, error: null }),
}));
beforeEach(() => {
	vi.resetAllMocks();
	mocks.list.mockResolvedValue({
		contents: { folders: [], folders_total: 0 },
	});
});
describe("scoped workspace sidebar", () => {
	it("never requests directory data before access exists", () => {
		render(<AdminWorkspaceSidebar />);
		expect(mocks.list).not.toHaveBeenCalled();
	});
	it("uses the same access for root, nested tree and navigation with file reads disabled", async () => {
		mocks.list.mockResolvedValueOnce({
			contents: {
				folders: [{ id: 9, name: "target-folder", icon: { kind: "default" } }],
				folders_total: 1,
			},
		});
		const navigate = vi.fn();
		render(<AdminWorkspaceSidebar accessId="grant" onNavigate={navigate} />);
		fireEvent.click(
			await screen.findByRole("button", { name: "target-folder" }),
		);
		expect(navigate).toHaveBeenCalledWith(9);
		expect(mocks.list).toHaveBeenCalledWith(
			expect.objectContaining({
				access_id: "grant",
				file_limit: 0,
				folder_limit: 100,
			}),
		);
		fireEvent.click(screen.getByRole("button", { name: "expand_tree" }));
		await waitFor(() =>
			expect(mocks.list).toHaveBeenCalledWith(
				expect.objectContaining({
					access_id: "grant",
					folder_id: 9,
					file_limit: 0,
				}),
			),
		);
	});
});

it("provides the target recycle-bin entry alongside the tree using the current access", async () => {
	const onTrash = vi.fn();
	render(<AdminWorkspaceSidebar accessId="grant" onTrash={onTrash} inTrash />);
	fireEvent.click(screen.getByRole("button", { name: "core:trash" }));
	expect(onTrash).toHaveBeenCalledOnce();
	expect(
		screen.queryByRole("button", { name: /restore|purge/i }),
	).not.toBeInTheDocument();
});

it("shares the full root and folder hit areas without enabling mutation or bubbling toggle clicks", async () => {
	mocks.list.mockResolvedValue({
		contents: {
			folders: [{ id: 9, name: "folder", icon: { kind: "default" } }],
			folders_total: 1,
		},
	});
	const onNavigate = vi.fn();
	const onTrash = vi.fn();
	const view = render(
		<AdminWorkspaceSidebar
			accessId="grant"
			onNavigate={onNavigate}
			onTrash={onTrash}
			currentId={9}
		/>,
	);
	const folder = await screen.findByRole("button", { name: "folder" });
	const row = folder.closest<HTMLElement>("[data-folder-tree-row]");
	expect(row).not.toBeNull();
	expect(row).not.toHaveAttribute("draggable");
	expect(row).toHaveClass("bg-accent");
	fireEvent.click(row as HTMLElement);
	expect(onNavigate).toHaveBeenCalledExactlyOnceWith(9);
	fireEvent.click(folder);
	expect(onNavigate).toHaveBeenCalledTimes(2);
	fireEvent.click(
		within(row as HTMLElement).getByRole("button", { name: "expand_tree" }),
	);
	expect(onNavigate).toHaveBeenCalledTimes(2);
	const root = screen
		.getByRole("button", { name: "root" })
		.closest<HTMLElement>("[data-folder-tree-root-row]");
	fireEvent.click(root as HTMLElement);
	expect(onNavigate).toHaveBeenLastCalledWith();
	expect(onNavigate).toHaveBeenCalledTimes(3);
	fireEvent.click(
		within(root as HTMLElement).getByRole("button", { name: "collapse_tree" }),
	);
	expect(onNavigate).toHaveBeenCalledTimes(3);
	view.rerender(
		<AdminWorkspaceSidebar
			accessId="grant"
			onNavigate={onNavigate}
			onTrash={onTrash}
			currentId={9}
			inTrash
		/>,
	);
	expect(root).not.toHaveClass("bg-accent");
	const trash = screen.getByRole("button", { name: "core:trash" });
	expect(trash).toHaveAttribute("aria-current", "page");
	fireEvent.click(trash);
	expect(onTrash).toHaveBeenCalledOnce();
});

it("uses the shared user identity and initials fallback when the inspected user has no avatar", () => {
	const user = {
		id: 7,
		username: "target-user",
		storage_used: 0,
		profile: {
			display_name: "Target User",
			avatar: { source: "none", url_512: null, url_1024: null, version: 0 },
		},
	} as import("@/types/api").UserInfo;
	const view = render(<AdminWorkspaceSidebar user={user} />);
	const identity = screen.getByTestId("admin-workspace-identity");
	expect(within(identity).getByText("TU")).toBeInTheDocument();
	expect(within(identity).getByText("@target-user")).toBeInTheDocument();
	expect(within(identity).queryByRole("img")).not.toBeInTheDocument();
	view.rerender(
		<AdminWorkspaceSidebar
			user={{
				...user,
				profile: {
					display_name: null,
					avatar: {
						source: "upload",
						url_512: "https://example.test/avatar.webp",
						url_1024: null,
						version: 1,
					},
				},
			}}
		/>,
	);
	expect(
		within(identity).getByRole("img", { name: "target-user" }),
	).toHaveAttribute("src", "https://example.test/avatar.webp");
});

it("appends directory pages and collapses a loaded nested tree", async () => {
	mocks.list.mockReset();
	mocks.list.mockImplementation(async ({ folder_id, folder_offset }) => ({
		contents: {
			folders: folder_id
				? [{ id: 11, name: "child" }]
				: folder_offset
					? [{ id: 10, name: "second" }]
					: [{ id: 9, name: "first" }],
			folders_total: folder_id ? 1 : 2,
		},
	}));
	const navigate = vi.fn();
	render(<AdminWorkspaceSidebar accessId="grant" onNavigate={navigate} />);
	fireEvent.click(
		await screen.findByRole("button", { name: "admin:content_next_folders" }),
	);
	expect(
		await screen.findByRole("button", { name: "second" }),
	).toBeInTheDocument();
	expect(screen.getByRole("button", { name: "first" })).toBeInTheDocument();
	expect(mocks.list).toHaveBeenCalledWith(
		expect.objectContaining({ folder_offset: 1, folder_id: undefined }),
	);
	const row = screen
		.getByRole("button", { name: "first" })
		.closest("[data-folder-tree-row]") as HTMLElement;
	fireEvent.click(within(row).getByRole("button", { name: "expand_tree" }));
	expect(
		await screen.findByRole("button", { name: "child" }),
	).toBeInTheDocument();
	fireEvent.click(within(row).getByRole("button", { name: "collapse_tree" }));
	await waitFor(() =>
		expect(
			screen.queryByRole("button", { name: "child" }),
		).not.toBeInTheDocument(),
	);
	expect(navigate).not.toHaveBeenCalled();
});

it("shows tree access errors and permits ending an inspected team workspace", async () => {
	const error = new Error("revoked");
	mocks.list.mockRejectedValueOnce(error);
	const end = vi.fn();
	render(
		<AdminWorkspaceSidebar
			accessId="grant"
			onNavigate={vi.fn()}
			onEnd={end}
			team={
				{
					id: 9,
					name: "Support team",
					storage_used: 1024,
				} as import("@/types/api").AdminTeamInfo
			}
		/>,
	);
	expect(await screen.findByRole("alert")).toHaveTextContent(
		"admin:content_workspace_unavailable",
	);
	expect(mocks.error).toHaveBeenCalledWith(error);
	expect(screen.getByText("Support team")).toBeInTheDocument();
	expect(screen.getByTestId("admin-workspace-identity")).toHaveTextContent(
		"1.0 KB",
	);
	fireEvent.click(screen.getByRole("button", { name: "content_access_exit" }));
	expect(end).toHaveBeenCalledOnce();
});

it.each(["resolve", "reject"] as const)(
	"ignores a previous access tree's late %s",
	async (settlement) => {
		let resolve!: (value: unknown) => void;
		let reject!: (error: Error) => void;
		mocks.error.mockClear();
		mocks.list.mockReturnValueOnce(
			new Promise((yes, no) => {
				resolve = yes;
				reject = no;
			}),
		);
		const navigate = vi.fn();
		const view = render(
			<AdminWorkspaceSidebar accessId="old" onNavigate={navigate} />,
		);
		view.rerender(
			<AdminWorkspaceSidebar accessId="new" onNavigate={navigate} />,
		);
		await waitFor(() =>
			expect(mocks.list).toHaveBeenCalledWith(
				expect.objectContaining({ access_id: "new" }),
			),
		);
		await act(async () =>
			settlement === "resolve"
				? resolve({
						contents: { folders: [{ id: 4, name: "stale" }], folders_total: 1 },
					})
				: reject(new Error("stale")),
		);
		expect(screen.queryByText("stale")).not.toBeInTheDocument();
		expect(mocks.error).not.toHaveBeenCalled();
	},
);
