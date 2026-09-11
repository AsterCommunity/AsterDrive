import { render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AdminTeamDetailPage from "@/pages/admin/AdminTeamDetailPage";

const mockState = vi.hoisted(() => ({
	handleApiError: vi.fn(),
	listPolicyGroups: vi.fn(),
	navigate: vi.fn(),
	adminTeamDetailEditorProps: null as null | {
		onBack?: () => void;
		onPageTabChange?: (tab: string, options?: { replace?: boolean }) => void;
	},
}));

vi.mock("react-router-dom", () => ({
	Navigate: ({ to }: { to: string }) => <div data-testid="navigate">{to}</div>,
	useNavigate: () => mockState.navigate,
	useParams: () => ({
		section: "overview",
		teamId: "1",
	}),
}));

vi.mock("@/components/admin/AdminTeamDetailEditor", () => ({
	AdminTeamDetailEditor: (props: {
		onBack?: () => void;
		onPageTabChange?: (tab: string, options?: { replace?: boolean }) => void;
	}) => {
		mockState.adminTeamDetailEditorProps = props;
		return (
			<div data-testid="admin-team-detail-editor">admin-team-detail-editor</div>
		);
	},
}));

vi.mock("@/components/layout/AdminLayout", () => ({
	AdminLayout: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

vi.mock("@/components/layout/AdminPageShell", () => ({
	AdminPageShell: ({
		children,
		className,
	}: {
		children: ReactNode;
		className?: string;
	}) => (
		<div data-testid="admin-page-shell" className={className}>
			{children}
		</div>
	),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/lib/adminPolicyGroupLookup", () => ({
	readAdminPolicyGroupLookup: () => null,
	loadAdminPolicyGroupLookup: () => mockState.listPolicyGroups(100),
}));

describe("AdminTeamDetailPage", () => {
	beforeEach(() => {
		mockState.handleApiError.mockReset();
		mockState.listPolicyGroups.mockReset();
		mockState.navigate.mockReset();
		mockState.adminTeamDetailEditorProps = null;

		mockState.listPolicyGroups.mockResolvedValue([]);
	});

	it("keeps the page shell fixed so the detail panel handles scrolling internally", async () => {
		render(<AdminTeamDetailPage />);

		await waitFor(() => {
			expect(mockState.listPolicyGroups).toHaveBeenCalledWith(100);
		});

		expect(screen.getByTestId("admin-page-shell")).toHaveClass(
			"overflow-hidden",
		);
		expect(screen.getByTestId("admin-team-detail-editor")).toBeInTheDocument();
	});

	it("changes tabs without enabling a view transition", async () => {
		render(<AdminTeamDetailPage />);

		await waitFor(() => {
			expect(
				mockState.adminTeamDetailEditorProps?.onPageTabChange,
			).toBeDefined();
		});

		mockState.adminTeamDetailEditorProps?.onPageTabChange?.("audit", {
			replace: false,
		});

		expect(mockState.navigate).toHaveBeenCalledWith("/admin/teams/1/audit", {
			replace: false,
			viewTransition: false,
		});
	});

	it("returns to the team list without enabling a view transition", async () => {
		render(<AdminTeamDetailPage />);

		await waitFor(() => {
			expect(mockState.adminTeamDetailEditorProps?.onBack).toBeDefined();
		});

		mockState.adminTeamDetailEditorProps?.onBack?.();

		expect(mockState.navigate).toHaveBeenCalledWith("/admin/teams", {
			viewTransition: false,
		});
	});
});
