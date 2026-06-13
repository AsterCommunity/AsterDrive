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
import { SkeletonTable } from "@/components/common/SkeletonTable";
import { UserIdentity } from "@/components/common/UserIdentity";
import { AdminSurface } from "@/components/layout/AdminSurface";
import { Badge } from "@/components/ui/badge";
import { Icon } from "@/components/ui/icon";
import {
	formatAuditDetail,
	formatAuditSummary,
	formatAuditTarget,
	formatAuditTargetType,
	getAuditActionBadgeClass,
} from "@/lib/audit";
import { PAGE_SECTION_PADDING_CLASS } from "@/lib/constants";
import { formatDateAbsolute, formatDateAbsoluteWithOffset } from "@/lib/format";
import { cn } from "@/lib/utils";
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
		<AdminSurface padded={false} className="min-h-0 overflow-hidden">
			<div className={cn("border-b py-4", PAGE_SECTION_PADDING_CLASS)}>
				<h3 className="text-base font-semibold">
					{t("overview_recent_events")}
				</h3>
				<p className="mt-1 text-sm text-muted-foreground">
					{t("overview_recent_events_desc")}
				</p>
			</div>

			{loading && !overview ? (
				<div className="py-4 md:py-6">
					<SkeletonTable columns={4} rows={8} />
				</div>
			) : overview?.recent_events.length ? (
				<Table>
					<TableHeader>
						<TableRow>
							<TableHead>{t("audit_time")}</TableHead>
							<TableHead>{t("audit_action")}</TableHead>
							<TableHead>{t("audit_user")}</TableHead>
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
									<TableCell>
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
									<TableCell>
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
		</AdminSurface>
	);
}
