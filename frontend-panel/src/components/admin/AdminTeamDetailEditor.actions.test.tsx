import { act, render, waitFor } from "@testing-library/react";
import type { FormEvent, ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AdminTeamDetailEditor } from "@/components/admin/AdminTeamDetailEditor";

const state = vi.hoisted(() => ({
	team: null as null | Record<string, unknown>,
	overviewProps: null as null | Record<string, unknown>,
	membersProps: null as null | Record<string, unknown>,
	dangerProps: null as null | Record<string, unknown>,
	shellProps: null as null | Record<string, unknown>,
	handleApiError: vi.fn(),
	loadAuditEntries: vi.fn(),
	loadMembers: vi.fn(),
	loadTeamDetail: vi.fn(),
	addMember: vi.fn(),
	deleteTeam: vi.fn(),
	removeMember: vi.fn(),
	restoreTeam: vi.fn(),
	update: vi.fn(),
	updateMember: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("sonner", () => ({ toast: { error: vi.fn(), success: vi.fn() } }));
vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => state.handleApiError(...args),
}));
vi.mock("@/services/adminService", () => ({
	adminTeamService: {
		addMember: (...args: unknown[]) => state.addMember(...args),
		delete: (...args: unknown[]) => state.deleteTeam(...args),
		removeMember: (...args: unknown[]) => state.removeMember(...args),
		restore: (...args: unknown[]) => state.restoreTeam(...args),
		update: (...args: unknown[]) => state.update(...args),
		updateMember: (...args: unknown[]) => state.updateMember(...args),
	},
}));
vi.mock("@/components/admin/admin-team-detail/useAdminTeamDetailData", () => ({
	useAdminTeamDetailData: () => ({
		auditEntries: [],
		auditLoading: false,
		auditTotal: 0,
		detailLoading: false,
		loadAuditEntries: state.loadAuditEntries,
		loadMembers: state.loadMembers,
		loadTeamDetail: state.loadTeamDetail,
		managerCount: 1,
		memberLoading: false,
		memberTotal: 0,
		members: [],
		ownerCount: 1,
		team: state.team,
	}),
}));
vi.mock("@/components/admin/admin-team-detail/useAdminTeamDetailTabs", () => ({
	useAdminTeamDetailTabs: ({ pageTab }: { pageTab: string }) => ({
		currentTab: pageTab,
		handleTabChange: vi.fn(),
		panelAnimationClass: "",
	}),
}));
vi.mock(
	"@/components/admin/admin-team-detail/useAdminTeamDetailScrollRestoration",
	() => ({ useAdminTeamDetailScrollRestoration: vi.fn() }),
);
vi.mock("@/components/admin/admin-team-detail/AdminTeamDetailSections", () => ({
	AdminTeamDetailAuditSection: () => null,
	AdminTeamDetailDangerSection: (props: Record<string, unknown>) => {
		state.dangerProps = props;
		return null;
	},
	AdminTeamDetailMembersSection: (props: Record<string, unknown>) => {
		state.membersProps = props;
		return null;
	},
	AdminTeamDetailOverviewSection: (props: Record<string, unknown>) => {
		state.overviewProps = props;
		return null;
	},
}));
vi.mock("@/components/admin/admin-team-detail/AdminTeamDetailShell", () => ({
	AdminTeamDetailShell: (props: Record<string, unknown>) => {
		state.shellProps = props;
		return (
			<>
				{props.overviewSection as ReactNode}
				{props.membersSection as ReactNode}
				{props.dangerSection as ReactNode}
			</>
		);
	},
}));

function team(archived = false) {
	return {
		archived_at: archived ? "2026-09-11T00:00:00Z" : null,
		created_at: "2026-09-01T00:00:00Z",
		created_by: { id: 1, username: "root" },
		description: "Team description",
		id: 14,
		member_count: 1,
		name: "Product",
		policy_group_id: 5,
		storage_quota: 1024,
		storage_used: 512,
		updated_at: "2026-09-01T00:00:00Z",
	};
}

