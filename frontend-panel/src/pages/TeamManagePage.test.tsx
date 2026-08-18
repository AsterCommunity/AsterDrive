import { render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import TeamManagePage from "@/pages/TeamManagePage";

const mockState = vi.hoisted(() => ({
	ensureLoaded: vi.fn(),
	navigate: vi.fn(),
	reload: vi.fn(),
	teamManageDetailProps: null as null | {
		onPageTabChange?: (tab: string, options?: { replace?: boolean }) => void;
	},
}));

vi.mock("react-router-dom", () => ({
	Navigate: ({ to }: { to: string }) => <div data-testid="navigate">{to}</div>,
	useNavigate: () => mockState.navigate,
	useParams: () => ({
		section: "overview",
		teamId: "11",
	}),
}));

vi.mock("@/components/layout/AppLayout", () => ({
	AppLayout: ({ children }: { children: ReactNode }) => (
		<div data-testid="app-layout">{children}</div>
	),
}));

vi.mock("@/components/settings/team-manage-detail/TeamManageDetail", () => ({
	TeamManageDetail: (props: {
		onPageTabChange?: (tab: string, options?: { replace?: boolean }) => void;
	}) => {
		mockState.teamManageDetailProps = props;
		return <div data-testid="team-manage-detail">team-manage-detail</div>;
	},
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: <T,>(selector: (state: { user: { id: number } }) => T) =>
		selector({ user: { id: 7 } }),
}));

vi.mock("@/stores/teamStore", () => ({
	useTeamStore: <T,>(
		selector: (state: {
			ensureLoaded: typeof mockState.ensureLoaded;
			reload: typeof mockState.reload;
			teams: Array<{ id: number }>;
		}) => T,
	) =>
		selector({
			ensureLoaded: mockState.ensureLoaded,
			reload: mockState.reload,
			teams: [{ id: 11 }],
		}),
}));

describe("TeamManagePage", () => {
	beforeEach(() => {
		mockState.ensureLoaded.mockReset();
		mockState.navigate.mockReset();
		mockState.reload.mockReset();
		mockState.teamManageDetailProps = null;

		mockState.ensureLoaded.mockResolvedValue(undefined);
		mockState.reload.mockResolvedValue(undefined);
	});

	it("pins the page shell and lets the detail panel own scrolling", async () => {
		const { container } = render(<TeamManagePage />);

		await waitFor(() => {
			expect(mockState.ensureLoaded).toHaveBeenCalledWith(7);
		});

		const appLayout = screen.getByTestId("app-layout");
		const pageShell = appLayout.firstElementChild;
		const pageBody = pageShell?.firstElementChild;

		expect(pageShell).toHaveClass(
			"flex",
			"min-h-0",
			"flex-1",
			"flex-col",
			"overflow-hidden",
		);
		expect(pageBody).toHaveClass("mx-auto", "min-h-0", "flex-1", "flex-col");
		expect(screen.getByTestId("team-manage-detail")).toBeInTheDocument();
		expect(container.querySelector("[data-testid='navigate']")).toBeNull();
	});

	it("changes tabs without enabling a view transition", async () => {
		render(<TeamManagePage />);

		await waitFor(() => {
			expect(mockState.teamManageDetailProps?.onPageTabChange).toBeDefined();
		});

		mockState.teamManageDetailProps?.onPageTabChange?.("members", {
			replace: true,
		});

		expect(mockState.navigate).toHaveBeenCalledWith(
			"/settings/teams/11/members",
			{
				replace: true,
				viewTransition: false,
			},
		);
	});
});
