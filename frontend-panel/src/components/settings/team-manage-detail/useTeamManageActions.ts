import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { handleApiError } from "@/hooks/useApiError";
import { teamService } from "@/services/teamService";
import type { TeamInfo, TeamMemberRole } from "@/types/api";

interface UseTeamManageActionsArgs {
	canArchiveTeam: boolean;
	canManageTeam: boolean;
	currentUserId: number | null;
	loadAuditEntries: (teamId: number, offset?: number) => Promise<void>;
	loadMembers: (teamId: number, offset?: number) => Promise<void>;
	loadTeamDetail: (teamId: number) => Promise<void>;
	onExit: () => void;
	onTeamsReload: () => Promise<void>;
	safeMemberOffset: number;
	setMemberIdentifier: (identifier: string) => void;
	setMemberOffset: (offset: number) => void;
	setMemberRole: (role: TeamMemberRole) => void;
	teamDetail: TeamInfo | null;
	teamId: number;
}

export function useTeamManageActions({
	canArchiveTeam,
	canManageTeam,
	currentUserId,
	loadAuditEntries,
	loadMembers,
	loadTeamDetail,
	onExit,
	onTeamsReload,
	safeMemberOffset,
	setMemberIdentifier,
	setMemberOffset,
	setMemberRole,
	teamDetail,
	teamId,
}: UseTeamManageActionsArgs) {
	const { t } = useTranslation(["settings"]);
	const [mutating, setMutating] = useState(false);

	const handleUpdateTeam = useCallback(
		async (teamName: string, teamDescription: string) => {
			if (!teamDetail || !canManageTeam) {
				return;
			}

			const nextName = teamName.trim();
			if (!nextName) {
				return;
			}

			try {
				setMutating(true);
				await teamService.update(teamDetail.id, {
					name: nextName,
					description: teamDescription.trim() || undefined,
				});
				await Promise.all([
					loadTeamDetail(teamDetail.id),
					canManageTeam ? loadAuditEntries(teamDetail.id) : Promise.resolve(),
					onTeamsReload(),
				]);
				toast.success(t("settings:settings_team_updated"));
			} catch (error) {
				handleApiError(error);
			} finally {
				setMutating(false);
			}
		},
		[
			canManageTeam,
			loadAuditEntries,
			loadTeamDetail,
			onTeamsReload,
			t,
			teamDetail,
		],
	);

	const handleAddMember = useCallback(
		async (identifier: string, role: TeamMemberRole) => {
			if (!canManageTeam) {
				return;
			}

			const nextIdentifier = identifier.trim();
			if (!nextIdentifier) {
				return;
			}

			try {
				setMutating(true);
				await teamService.addMember(teamId, {
					identifier: nextIdentifier,
					role,
				});
				setMemberIdentifier("");
				setMemberRole("member");
				setMemberOffset(0);
				await Promise.all([
					loadTeamDetail(teamId),
					loadMembers(teamId, 0),
					loadAuditEntries(teamId),
					onTeamsReload(),
				]);
				toast.success(t("settings:settings_team_member_added"));
			} catch (error) {
				handleApiError(error);
			} finally {
				setMutating(false);
			}
		},
		[
			canManageTeam,
			loadAuditEntries,
			loadMembers,
			loadTeamDetail,
			onTeamsReload,
			setMemberIdentifier,
			setMemberOffset,
			setMemberRole,
			t,
			teamId,
		],
	);

	const handleUpdateMemberRole = useCallback(
		async (memberUserId: number, role: TeamMemberRole) => {
			if (!canManageTeam) {
				return;
			}

			try {
				setMutating(true);
				await teamService.updateMember(teamId, memberUserId, { role });
				await Promise.all([
					loadTeamDetail(teamId),
					loadMembers(teamId, safeMemberOffset),
					loadAuditEntries(teamId),
				]);
				toast.success(t("settings:settings_team_member_role_updated"));
			} catch (error) {
				handleApiError(error);
			} finally {
				setMutating(false);
			}
		},
		[
			canManageTeam,
			loadAuditEntries,
			loadMembers,
			loadTeamDetail,
			safeMemberOffset,
			t,
			teamId,
		],
	);

	const handleRemoveMember = useCallback(
		async (memberUserId: number) => {
			const removingSelf = memberUserId === currentUserId;

			try {
				setMutating(true);
				await teamService.removeMember(teamId, memberUserId);
				await onTeamsReload();
				if (removingSelf) {
					onExit();
					toast.success(t("settings:settings_team_left"));
				} else {
					await Promise.all([
						loadTeamDetail(teamId),
						loadMembers(teamId, safeMemberOffset),
						loadAuditEntries(teamId),
					]);
					toast.success(t("settings:settings_team_member_removed"));
				}
			} catch (error) {
				handleApiError(error);
			} finally {
				setMutating(false);
			}
		},
		[
			currentUserId,
			loadAuditEntries,
			loadMembers,
			loadTeamDetail,
			onExit,
			onTeamsReload,
			safeMemberOffset,
			t,
			teamId,
		],
	);

	const handleArchiveTeam = useCallback(async () => {
		if (!canArchiveTeam) {
			return;
		}

		try {
			setMutating(true);
			await teamService.delete(teamId);
			await onTeamsReload();
			toast.success(t("settings:settings_team_deleted"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setMutating(false);
		}
	}, [canArchiveTeam, onTeamsReload, t, teamId]);

	return {
		handleAddMember,
		handleArchiveTeam,
		handleRemoveMember,
		handleUpdateMemberRole,
		handleUpdateTeam,
		mutating,
	};
}
