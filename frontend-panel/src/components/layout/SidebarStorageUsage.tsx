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

	const usedLabel = formatBytes(storageUsed);
	const quotaLabel =
		storageQuota > 0 ? formatBytes(storageQuota) : t("core:unlimited");

	return (
		<>
			<Separator />
			<div className="shrink-0 space-y-1.5 px-3 pt-3 pb-[calc(0.75rem+env(safe-area-inset-bottom))] md:pb-3">
				<p
					className="text-xs font-medium text-muted-foreground"
					data-testid="user-sidebar-storage-title"
				>
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
					{t("files:storage_quota", {
						used: usedLabel,
						quota: quotaLabel,
					})}
				</p>
			</div>
		</>
	);
}
