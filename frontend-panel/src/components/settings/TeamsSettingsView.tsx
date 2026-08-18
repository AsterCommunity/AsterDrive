import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { EmptyState } from "@/components/common/EmptyState";
import { SettingsSection } from "@/components/common/SettingsScaffold";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { formatBytes, formatDateShort } from "@/lib/format";
import { getTeamRoleBadgeClass, isTeamManager } from "@/lib/team";
import { getUserDisplayName } from "@/lib/user";
import { teamService } from "@/services/teamService";
import { useAuthStore } from "@/stores/authStore";
import { useTeamStore } from "@/stores/teamStore";
import type { TeamInfo, TeamMemberRole } from "@/types/api";

export function TeamsSettingsView() {
	const { t } = useTranslation(["core", "settings"]);
	const navigate = useNavigate();
	const user = useAuthStore((state) => state.user);
	const teams = useTeamStore((state) => state.teams);
	const loadingTeams = useTeamStore((state) => state.loading);
	const ensureTeamsLoaded = useTeamStore((state) => state.ensureLoaded);
	const reloadTeams = useTeamStore((state) => state.reload);
	const [archivedTeams, setArchivedTeams] = useState<TeamInfo[]>([]);
	const [archivedLoading, setArchivedLoading] = useState(false);
	const [restoringTeamId, setRestoringTeamId] = useState<number | null>(null);

	const roleLabel = (role: TeamMemberRole) =>
		t(`settings:settings_team_role_${role}`);

	const loadArchivedTeams = useCallback(async () => {
		if (user?.id == null) {
			setArchivedTeams([]);
			return;
		}

		setArchivedLoading(true);
		try {
			setArchivedTeams(await teamService.list({ archived: true }));
		} catch (error) {
			handleApiError(error);
		} finally {
			setArchivedLoading(false);
		}
	}, [user?.id]);

	useEffect(() => {
		void ensureTeamsLoaded(user?.id ?? null).catch(() => undefined);
		void loadArchivedTeams();
	}, [ensureTeamsLoaded, loadArchivedTeams, user?.id]);

	const handleRestoreTeam = async (teamId: number) => {
		try {
			setRestoringTeamId(teamId);
			const restored = await teamService.restore(teamId);
			await Promise.all([reloadTeams(user?.id ?? null), loadArchivedTeams()]);
			navigate(`/settings/teams/${restored.id}/overview`, {
				viewTransition: false,
			});
			toast.success(t("settings:settings_team_restored"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setRestoringTeamId(null);
		}
	};

	return (
		<div className="flex flex-col gap-10">
			<SettingsSection
				title={t("settings:settings_teams")}
				description={t("settings:settings_teams_desc")}
			>
				{loadingTeams && teams.length === 0 ? (
					<div className="py-10 text-center text-sm text-muted-foreground">
						{t("core:loading")}
					</div>
				) : teams.length === 0 ? (
					<EmptyState
						icon={<Icon name="Cloud" className="size-10" />}
						title={t("settings:settings_teams_empty_title")}
						description={t("settings:settings_teams_empty_desc")}
					/>
				) : (
					<div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
						{teams.map((team) => (
							<div
								key={team.id}
								className="rounded-xl bg-card p-4 shadow-xs transition-colors dark:shadow-none"
							>
								<div className="flex items-start justify-between gap-3">
									<div className="space-y-1">
										<p className="font-semibold">{team.name}</p>
										{team.description ? (
											<p className="text-sm text-muted-foreground">
												{team.description}
											</p>
										) : null}
									</div>
									<Badge className={getTeamRoleBadgeClass(team.my_role)}>
										{roleLabel(team.my_role)}
									</Badge>
								</div>
								<div className="mt-4 space-y-2 text-sm text-muted-foreground">
									<div className="flex items-center justify-between gap-3">
										<span>{t("settings:settings_team_members_count")}</span>
										<span>{team.member_count}</span>
									</div>
									<div className="flex items-center justify-between gap-3">
										<span>{t("settings:settings_team_created_by")}</span>
										<span className="truncate">
											{team.created_by
												? getUserDisplayName(team.created_by)
												: "-"}
										</span>
									</div>
									<div className="flex items-center justify-between gap-3">
										<span>{t("settings:settings_team_quota")}</span>
										<span>
											{formatBytes(team.storage_used)}
											{team.storage_quota > 0
												? ` / ${formatBytes(team.storage_quota)}`
												: ` / ${t("core:unlimited")}`}
										</span>
									</div>
								</div>
								<div className="mt-4 flex gap-2">
									<Button
										type="button"
										onClick={() =>
											navigate(`/settings/teams/${team.id}/overview`, {
												viewTransition: false,
											})
										}
									>
										{t("core:manage")}
									</Button>
									<Button
										type="button"
										variant="ghost"
										onClick={() =>
											navigate(`/teams/${team.id}`, {
												viewTransition: false,
											})
										}
									>
										{t("settings:settings_team_open_workspace")}
									</Button>
								</div>
							</div>
						))}
					</div>
				)}
			</SettingsSection>

			{archivedLoading || archivedTeams.length > 0 ? (
				<SettingsSection
					title={t("settings:settings_archived_teams")}
					description={t("settings:settings_archived_teams_desc")}
				>
					{archivedLoading && archivedTeams.length === 0 ? (
						<div className="py-10 text-center text-sm text-muted-foreground">
							{t("core:loading")}
						</div>
					) : (
						<div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
							{archivedTeams.map((team) => (
								<div
									key={team.id}
									className="rounded-xl bg-muted/40 p-4 dark:bg-muted/15"
								>
									<div className="flex items-start justify-between gap-3">
										<div className="space-y-1">
											<p className="font-semibold">{team.name}</p>
											{team.description ? (
												<p className="text-sm text-muted-foreground">
													{team.description}
												</p>
											) : null}
										</div>
										<div className="flex flex-wrap justify-end gap-2">
											<Badge variant="outline">
												{t("settings:settings_team_archived_badge")}
											</Badge>
											<Badge className={getTeamRoleBadgeClass(team.my_role)}>
												{roleLabel(team.my_role)}
											</Badge>
										</div>
									</div>
									<div className="mt-4 space-y-2 text-sm text-muted-foreground">
										<div className="flex items-center justify-between gap-3">
											<span>{t("settings:settings_team_members_count")}</span>
											<span>{team.member_count}</span>
										</div>
										<div className="flex items-center justify-between gap-3">
											<span>{t("settings:settings_team_created_by")}</span>
											<span className="truncate">
												{team.created_by
													? getUserDisplayName(team.created_by)
													: "-"}
											</span>
										</div>
										<div className="flex items-center justify-between gap-3">
											<span>{t("settings:settings_team_archived_at")}</span>
											<span>
												{team.archived_at
													? formatDateShort(team.archived_at)
													: "-"}
											</span>
										</div>
									</div>
									{isTeamManager(team.my_role) ? (
										<div className="mt-4">
											<Button
												type="button"
												variant="outline"
												disabled={restoringTeamId === team.id}
												onClick={() => void handleRestoreTeam(team.id)}
											>
												{restoringTeamId === team.id ? (
													<Icon
														name="Spinner"
														className="mr-1 size-4 animate-spin"
													/>
												) : (
													<Icon
														name="ArrowCounterClockwise"
														className="mr-1 size-4"
													/>
												)}
												{t("settings:settings_team_restore")}
											</Button>
										</div>
									) : null}
								</div>
							))}
						</div>
					)}
				</SettingsSection>
			) : null}
		</div>
	);
}
