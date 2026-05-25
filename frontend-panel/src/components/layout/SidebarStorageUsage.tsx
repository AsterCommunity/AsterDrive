import { useTranslation } from "react-i18next";
import { Progress } from "@/components/ui/progress";
import { Separator } from "@/components/ui/separator";
import { formatBytes } from "@/lib/format";
import { isTeamWorkspace } from "@/lib/workspace";
import type { SidebarContentProps } from "./sidebarTypes";

type SidebarStorageUsageProps = Pick<
	SidebarContentProps,
	"activeTeam" | "storageQuota" | "storageUsed" | "user" | "workspace"
>;

export function SidebarStorageUsage({
	activeTeam,
	storageQuota,
	storageUsed,
	user,
	workspace,
}: SidebarStorageUsageProps) {
	const { t } = useTranslation();

	if (!user || (isTeamWorkspace(workspace) && !activeTeam)) {
		return null;
	}

	return (
		<>
			<Separator />
			<div className="shrink-0 space-y-1.5 px-3 pt-3 pb-[calc(0.75rem+env(safe-area-inset-bottom))] md:pb-3">
				<p className="text-xs font-medium text-muted-foreground">
					{activeTeam ? activeTeam.name : t("files:storage_space")}
				</p>
				<Progress
					value={
						storageQuota > 0
							? Math.min((storageUsed / storageQuota) * 100, 100)
							: 0
					}
					className="h-1.5"
				/>
				<p className="text-xs text-muted-foreground">
					{storageQuota > 0
						? t("files:storage_quota", {
								used: formatBytes(storageUsed),
								quota: formatBytes(storageQuota),
							})
						: t("files:storage_used", {
								used: formatBytes(storageUsed),
							})}
				</p>
			</div>
		</>
	);
}
