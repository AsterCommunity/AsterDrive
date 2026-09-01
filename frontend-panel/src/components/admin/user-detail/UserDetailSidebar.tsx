import { useTranslation } from "react-i18next";
import { UserAvatarImage } from "@/components/common/UserAvatarImage";
import {
	getRoleBadgeClass,
	getStatusBadgeClass,
} from "@/components/common/userBadgeClasses";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { formatBytes, formatDateAbsolute } from "@/lib/format";
import { getNormalizedDisplayName, getUserDisplayName } from "@/lib/user";
import type { UserInfo } from "@/types/api";

interface UserDetailSidebarProps {
	quota: number;
	usagePercentage: number;
	used: number;
	user: UserInfo;
}

export function UserDetailSidebar({
	quota,
	usagePercentage,
	used,
	user,
}: UserDetailSidebarProps) {
	const { t } = useTranslation(["admin", "core"]);
	const displayName = getUserDisplayName(user);
	const showUsernameSecondary =
		getNormalizedDisplayName(user.profile.display_name) !== null &&
		displayName !== user.username;

	return (
		<aside className="border-b lg:min-h-0 lg:w-80 lg:flex-none lg:overflow-y-auto lg:border-b-0">
			<div className="space-y-5 p-6 max-lg:space-y-4 max-lg:p-4">
				<div className="flex flex-col gap-y-3 max-lg:flex-row max-lg:items-start max-lg:gap-3">
					<UserAvatarImage
						avatar={user.profile.avatar}
						name={displayName}
						size="xl"
						className="aspect-square w-full max-w-[220px] max-lg:size-20 max-lg:max-w-none max-lg:rounded-xl max-lg:text-2xl"
					/>
					<div className="space-y-3 max-lg:min-w-0 max-lg:flex-1">
						<div className="space-y-1">
							<h3 className="text-lg font-semibold text-foreground">
								{displayName}
							</h3>
							{showUsernameSecondary ? (
								<p className="text-sm text-muted-foreground">
									@{user.username}
								</p>
							) : null}
							<p className="text-sm text-muted-foreground max-lg:break-all">
								{user.email}
							</p>
						</div>
						<div className="flex flex-wrap gap-2">
							<Badge variant="outline" className={getRoleBadgeClass(user.role)}>
								{user.role === "admin" ? "Admin" : "User"}
							</Badge>
							<Badge
								variant="outline"
								className={getStatusBadgeClass(user.status)}
							>
								{user.status === "active"
									? t("core:active")
									: t("core:disabled_status")}
							</Badge>
						</div>
					</div>
				</div>

				<div className="grid gap-y-3 max-lg:grid-cols-2 max-lg:gap-3">
					<div className="space-y-3 border-t pt-5 max-lg:space-y-2">
						<div className="space-y-1">
							<p className="text-xs uppercase tracking-wide text-muted-foreground">
								ID
							</p>
							<p className="font-mono text-sm text-foreground">{user.id}</p>
						</div>
						<div className="space-y-1">
							<p className="text-xs uppercase tracking-wide text-muted-foreground">
								{t("core:created_at")}
							</p>
							<p className="text-sm text-foreground">
								{formatDateAbsolute(user.created_at)}
							</p>
						</div>
					</div>

					<div className="space-y-3 border-t pt-5 max-lg:space-y-2">
						<div>
							<p className="text-sm font-medium text-foreground">
								{t("storage")}
							</p>
							<p className="text-xs text-muted-foreground">
								{formatBytes(used)}
								{quota > 0
									? ` / ${formatBytes(quota)}`
									: ` / ${t("core:unlimited")}`}
							</p>
						</div>
						{quota > 0 ? (
							<Progress value={usagePercentage} className="h-2" />
						) : null}
					</div>
				</div>
			</div>
		</aside>
	);
}
