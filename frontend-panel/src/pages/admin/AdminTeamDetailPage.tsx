import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Navigate, useNavigate, useParams } from "react-router-dom";
import {
	AdminTeamDetailDialog,
	type AdminTeamDetailTab,
} from "@/components/admin/AdminTeamDetailDialog";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { handleApiError } from "@/hooks/useApiError";
import { usePageTitle } from "@/hooks/usePageTitle";
import {
	loadAdminPolicyGroupLookup,
	readAdminPolicyGroupLookup,
} from "@/lib/adminPolicyGroupLookup";
import type { StoragePolicyGroup } from "@/types/api";

function isAdminTeamDetailTab(
	value: string | undefined,
): value is AdminTeamDetailTab {
	return (
		value === "overview" ||
		value === "members" ||
		value === "audit" ||
		value === "danger"
	);
}

function getAdminTeamDetailSectionTitle(
	section: AdminTeamDetailTab,
	t: ReturnType<typeof useTranslation>["t"],
) {
	switch (section) {
		case "members":
			return t("settings:settings_team_members");
		case "audit":
			return t("settings:settings_team_audit_title");
		case "danger":
			return t("settings:settings_team_danger_zone");
		default:
			return t("settings:settings_team_overview");
	}
}

export default function AdminTeamDetailPage() {
	const { t } = useTranslation(["admin", "settings"]);
	const navigate = useNavigate();
	const { teamId, section } = useParams<{
		teamId?: string;
		section?: string;
	}>();
	const parsedTeamId = Number(teamId);
	const initialPolicyGroups = readAdminPolicyGroupLookup();
	const [policyGroups, setPolicyGroups] = useState<StoragePolicyGroup[]>(
		initialPolicyGroups ?? [],
	);
	const [policyGroupsLoading, setPolicyGroupsLoading] = useState(
		initialPolicyGroups == null,
	);
	const validatedSection = isAdminTeamDetailTab(section) ? section : "overview";
	usePageTitle(
		`${t("teams")} · ${getAdminTeamDetailSectionTitle(validatedSection, t)}`,
	);

	const loadPolicyGroups = useCallback(
		async (options?: { force?: boolean }) => {
			try {
				const cachedPolicyGroups = readAdminPolicyGroupLookup();
				if (!options?.force && cachedPolicyGroups != null) {
					setPolicyGroups(cachedPolicyGroups);
					setPolicyGroupsLoading(false);
				} else {
					setPolicyGroupsLoading(true);
				}
				setPolicyGroups(await loadAdminPolicyGroupLookup(options));
			} catch (error) {
				handleApiError(error);
			} finally {
				setPolicyGroupsLoading(false);
			}
		},
		[],
	);

	useEffect(() => {
		void loadPolicyGroups();
	}, [loadPolicyGroups]);

	if (!Number.isSafeInteger(parsedTeamId) || parsedTeamId <= 0) {
		return <Navigate to="/admin/teams" replace />;
	}

	if (section == null) {
		return <Navigate to={`/admin/teams/${parsedTeamId}/overview`} replace />;
	}

	if (!isAdminTeamDetailTab(section)) {
		return <Navigate to={`/admin/teams/${parsedTeamId}/overview`} replace />;
	}

	return (
		<AdminLayout>
			<AdminPageShell className="overflow-hidden">
				<AdminTeamDetailDialog
					layout="page"
					open
					teamId={parsedTeamId}
					policyGroups={policyGroups}
					policyGroupsLoading={policyGroupsLoading}
					onListChange={async () => undefined}
					onOpenChange={(open) => {
						if (!open) {
							navigate("/admin/teams", { viewTransition: false });
						}
					}}
					onPageTabChange={(tab, options) => {
						navigate(`/admin/teams/${parsedTeamId}/${tab}`, {
							replace: options?.replace,
							viewTransition: false,
						});
					}}
					onRefreshPolicyGroups={() => loadPolicyGroups({ force: true })}
					pageTab={section}
				/>
			</AdminPageShell>
		</AdminLayout>
	);
}
