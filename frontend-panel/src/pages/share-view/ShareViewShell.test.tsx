import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ShareFolderPageShell, SharePageShell } from "./ShareViewShell";

vi.mock("@/components/layout/ShareTopBar", () => ({
	ShareTopBar: ({
		mobileOpen,
		onSidebarToggle,
	}: {
		mobileOpen?: boolean;
		onSidebarToggle?: () => void;
	}) => (
		<button
			type="button"
			data-testid="share-topbar"
			data-open={String(mobileOpen)}
			onClick={onSidebarToggle}
		>
			toggle
		</button>
	),
}));

vi.mock("@/pages/share-view/ShareFolderSidebar", () => ({
	ShareFolderSidebar: ({
		mobileOpen,
		onMobileClose,
	}: {
		mobileOpen: boolean;
		onMobileClose: () => void;
	}) => (
		<button
			type="button"
			data-testid="share-sidebar"
			data-open={String(mobileOpen)}
			onClick={onMobileClose}
		>
			close
		</button>
	),
}));

describe("ShareFolderPageShell", () => {
	it("uses the dynamic viewport height for non-folder share shells", () => {
		const { container } = render(
			<SharePageShell>
				<div>share contents</div>
			</SharePageShell>,
		);

		expect(container.firstChild).toHaveClass("h-dvh");
		expect(container.firstChild).not.toHaveClass("h-screen");
	});

	it("keeps the topbar and mobile folder drawer state synchronized", () => {
		render(
			<ShareFolderPageShell
				breadcrumb={[{ id: null, name: "Shared Root" }]}
				folderContents={null}
				info={{ name: "Shared Root" } as never}
				shareOwnerText="Shared by Alice"
				token="share-token"
				onNavigate={vi.fn()}
			>
				<div>share contents</div>
			</ShareFolderPageShell>,
		);

		expect(screen.getByTestId("share-topbar")).toHaveAttribute(
			"data-open",
			"false",
		);
		fireEvent.click(screen.getByTestId("share-topbar"));
		expect(screen.getByTestId("share-topbar")).toHaveAttribute(
			"data-open",
			"true",
		);
		expect(screen.getByTestId("share-sidebar")).toHaveAttribute(
			"data-open",
			"true",
		);

		fireEvent.click(screen.getByTestId("share-sidebar"));
		expect(screen.getByTestId("share-sidebar")).toHaveAttribute(
			"data-open",
			"false",
		);
		expect(screen.getByText("share contents")).toBeInTheDocument();
	});
});