function renderEditor() {
	return render(
		<AdminTeamDetailEditor
			onBack={vi.fn()}
			onPageTabChange={vi.fn()}
			onRefreshPolicyGroups={async () => undefined}
			pageTab="overview"
			policyGroups={[
				{
					id: 5,
					name: "Primary",
					is_enabled: true,
					items: [{ id: 1 }],
				} as never,
			]}
			policyGroupsLoading={false}
			teamId={14}
		/>,
	);
}

function requiredProp<T>(
	props: Record<string, unknown> | null,
	name: string,
): T {
	if (!props || !(name in props)) {
		throw new Error(`missing captured prop: ${name}`);
	}
	return props[name] as T;
}

describe("AdminTeamDetailEditor actions", () => {
	beforeEach(() => {
		state.team = team();
		state.overviewProps = null;
		state.membersProps = null;
		state.dangerProps = null;
		state.shellProps = null;
		for (const mock of [
			state.handleApiError,
			state.loadAuditEntries,
			state.loadMembers,
			state.loadTeamDetail,
			state.addMember,
			state.deleteTeam,
			state.removeMember,
			state.restoreTeam,
			state.update,
			state.updateMember,
		]) {
			mock.mockReset();
			mock.mockResolvedValue(undefined);
		}
	});

	it("runs archive, restore, and member mutations and guards archived teams", async () => {
		const view = renderEditor();
		await waitFor(() => expect(state.overviewProps?.name).toBe("Product"));

		await act(async () => {
			await requiredProp<() => Promise<void>>(state.dangerProps, "onArchive")();
		});
		expect(state.deleteTeam).toHaveBeenCalledWith(14);

		act(() => {
			requiredProp<(value: string) => void>(
				state.membersProps,
				"setMemberIdentifier",
			)("new-user");
		});
		await act(async () => {
			await requiredProp<(event: FormEvent<HTMLFormElement>) => Promise<void>>(
				state.membersProps,
				"onAddMember",
			)({ preventDefault: vi.fn() } as never);
			await requiredProp<(id: number, role: string) => Promise<void>>(
				state.membersProps,
				"onUpdateMemberRole",
			)(2, "admin");
			await requiredProp<(id: number) => Promise<void>>(
				state.membersProps,
				"onRemoveMember",
			)(2);
		});
		expect(state.addMember).toHaveBeenCalledWith(14, {
			identifier: "new-user",
			role: "member",
		});
		expect(state.updateMember).toHaveBeenCalledWith(14, 2, { role: "admin" });
		expect(state.removeMember).toHaveBeenCalledWith(14, 2);

		state.team = team(true);
		view.rerender(
			<AdminTeamDetailEditor
				onBack={vi.fn()}
				onPageTabChange={vi.fn()}
				onRefreshPolicyGroups={async () => undefined}
				pageTab="danger"
				policyGroups={[]}
				policyGroupsLoading={false}
				teamId={14}
			/>,
		);
		await act(async () => {
			await requiredProp<() => Promise<void>>(state.dangerProps, "onRestore")();
			await requiredProp<(event: FormEvent<HTMLFormElement>) => Promise<void>>(
				state.membersProps,
				"onAddMember",
			)({ preventDefault: vi.fn() } as never);
			await requiredProp<(id: number, role: string) => Promise<void>>(
				state.membersProps,
				"onUpdateMemberRole",
			)(2, "member");
			await requiredProp<(id: number) => Promise<void>>(
				state.membersProps,
				"onRemoveMember",
			)(2);
		});
		expect(state.restoreTeam).toHaveBeenCalledWith(14);
		expect(state.addMember).toHaveBeenCalledTimes(1);
		expect(state.updateMember).toHaveBeenCalledTimes(1);
		expect(state.removeMember).toHaveBeenCalledTimes(1);

		requiredProp<() => void>(state.shellProps, "onContentScroll")();
		requiredProp<() => void>(state.shellProps, "onSidebarScroll")();
	});

	it("routes mutation failures through the shared error handler", async () => {
		const error = new Error("team mutation failed");
		state.deleteTeam.mockRejectedValueOnce(error);
		renderEditor();
		await waitFor(() => expect(state.dangerProps).not.toBeNull());
		await act(async () => {
			await requiredProp<() => Promise<void>>(state.dangerProps, "onArchive")();
		});
		expect(state.handleApiError).toHaveBeenCalledWith(error);
	});
});
