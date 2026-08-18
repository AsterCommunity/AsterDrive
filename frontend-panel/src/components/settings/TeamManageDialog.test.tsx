import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TeamManageDialog } from "@/components/settings/TeamManageDialog";
import type { UserSummary } from "@/types/api";

const mockState = vi.hoisted(() => ({
	handleApiError: vi.fn(),
	navigate: vi.fn(),
}));

const teamServiceMocks = vi.hoisted(() => ({
	addMember: vi.fn(),
	delete: vi.fn(),
	get: vi.fn(),
	listAuditLogs: vi.fn(),
	listMembers: vi.fn(),
	removeMember: vi.fn(),
	update: vi.fn(),
	updateMember: vi.fn(),
}));

function createUserSummary(): UserSummary {
	return {
		id: 1,
		username: "owner",
		profile: {
			display_name: "Owner",
			avatar: {
				source: "none",
				url_1024: null,
				url_512: null,
				version: 0,
			},
		},
	};
}

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("react-router-dom", () => ({
	useNavigate: () => mockState.navigate,
}));

vi.mock("sonner", () => ({
	toast: {
		error: vi.fn(),
		success: vi.fn(),
	},
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/services/teamService", () => ({
	teamService: teamServiceMocks,
}));

describe("TeamManageDialog", () => {
	beforeEach(() => {
		mockState.handleApiError.mockReset();
		mockState.navigate.mockReset();
		teamServiceMocks.addMember.mockReset();
		teamServiceMocks.delete.mockReset();
		teamServiceMocks.get.mockReset();
		teamServiceMocks.listAuditLogs.mockReset();
		teamServiceMocks.listMembers.mockReset();
		teamServiceMocks.removeMember.mockReset();
		teamServiceMocks.update.mockReset();
		teamServiceMocks.updateMember.mockReset();

		teamServiceMocks.get.mockResolvedValue({
			archived_at: null,
			created_at: "2026-04-01T00:00:00Z",
			created_by: createUserSummary(),
			description: "Team description",
			id: 11,
			member_count: 8,
			my_role: "owner",
			name: "Product",
			policy_group_id: null,
			storage_quota: 1024,
			storage_used: 512,
			updated_at: "2026-04-02T00:00:00Z",
		});
		teamServiceMocks.listMembers.mockResolvedValue({
			items: [],
			manager_count: 1,
			owner_count: 1,
			total: 0,
		});
		teamServiceMocks.listAuditLogs.mockResolvedValue({
			items: [],
			total: 0,
		});
	});

	it("uses a fixed shell and a native scrollable detail column in page layout", async () => {
		const { container } = render(
			<TeamManageDialog
				layout="page"
				currentUserId={1}
				onArchivedReload={async () => undefined}
				onOpenChange={vi.fn()}
				onPageTabChange={vi.fn()}
				onTeamsReload={async () => undefined}
				open
				pageTab="overview"
				teamId={11}
				teamSummary={{
					archived_at: null,
					created_at: "2026-04-01T00:00:00Z",
					created_by: createUserSummary(),
					description: "Team description",
					id: 11,
					member_count: 8,
					my_role: "owner",
					name: "Product",
					policy_group_id: null,
					storage_quota: 1024,
					storage_used: 512,
					updated_at: "2026-04-02T00:00:00Z",
				}}
			/>,
		);

		await waitFor(() => {
			expect(teamServiceMocks.get).toHaveBeenCalledWith(11);
			expect(teamServiceMocks.listMembers).toHaveBeenCalled();
			expect(teamServiceMocks.listAuditLogs).toHaveBeenCalled();
		});

		expect(
			container.querySelector(
				".flex.min-h-0.flex-1.flex-col.overflow-y-auto.lg\\:overflow-hidden",
			),
		).not.toBeNull();
		expect(
			container.querySelector(
				".flex.min-h-full.flex-col.lg\\:h-full.lg\\:min-h-0.lg\\:flex-1.lg\\:flex-row",
			),
		).not.toBeNull();
		expect(
			container.querySelector(
				".border-b.lg\\:min-h-0.lg\\:w-80.lg\\:flex-none.lg\\:overflow-y-auto",
			),
		).not.toBeNull();
		expect(
			container.querySelector(
				".min-h-0.min-w-0.lg\\:flex-1.lg\\:flex.lg\\:h-full.lg\\:flex-col.lg\\:overflow-hidden",
			),
		).not.toBeNull();
		expect(
			container.querySelector(
				".flex.flex-col.lg\\:h-full.lg\\:min-h-0.lg\\:flex-1.lg\\:overflow-hidden",
			),
		).not.toBeNull();
		expect(
			container.querySelector(
				".px-6.pt-4.pb-6.lg\\:min-h-0.lg\\:flex-1.lg\\:overflow-y-auto",
			),
		).not.toBeNull();
		expect(container.querySelector('[data-slot="scroll-area"]')).toBeNull();

		const membersTab = screen.getByRole("tab", {
			name: "settings:settings_team_members",
		});
		expect(membersTab).toHaveClass("min-w-0");
		expect(membersTab).toHaveClass("flex-none");
		expect(membersTab.parentElement).toHaveClass("w-full", "gap-5", "border-b");
		expect(membersTab.parentElement).not.toHaveClass("overflow-x-auto");
	});

	it("keeps the overview name input mounted while editing in page layout", async () => {
		render(
			<TeamManageDialog
				layout="page"
				currentUserId={1}
				onArchivedReload={async () => undefined}
				onOpenChange={vi.fn()}
				onPageTabChange={vi.fn()}
				onTeamsReload={async () => undefined}
				open
				pageTab="overview"
				teamId={11}
				teamSummary={{
					archived_at: null,
					created_at: "2026-04-01T00:00:00Z",
					created_by: createUserSummary(),
					description: "Team description",
					id: 11,
					member_count: 8,
					my_role: "owner",
					name: "Product",
					policy_group_id: null,
					storage_quota: 1024,
					storage_used: 512,
					updated_at: "2026-04-02T00:00:00Z",
				}}
			/>,
		);

		const input = (await screen.findByLabelText(
			"core:name",
		)) as HTMLInputElement;
		fireEvent.change(input, { target: { value: "Product Ops" } });

		expect(input.isConnected).toBe(true);
		expect(screen.getByLabelText("core:name")).toBe(input);
		expect(input.value).toBe("Product Ops");
	});
});
