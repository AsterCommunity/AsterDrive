import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AdminTeamDetailDialog } from "@/components/admin/AdminTeamDetailDialog";
import type { UserSummary } from "@/types/api";

const mockState = vi.hoisted(() => ({
	handleApiError: vi.fn(),
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
}));

const adminTeamServiceMocks = vi.hoisted(() => ({
	addMember: vi.fn(),
	delete: vi.fn(),
	get: vi.fn(),
	listAuditLogs: vi.fn(),
	listMembers: vi.fn(),
	removeMember: vi.fn(),
	restore: vi.fn(),
	update: vi.fn(),
	updateMember: vi.fn(),
}));

function createUserSummary(): UserSummary {
	return {
		id: 1,
		username: "root",
		profile: {
			display_name: "Root",
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

vi.mock("sonner", () => ({
	toast: {
		error: (...args: unknown[]) => mockState.toastError(...args),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/services/adminService", () => ({
	adminTeamService: adminTeamServiceMocks,
}));

describe("AdminTeamDetailDialog", () => {
	beforeEach(() => {
		mockState.handleApiError.mockReset();
		mockState.toastError.mockReset();
		mockState.toastSuccess.mockReset();
		adminTeamServiceMocks.addMember.mockReset();
		adminTeamServiceMocks.delete.mockReset();
		adminTeamServiceMocks.get.mockReset();
		adminTeamServiceMocks.listAuditLogs.mockReset();
		adminTeamServiceMocks.listMembers.mockReset();
		adminTeamServiceMocks.removeMember.mockReset();
		adminTeamServiceMocks.restore.mockReset();
		adminTeamServiceMocks.update.mockReset();
		adminTeamServiceMocks.updateMember.mockReset();

		adminTeamServiceMocks.get.mockResolvedValue({
			archived_at: null,
			created_at: "2026-04-01T00:00:00Z",
			created_by: createUserSummary(),
			description: "Team description",
			id: 14,
			member_count: 8,
			name: "Product",
			policy_group_id: 5,
			storage_quota: 1024,
			storage_used: 512,
			updated_at: "2026-04-02T00:00:00Z",
		});
		adminTeamServiceMocks.listMembers.mockResolvedValue({
			items: [],
			manager_count: 1,
			owner_count: 1,
			total: 0,
		});
		adminTeamServiceMocks.listAuditLogs.mockResolvedValue({
			items: [],
			total: 0,
		});
	});

	it("uses a fixed shell and a native scrollable detail column in page layout", async () => {
		const { container } = render(
			<AdminTeamDetailDialog
				layout="page"
				onListChange={async () => undefined}
				onOpenChange={vi.fn()}
				onPageTabChange={vi.fn()}
				onRefreshPolicyGroups={async () => undefined}
				open
				pageTab="overview"
				policyGroups={[
					{
						created_at: "2026-04-01T00:00:00Z",
						description: "",
						id: 5,
						is_default: false,
						is_enabled: true,
						items: [
							{
								id: 1,
								max_file_size: 0,
								min_file_size: 0,
								policy: {
									id: 7,
									name: "Default",
								},
								policy_id: 7,
								priority: 1,
							},
						],
						name: "Primary",
						updated_at: "2026-04-01T00:00:00Z",
					},
				]}
				policyGroupsLoading={false}
				teamId={14}
			/>,
		);

		await waitFor(() => {
			expect(adminTeamServiceMocks.get).toHaveBeenCalledWith(14);
			expect(adminTeamServiceMocks.listMembers).toHaveBeenCalled();
			expect(adminTeamServiceMocks.listAuditLogs).toHaveBeenCalled();
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
				".border-b.bg-muted\\/20.lg\\:min-h-0.lg\\:w-80.lg\\:flex-none.lg\\:overflow-y-auto",
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
		expect(membersTab).not.toHaveClass("flex-none");
		expect(membersTab.parentElement).toHaveClass("w-full", "gap-5", "border-b");
		expect(membersTab.parentElement).not.toHaveClass("overflow-x-auto");
	});

	it("keeps the overview name input mounted while editing in page layout", async () => {
		render(
			<AdminTeamDetailDialog
				layout="page"
				onListChange={async () => undefined}
				onOpenChange={vi.fn()}
				onPageTabChange={vi.fn()}
				onRefreshPolicyGroups={async () => undefined}
				open
				pageTab="overview"
				policyGroups={[
					{
						created_at: "2026-04-01T00:00:00Z",
						description: "",
						id: 5,
						is_default: false,
						is_enabled: true,
						items: [
							{
								id: 1,
								max_file_size: 0,
								min_file_size: 0,
								policy: {
									id: 7,
									name: "Default",
								},
								policy_id: 7,
								priority: 1,
							},
						],
						name: "Primary",
						updated_at: "2026-04-01T00:00:00Z",
					},
				]}
				policyGroupsLoading={false}
				teamId={14}
			/>,
		);

		const input = (await screen.findByLabelText(
			"core:name",
		)) as HTMLInputElement;
		await waitFor(() => {
			expect(input).not.toBeDisabled();
		});
		fireEvent.change(input, { target: { value: "Product Ops" } });

		expect(input.isConnected).toBe(true);
		expect(screen.getByLabelText("core:name")).toBe(input);
		expect(input.value).toBe("Product Ops");
	});

	it("converts the overview quota field from MB to bytes when saving", async () => {
		render(
			<AdminTeamDetailDialog
				layout="page"
				onListChange={async () => undefined}
				onOpenChange={vi.fn()}
				onPageTabChange={vi.fn()}
				onRefreshPolicyGroups={async () => undefined}
				open
				pageTab="overview"
				policyGroups={[
					{
						created_at: "2026-04-01T00:00:00Z",
						description: "",
						id: 5,
						is_default: false,
						is_enabled: true,
						items: [
							{
								id: 1,
								max_file_size: 0,
								min_file_size: 0,
								policy: {
									id: 7,
									name: "Default",
								},
								policy_id: 7,
								priority: 1,
							},
						],
						name: "Primary",
						updated_at: "2026-04-01T00:00:00Z",
					},
				]}
				policyGroupsLoading={false}
				teamId={14}
			/>,
		);

		const quotaInput = (await screen.findByLabelText(
			"team_quota_mb",
		)) as HTMLInputElement;
		fireEvent.change(quotaInput, { target: { value: "4" } });
		fireEvent.click(screen.getByRole("button", { name: "save_changes" }));

		await waitFor(() => {
			expect(adminTeamServiceMocks.update).toHaveBeenCalledWith(14, {
				name: "Product",
				description: "Team description",
				storage_quota: 4 * 1024 * 1024,
				policy_group_id: 5,
			});
		});
	});

	it("sends zero quota when the overview quota field is set to zero", async () => {
		render(
			<AdminTeamDetailDialog
				layout="page"
				onListChange={async () => undefined}
				onOpenChange={vi.fn()}
				onPageTabChange={vi.fn()}
				onRefreshPolicyGroups={async () => undefined}
				open
				pageTab="overview"
				policyGroups={[
					{
						created_at: "2026-04-01T00:00:00Z",
						description: "",
						id: 5,
						is_default: false,
						is_enabled: true,
						items: [
							{
								id: 1,
								max_file_size: 0,
								min_file_size: 0,
								policy: {
									id: 7,
									name: "Default",
								},
								policy_id: 7,
								priority: 1,
							},
						],
						name: "Primary",
						updated_at: "2026-04-01T00:00:00Z",
					},
				]}
				policyGroupsLoading={false}
				teamId={14}
			/>,
		);

		const quotaInput = (await screen.findByLabelText(
			"team_quota_mb",
		)) as HTMLInputElement;
		fireEvent.change(quotaInput, { target: { value: "0" } });
		fireEvent.click(screen.getByRole("button", { name: "save_changes" }));

		await waitFor(() => {
			expect(adminTeamServiceMocks.update).toHaveBeenCalledWith(14, {
				name: "Product",
				description: "Team description",
				storage_quota: 0,
				policy_group_id: 5,
			});
		});
	});

	it("does not treat zero text as a change when the team quota is already unlimited", async () => {
		adminTeamServiceMocks.get.mockResolvedValueOnce({
			archived_at: null,
			created_at: "2026-04-01T00:00:00Z",
			created_by: createUserSummary(),
			description: "Team description",
			id: 14,
			member_count: 8,
			name: "Product",
			policy_group_id: 5,
			storage_quota: 0,
			storage_used: 512,
			updated_at: "2026-04-02T00:00:00Z",
		});

		render(
			<AdminTeamDetailDialog
				layout="page"
				onListChange={async () => undefined}
				onOpenChange={vi.fn()}
				onPageTabChange={vi.fn()}
				onRefreshPolicyGroups={async () => undefined}
				open
				pageTab="overview"
				policyGroups={[
					{
						created_at: "2026-04-01T00:00:00Z",
						description: "",
						id: 5,
						is_default: false,
						is_enabled: true,
						items: [
							{
								id: 1,
								max_file_size: 0,
								min_file_size: 0,
								policy: {
									id: 7,
									name: "Default",
								},
								policy_id: 7,
								priority: 1,
							},
						],
						name: "Primary",
						updated_at: "2026-04-01T00:00:00Z",
					},
				]}
				policyGroupsLoading={false}
				teamId={14}
			/>,
		);

		const quotaInput = (await screen.findByLabelText(
			"team_quota_mb",
		)) as HTMLInputElement;
		fireEvent.change(quotaInput, { target: { value: "0" } });

		expect(screen.getByRole("button", { name: "save_changes" })).toBeDisabled();
		expect(adminTeamServiceMocks.update).not.toHaveBeenCalled();
	});

	it("preserves non-integer MB quota bytes when saving unrelated overview edits", async () => {
		adminTeamServiceMocks.get.mockResolvedValueOnce({
			archived_at: null,
			created_at: "2026-04-01T00:00:00Z",
			created_by: createUserSummary(),
			description: "Team description",
			id: 14,
			member_count: 8,
			name: "Product",
			policy_group_id: 5,
			storage_quota: 1024,
			storage_used: 512,
			updated_at: "2026-04-02T00:00:00Z",
		});

		render(
			<AdminTeamDetailDialog
				layout="page"
				onListChange={async () => undefined}
				onOpenChange={vi.fn()}
				onPageTabChange={vi.fn()}
				onRefreshPolicyGroups={async () => undefined}
				open
				pageTab="overview"
				policyGroups={[
					{
						created_at: "2026-04-01T00:00:00Z",
						description: "",
						id: 5,
						is_default: false,
						is_enabled: true,
						items: [
							{
								id: 1,
								max_file_size: 0,
								min_file_size: 0,
								policy: {
									id: 7,
									name: "Default",
								},
								policy_id: 7,
								priority: 1,
							},
						],
						name: "Primary",
						updated_at: "2026-04-01T00:00:00Z",
					},
				]}
				policyGroupsLoading={false}
				teamId={14}
			/>,
		);

		const quotaInput = (await screen.findByLabelText(
			"team_quota_mb",
		)) as HTMLInputElement;
		expect(quotaInput.value).toBe(String(1024 / 1024 / 1024));
		fireEvent.change(screen.getByDisplayValue("Team description"), {
			target: { value: "Updated description" },
		});
		const saveButton = screen.getByRole("button", { name: "save_changes" });
		await waitFor(() => {
			expect(saveButton).not.toBeDisabled();
		});
		fireEvent.click(saveButton);

		await waitFor(() => {
			expect(adminTeamServiceMocks.update).toHaveBeenCalledWith(14, {
				name: "Product",
				description: "Updated description",
				storage_quota: 1024,
				policy_group_id: 5,
			});
		});
	});

	it("rejects overflowing overview quota values before saving", async () => {
		render(
			<AdminTeamDetailDialog
				layout="page"
				onListChange={async () => undefined}
				onOpenChange={vi.fn()}
				onPageTabChange={vi.fn()}
				onRefreshPolicyGroups={async () => undefined}
				open
				pageTab="overview"
				policyGroups={[
					{
						created_at: "2026-04-01T00:00:00Z",
						description: "",
						id: 5,
						is_default: false,
						is_enabled: true,
						items: [
							{
								id: 1,
								max_file_size: 0,
								min_file_size: 0,
								policy: {
									id: 7,
									name: "Default",
								},
								policy_id: 7,
								priority: 1,
							},
						],
						name: "Primary",
						updated_at: "2026-04-01T00:00:00Z",
					},
				]}
				policyGroupsLoading={false}
				teamId={14}
			/>,
		);

		const quotaInput = (await screen.findByLabelText(
			"team_quota_mb",
		)) as HTMLInputElement;
		fireEvent.change(quotaInput, {
			target: { value: "999999999999999999999999" },
		});

		expect(screen.getByRole("button", { name: "save_changes" })).toBeDisabled();
		expect(adminTeamServiceMocks.update).not.toHaveBeenCalled();
		expect(mockState.toastError).not.toHaveBeenCalled();
	});
});
