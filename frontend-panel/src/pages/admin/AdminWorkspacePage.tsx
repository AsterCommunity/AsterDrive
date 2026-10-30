import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Navigate, useParams } from "react-router-dom";
import { AdminWorkspaceBrowser } from "@/components/admin/content-access/AdminWorkspaceBrowser";
import { AppLayout } from "@/components/layout/AppLayout";
import { handleApiError } from "@/hooks/useApiError";
import { usePageTitle } from "@/hooks/usePageTitle";
import { getUserDisplayName } from "@/lib/user";
import { adminTeamService, adminUserService } from "@/services/adminService";
import type { AdminTeamInfo, UserInfo } from "@/types/api";

export default function AdminWorkspacePage() {
	const { userId, teamId } = useParams<{ userId?: string; teamId?: string }>();
	const { t } = useTranslation("admin");
	const id = Number(userId ?? teamId);
	const valid = Number.isSafeInteger(id) && id > 0;
	const [target, setTarget] = useState<{
		user?: UserInfo;
		team?: AdminTeamInfo;
	} | null>(null);
	const [failed, setFailed] = useState(false);
	usePageTitle(
		target?.user
			? getUserDisplayName(target.user)
			: (target?.team?.name ?? t("content_workspace")),
	);
	useEffect(() => {
		if (!valid) return;
		let active = true;
		setTarget(null);
		setFailed(false);
		const request = userId
			? adminUserService.get(id).then((user) => ({ user }))
			: adminTeamService.get(id).then((team) => ({ team }));
		request
			.then((value) => {
				if (active) setTarget(value);
			})
			.catch((error) => {
				if (active) {
					setFailed(true);
					handleApiError(error);
				}
			});
		return () => {
			active = false;
		};
	}, [valid, id, userId]);
	if (!valid)
		return <Navigate to={userId ? "/admin/users" : "/admin/teams"} replace />;
	if (!target)
		return (
			<AppLayout
				sidebarContent={
					<div className="p-4">{t("content_workspace_read_only_short")}</div>
				}
			>
				<p className="p-5" role={failed ? "alert" : undefined}>
					{t(failed ? "content_workspace_unavailable" : "core:loading")}
				</p>
			</AppLayout>
		);
	return (
		<AdminWorkspaceBrowser
			key={`${userId ? "personal" : "team"}:${id}`}
			scopeKind={userId ? "personal" : "team"}
			targetId={id}
			{...target}
		/>
	);
}
