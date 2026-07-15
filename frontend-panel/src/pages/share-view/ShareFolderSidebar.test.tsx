import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ShareFolderSidebar } from "./ShareFolderSidebar";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: { name?: string }) =>
			options?.name ? `${key}:${options.name}` : key,
	}),
}));

vi.mock("@/components/common/UserAvatarImage", () => ({
	UserAvatarImage: ({ name }: { name: string }) => <span>avatar:{name}</span>,
}));

vi.mock("@/components/ui/scroll-area", () => ({
	ScrollArea: ({ children }: { children: React.ReactNode }) => (
		<div data-testid="share-tree-scroll">{children}</div>
	),
}));

vi.mock("@/pages/share-view/ShareFolderTree", () => ({
	ShareFolderTree: ({
		onNavigate,
	}: {
		onNavigate: (folderId: number, folderName: string) => void;
	}) => (
		<button type="button" onClick={() => onNavigate(7, "Docs")}>
			open-docs
		</button>
	),
}));

const info = {
	name: "Shared Root",
	shared_by: { avatar: null, name: "Alice" },
} as never;

describe("ShareFolderSidebar", () => {
	it("uses the system mobile overlay and closes it from the backdrop", () => {
		const onMobileClose = vi.fn();
		const { container } = render(
			<ShareFolderSidebar
				breadcrumb={[{ id: null, name: "Shared Root" }]}
				folderContents={null}
				info={info}
				mobileOpen
				shareOwnerText="share:shared_by:Alice"
				token="share-token"
				onMobileClose={onMobileClose}
				onNavigate={vi.fn()}
			/>,
		);

		expect(screen.getByTestId("share-folder-sidebar")).toHaveClass(
			"translate-x-0",
			"top-16",
			"h-[calc(100dvh-4rem)]",
		);
		expect(screen.getByText("Shared Root")).toBeInTheDocument();
		expect(screen.getByLabelText("share:shared_by:Alice")).toBeInTheDocument();
		expect(screen.getByText("share:share_owner")).toBeInTheDocument();
		expect(screen.getByText("share:n_downloads")).toBeInTheDocument();
		expect(screen.getByText("share:never_expires")).toBeInTheDocument();
		expect(screen.getByText("share:public_access")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "close_sidebar" }));
		expect(onMobileClose).toHaveBeenCalledTimes(1);
		expect(container.querySelector("aside")).not.toBeNull();
	});

	it("navigates only through the shared tree and closes the mobile drawer", () => {
		const onMobileClose = vi.fn();
		const onNavigate = vi.fn();
		render(
			<ShareFolderSidebar
				breadcrumb={[{ id: null, name: "Shared Root" }]}
				folderContents={null}
				info={info}
				mobileOpen
				shareOwnerText="share:shared_by:Alice"
				token="share-token"
				onMobileClose={onMobileClose}
				onNavigate={onNavigate}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "open-docs" }));
		expect(onNavigate).toHaveBeenCalledWith(7, "Docs");
		expect(onMobileClose).toHaveBeenCalledTimes(1);
	});
});
