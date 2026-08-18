import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Navigate, useNavigate, useParams } from "react-router-dom";
import { AppLayout } from "@/components/layout/AppLayout";
import { TeamManageDetail } from "@/components/settings/team-manage-detail/TeamManageDetail";
import type { TeamManageTab } from "@/components/settings/team-manage-detail/types";
import { usePageTitle } from "@/hooks/usePageTitle";
import { useAuthStore } from "@/stores/authStore";
import { useTeamStore } from "@/stores/teamStore";

function isTeamManageTab(value: string | undefined): value is TeamManageTab {
	return (
		value === "overview" ||
		value === "members" ||
		value === "webdav" ||
		value === "audit" ||
		value === "danger"
	);
}

function getTeamManageSectionTitle(
	section: TeamManageTab,
	t: ReturnType<typeof useTranslation>["t"],
) {
	switch (section) {
		case "members":
			return t("settings:settings_team_members");
		case "webdav":
			return t("settings:settings_team_webdav_title");
		case "audit":
			return t("settings:settings_team_audit_title");
		case "danger":
			return t("settings:settings_team_danger_zone");
		default:
			return t("settings:settings_team_overview");
	}
}

export default function TeamManagePage() {
	const { t } = useTranslation("settings");
	const navigate = useNavigate();
	const { teamId, section } = useParams<{
		teamId?: string;
		section?: string;
	}>();
	const parsedTeamId = Number(teamId);
	const user = useAuthStore((state) => state.user);
	const ensureTeamsLoaded = useTeamStore((state) => state.ensureLoaded);
	const reloadTeams = useTeamStore((state) => state.reload);
	const teams = useTeamStore((state) => state.teams);
	const validatedSection = isTeamManageTab(section) ? section : "overview";
	const teamSummary = teams.find((team) => team.id === parsedTeamId) ?? null;
	const pageTitle = [
		teamSummary?.name ?? t("settings_team_manage_title"),
		getTeamManageSectionTitle(validatedSection, t),
	].join(" · ");
	usePageTitle(pageTitle);

	useEffect(() => {
		void ensureTeamsLoaded(user?.id ?? null).catch(() => undefined);
	}, [ensureTeamsLoaded, user?.id]);

	if (!Number.isSafeInteger(parsedTeamId) || parsedTeamId <= 0) {
		return <Navigate to="/settings/teams" replace />;
	}

	if (section == null) {
		return <Navigate to={`/settings/teams/${parsedTeamId}/overview`} replace />;
	}

	if (!isTeamManageTab(section)) {
		return <Navigate to={`/settings/teams/${parsedTeamId}/overview`} replace />;
	}

	return (
		<AppLayout>
			<div className="flex min-h-0 flex-1 flex-col overflow-hidden">
				<div className="mx-auto flex min-h-0 w-full max-w-7xl flex-1 flex-col py-4 md:py-6">
					<TeamManageDetail
						currentUserId={user?.id ?? null}
						onExit={() =>
							navigate("/settings/teams", { viewTransition: false })
						}
						onPageTabChange={(tab, options) => {
							navigate(`/settings/teams/${parsedTeamId}/${tab}`, {
								replace: options?.replace,
								viewTransition: false,
							});
						}}
						onTeamsReload={() => reloadTeams(user?.id ?? null)}
						pageTab={section}
						teamId={parsedTeamId}
						teamSummary={teamSummary}
					/>
				</div>
			</div>
		</AppLayout>
	);
}
