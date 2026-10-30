import {
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
describe("scoped workspace sidebar", () => {
	beforeEach(() => {
		vi.resetAllMocks();
		mocks.list.mockResolvedValue({
			contents: { folders: [], folders_total: 0 },
		});
	});
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
