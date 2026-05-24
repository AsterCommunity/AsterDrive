import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AdminLayout } from "@/components/layout/AdminLayout";

const mockState = vi.hoisted(() => ({
	currentPath: "/admin/users",
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => `translated:${key}`,
	}),
}));

vi.mock("react-router-dom", () => ({
	NavLink: ({
		to,
		onClick,
		className,
		children,
	}: {
		to: string;
		onClick?: () => void;
		className?: string | ((state: { isActive: boolean }) => string);
		children: React.ReactNode;
	}) => (
		<button
			type="button"
			onClick={onClick}
			className={
				typeof className === "function"
					? className({ isActive: to === mockState.currentPath })
					: className
			}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/layout/AdminTopBar", () => ({
	AdminTopBar: ({ onSidebarToggle }: { onSidebarToggle: () => void }) => (
		<button type="button" onClick={onSidebarToggle}>
			Toggle Admin Sidebar
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => (
		<span data-testid="icon" data-name={name} />
	),
}));

vi.mock("@/components/ui/scroll-area", () => ({
	ScrollArea: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

describe("AdminLayout", () => {
	beforeEach(() => {
		mockState.currentPath = "/admin/users";
	});

	function getMobileOverlay() {
		return screen.getByRole("button", {
			name: "translated:core:close_admin_sidebar",
			hidden: true,
		});
	}

	function expectMobileOverlayClosed() {
		const overlay = getMobileOverlay();
		expect(overlay.className).toContain("pointer-events-none");
		expect(overlay.className).toContain("opacity-0");
		expect(overlay).toHaveAttribute("tabindex", "-1");
	}

	function expectMobileOverlayOpen() {
		const overlay = screen.getByRole("button", {
			name: "translated:core:close_admin_sidebar",
		});
		expect(overlay.className).toContain("opacity-100");
		expect(overlay.className).not.toContain("pointer-events-none");
		expect(overlay).toHaveAttribute("tabindex", "0");
		return overlay;
	}

	it("renders the translated navigation and main content", () => {
		render(<AdminLayout>Admin Content</AdminLayout>);
		const expectedNavigationLabels = [
			"translated:overview",
			"translated:users",
			"translated:teams",
			"translated:policies",
			"translated:remote_nodes",
			"translated:external_auth",
			"translated:policy_groups",
			"translated:shares",
			"translated:tasks",
			"translated:locks",
			"translated:system_settings",
			"translated:audit_log",
			"translated:about",
		];

		expect(screen.getByText("Admin Content")).toBeInTheDocument();
		for (const label of expectedNavigationLabels) {
			expect(
				screen.getByRole("button", { name: new RegExp(label, "i") }),
			).toBeInTheDocument();
		}
		expect(screen.getAllByTestId("icon")).toHaveLength(
			expectedNavigationLabels.length,
		);
	});

	it("opens the mobile sidebar overlay and closes it again", () => {
		const { container } = render(<AdminLayout>Admin Content</AdminLayout>);

		expect(container.querySelector("aside")?.className).toContain(
			"-translate-x-full",
		);

		expectMobileOverlayClosed();

		fireEvent.click(
			screen.getByRole("button", { name: "Toggle Admin Sidebar" }),
		);
		expect(container.querySelector("aside")?.className).toContain(
			"translate-x-0",
		);
		expectMobileOverlayOpen();

		fireEvent.click(
			screen.getByRole("button", {
				name: "translated:core:close_admin_sidebar",
			}),
		);
		expectMobileOverlayClosed();
	});

	it("closes the mobile sidebar when a nav link is selected", () => {
		render(<AdminLayout>Admin Content</AdminLayout>);

		fireEvent.click(
			screen.getByRole("button", { name: "Toggle Admin Sidebar" }),
		);
		fireEvent.click(screen.getByRole("button", { name: /translated:locks/i }));

		expectMobileOverlayClosed();
	});

	it("uses full-height mobile overlay positioning below the top bar", () => {
		render(<AdminLayout>Admin Content</AdminLayout>);

		fireEvent.click(
			screen.getByRole("button", { name: "Toggle Admin Sidebar" }),
		);

		const overlay = expectMobileOverlayOpen();

		expect(overlay.className).toContain("bottom-0");
		expect(overlay.className).toContain("top-16");
	});
});
