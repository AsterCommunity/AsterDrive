import { useTranslation } from "react-i18next";
import {
	AdminTable as Table,
	AdminTableBody as TableBody,
	AdminTableCell as TableCell,
	AdminTableHead as TableHead,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { EmptyState } from "@/components/common/EmptyState";
import { SettingsSection } from "@/components/common/SettingsScaffold";
import { SkeletonTable } from "@/components/common/SkeletonTable";
import { UserIdentity } from "@/components/common/UserIdentity";
import { Badge } from "@/components/ui/badge";
import { Icon } from "@/components/ui/icon";
import {
	formatAuditDetail,
	formatAuditSummary,
	formatAuditTarget,
	formatAuditTargetType,
	getAuditActionBadgeClass,
} from "@/lib/audit";
import { formatDateAbsolute, formatDateAbsoluteWithOffset } from "@/lib/format";
import type { AdminOverview } from "@/types/api";

interface OverviewRecentEventsSectionProps {
	loading: boolean;
	overview: AdminOverview | null;
}

export function OverviewRecentEventsSection({
	loading,
	overview,
}: OverviewRecentEventsSectionProps) {
	const { t } = useTranslation("admin");

	return (
		<SettingsSection
			title={t("overview_recent_events")}
			description={t("overview_recent_events_desc")}
			className="min-w-0"
		>
			{loading && !overview ? (
				<SkeletonTable frameless columns={4} rows={8} />
			) : overview?.recent_events.length ? (
				<Table frameless className="min-w-[760px] table-fixed">
					<TableHeader>
						<TableRow>
							<TableHead className="w-[180px]">{t("audit_time")}</TableHead>
							<TableHead className="w-[200px]">{t("audit_action")}</TableHead>
							<TableHead className="w-[180px]">{t("audit_user")}</TableHead>
							<TableHead>{t("audit_entity")}</TableHead>
						</TableRow>
					</TableHeader>
					<TableBody>
						{overview.recent_events.map((event) => {
							const detail = formatAuditDetail(t, event);

							return (
								<TableRow key={event.id}>
									<TableCell
										className="text-xs text-muted-foreground whitespace-nowrap"
										title={formatDateAbsoluteWithOffset(event.created_at)}
									>
										{formatDateAbsolute(event.created_at)}
									</TableCell>
									<TableCell className="max-w-0">
										<Badge
											variant="outline"
											className={getAuditActionBadgeClass(event.action)}
										>
											{formatAuditSummary(t, event)}
										</Badge>
									</TableCell>
									<TableCell className="max-w-0">
										<UserIdentity user={event.user} />
									</TableCell>
									<TableCell className="max-w-0">
										<div className="flex min-w-0 flex-col gap-0.5">
											<span className="truncate text-sm">
												{formatAuditTarget(t, event)}
											</span>
											<span className="text-xs text-muted-foreground">
												{formatAuditTargetType(t, event)}
											</span>
											{detail ? (
												<span className="truncate text-xs text-muted-foreground/80">
													{detail}
												</span>
											) : null}
										</div>
									</TableCell>
								</TableRow>
							);
						})}
					</TableBody>
				</Table>
			) : (
				<EmptyState
					icon={<Icon name="Scroll" className="size-10" />}
					title={t("overview_recent_events_empty")}
					description={t("overview_recent_events_empty_desc")}
				/>
			)}
		</SettingsSection>
	);
}
