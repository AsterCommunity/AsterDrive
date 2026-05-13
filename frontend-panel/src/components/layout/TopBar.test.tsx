import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TopBar } from "@/components/layout/TopBar";

const mockState = vi.hoisted(() => ({
	onSearchOpen: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => `translated:${key}`,
	}),
}));

vi.mock("@/components/layout/HeaderControls", () => ({
	HeaderControls: ({ actions }: { actions?: React.ReactNode }) => (
		<div>
			HeaderControls
			{actions}
		</div>
	),
}));

vi.mock("@/components/layout/TopBarShell", () => ({
	TopBarShell: ({
		left,
		center,
		right,
		sidebarOpen,
		sidebarToggleLabels,
	}: {
		left: React.ReactNode;
		center?: React.ReactNode;
		right: React.ReactNode;
		sidebarOpen?: boolean;
		sidebarToggleLabels?: {
			open: string;
			close: string;
		};
	}) => (
		<div
			data-testid="topbar-shell"
			data-sidebar-open={String(Boolean(sidebarOpen))}
			data-open-label={sidebarToggleLabels?.open}
			data-close-label={sidebarToggleLabels?.close}
		>
			<div>{left}</div>
			<div>{center}</div>
			<div>{right}</div>
		</div>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => (
		<span data-testid="icon" data-name={name} />
	),
}));

describe("TopBar", () => {
	beforeEach(() => {
		mockState.onSearchOpen.mockReset();
	});

	it("renders the logo, search trigger, and forwarded actions", () => {
		render(
			<TopBar
				onSidebarToggle={vi.fn()}
				mobileOpen={false}
				actions={<button type="button">Extra</button>}
				onSearchOpen={mockState.onSearchOpen}
			/>,
		);

		expect(screen.getByAltText("translated:app_name")).toBeInTheDocument();
		expect(
			screen.getAllByRole("button", { name: "translated:search:open_search" }),
		).toHaveLength(2);
		expect(
			screen.getByText("translated:search:placeholder"),
		).toBeInTheDocument();
		expect(screen.getByText("HeaderControls")).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Extra" })).toBeInTheDocument();
		expect(screen.getByTestId("topbar-shell")).toHaveAttribute(
			"data-sidebar-open",
			"false",
		);
		expect(screen.getByTestId("topbar-shell")).toHaveAttribute(
			"data-open-label",
			"translated:open_sidebar",
		);
		expect(screen.getByTestId("topbar-shell")).toHaveAttribute(
			"data-close-label",
			"translated:close_sidebar",
		);
		expect(screen.getAllByTestId("icon")[0]).toHaveAttribute(
			"data-name",
			"MagnifyingGlass",
		);
	});

	it("opens the search dialog from both desktop and mobile triggers", () => {
		render(
			<TopBar
				onSidebarToggle={vi.fn()}
				mobileOpen={true}
				onSearchOpen={mockState.onSearchOpen}
			/>,
		);

		const buttons = screen.getAllByRole("button", {
			name: "translated:search:open_search",
		});
		fireEvent.click(buttons[0]);
		fireEvent.click(buttons[1]);

		expect(mockState.onSearchOpen).toHaveBeenCalledTimes(2);
	});
});
