import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Icon } from "@/components/ui/icon";
import type { MeResponse } from "@/types/api";

interface SecuritySummaryCardProps {
	sessionCount: number;
	user: MeResponse | null;
}

export function SecuritySummaryCard({
	sessionCount,
	user,
}: SecuritySummaryCardProps) {
	const { t } = useTranslation(["auth", "core", "settings"]);
	const emailVerified = !!user?.email_verified;

	return (
		<div className="rounded-lg bg-muted/20 px-3 py-2">
			<div className="grid divide-y md:grid-cols-3 md:divide-x md:divide-y-0">
				<div className="flex min-w-0 items-center gap-2 py-2 md:px-3 md:first:pl-0">
					<div className="rounded-md bg-primary/10 p-1.5 text-primary">
						<Icon name="Shield" className="size-4" />
					</div>
					<div className="min-w-0">
						<p className="text-xs font-medium text-muted-foreground">
							{t("settings:settings_security_account")}
						</p>
						<p className="truncate text-sm font-semibold">
							@{user?.username ?? ""}
						</p>
					</div>
				</div>

				<div className="flex min-w-0 items-center justify-between gap-3 py-2 md:px-3">
					<div className="flex min-w-0 items-center gap-2">
						<div className="rounded-md bg-secondary p-1.5 text-secondary-foreground">
							<Icon name="EnvelopeSimple" className="size-4" />
						</div>
						<div className="min-w-0">
							<p className="text-xs font-medium text-muted-foreground">
								{t("settings:settings_email_summary")}
							</p>
							<p className="truncate text-sm font-semibold">
								{user?.email ?? ""}
							</p>
						</div>
					</div>
					<Badge
						variant="outline"
						className={
							emailVerified
								? "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
								: "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300"
						}
					>
						{emailVerified
							? t("settings:settings_email_verified")
							: t("settings:settings_email_unverified")}
					</Badge>
				</div>

				<div className="flex min-w-0 items-center gap-2 py-2 md:px-3 md:last:pr-0">
					<div className="rounded-md bg-secondary p-1.5 text-secondary-foreground">
						<Icon name="Monitor" className="size-4" />
					</div>
					<div className="min-w-0">
						<p className="text-xs font-medium text-muted-foreground">
							{t("settings:settings_sessions_section")}
						</p>
						<p className="text-sm font-semibold">
							{t("settings:settings_security_session_count", {
								count: sessionCount,
							})}
						</p>
					</div>
				</div>
			</div>
		</div>
	);
}
