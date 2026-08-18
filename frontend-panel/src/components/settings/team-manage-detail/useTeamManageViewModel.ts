import type { TFunction } from "i18next";
import { useMemo } from "react";
import type { TeamInfo, TeamMemberRole, UserStatus } from "@/types/api";
import {
	TEAM_MANAGE_AUDIT_PAGE_SIZE,
	TEAM_MANAGE_MEMBER_PAGE_SIZE,
} from "./teamManageDetailState";

interface UseTeamManageViewModelArgs {
	activeTeamId: number | null;
	auditOffset: number;
	auditTotal: number;
	canAssignOwner: boolean;
	displayTeam: TeamInfo | null;
	memberOffset: number;
	memberQuery: string;
	memberRoleFilter: "__all__" | TeamMemberRole;
	memberStatusFilter: "__all__" | UserStatus;
	memberTotal: number;
	roleLabel: (role: TeamMemberRole) => string;
	t: TFunction<["core", "settings"]>;
	teamDraft: {
		baseDescription: string;
		baseName: string;
		description: string;
		name: string;
		teamId: number | null;
	} | null;
}

export function useTeamManageViewModel({
	activeTeamId,
	auditOffset,
	auditTotal,
	canAssignOwner,
	displayTeam,
	memberOffset,
	memberQuery,
	memberRoleFilter,
	memberStatusFilter,
	memberTotal,
	roleLabel,
	t,
	teamDraft,
}: UseTeamManageViewModelArgs) {
	const memberKeyword = memberQuery.trim();
	const memberRoleValue =
		memberRoleFilter === "__all__" ? undefined : memberRoleFilter;
	const memberStatusValue =
		memberStatusFilter === "__all__" ? undefined : memberStatusFilter;
	const memberFilters = useMemo(
		() => ({
			keyword: memberKeyword || undefined,
			role: memberRoleValue,
			status: memberStatusValue,
		}),
		[memberKeyword, memberRoleValue, memberStatusValue],
	);
	const roleOptions: TeamMemberRole[] = canAssignOwner
		? ["owner", "admin", "member"]
		: ["admin", "member"];
	const quota = displayTeam?.storage_quota ?? 0;
	const used = displayTeam?.storage_used ?? 0;
	const usagePercentage = quota > 0 ? Math.min((used / quota) * 100, 100) : 0;
	const statusFilterOptions = [
		{
			label: t("settings:settings_team_member_status_filter_all"),
			value: "__all__",
		},
		{ label: t("core:active"), value: "active" },
		{ label: t("core:disabled_status"), value: "disabled" },
	] satisfies ReadonlyArray<{
		label: string;
		value: "__all__" | UserStatus;
	}>;
	const roleFilterOptions = [
		{
			label: t("settings:settings_team_member_role_filter_all"),
			value: "__all__",
		},
		...roleOptions.map((role) => ({
			label: roleLabel(role),
			value: role,
		})),
	] satisfies ReadonlyArray<{
		label: string;
		value: "__all__" | TeamMemberRole;
	}>;
	const teamBaseName = displayTeam?.name ?? "";
	const teamBaseDescription = displayTeam?.description ?? "";
	const currentTeamDraft =
		teamDraft?.teamId === activeTeamId &&
		teamDraft.baseName === teamBaseName &&
		teamDraft.baseDescription === teamBaseDescription
			? teamDraft
			: null;
	const teamName = currentTeamDraft?.name ?? teamBaseName;
	const teamDescription = currentTeamDraft?.description ?? teamBaseDescription;
	const hasMemberFilters =
		memberKeyword.length > 0 ||
		memberRoleFilter !== "__all__" ||
		memberStatusFilter !== "__all__";
	const memberTotalPages = Math.max(
		1,
		Math.ceil(memberTotal / TEAM_MANAGE_MEMBER_PAGE_SIZE),
	);
	const safeMemberOffset =
		memberTotal === 0
			? 0
			: Math.min(
					memberOffset,
					Math.max(0, (memberTotalPages - 1) * TEAM_MANAGE_MEMBER_PAGE_SIZE),
				);
	const memberCurrentPage =
		Math.floor(safeMemberOffset / TEAM_MANAGE_MEMBER_PAGE_SIZE) + 1;
	const auditTotalPages = Math.max(
		1,
		Math.ceil(auditTotal / TEAM_MANAGE_AUDIT_PAGE_SIZE),
	);

	return {
		auditCurrentPage: Math.floor(auditOffset / TEAM_MANAGE_AUDIT_PAGE_SIZE) + 1,
		auditTotalPages,
		hasMemberFilters,
		memberCurrentPage,
		memberFilters,
		memberTotalPages,
		nextAuditPageDisabled:
			auditOffset + TEAM_MANAGE_AUDIT_PAGE_SIZE >= auditTotal,
		nextMemberPageDisabled:
			safeMemberOffset + TEAM_MANAGE_MEMBER_PAGE_SIZE >= memberTotal,
		prevAuditPageDisabled: auditOffset === 0,
		prevMemberPageDisabled: safeMemberOffset === 0,
		quota,
		roleFilterOptions,
		roleOptions,
		safeMemberOffset,
		statusFilterOptions,
		teamBaseDescription,
		teamBaseName,
		teamDescription,
		teamName,
		usagePercentage,
		used,
	};
}
