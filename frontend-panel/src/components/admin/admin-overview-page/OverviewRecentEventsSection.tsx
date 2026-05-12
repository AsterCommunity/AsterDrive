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
import { AdminSurface } from "@/components/layout/AdminSurface";
import { Badge } from "@/components/ui/badge";
import { Icon } from "@/components/ui/icon";
import { formatAuditAction, formatAuditEntityType } from "@/lib/audit";
import { PAGE_SECTION_PADDING_CLASS } from "@/lib/constants";
import { formatDateAbsolute, formatDateAbsoluteWithOffset } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { AdminOverview } from "@/types/api";
import { getOverviewActionBadgeClass } from "./overviewPresentation";

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
						{overview.recent_events.map((event) => (
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
										className={getOverviewActionBadgeClass(event.action)}
									>
										{formatAuditAction(t, event.action)}
									</Badge>
								</TableCell>
								<TableCell className="text-muted-foreground">
									#{event.user_id}
								</TableCell>
								<TableCell>
									<div className="flex flex-col gap-1">
										<span className="text-sm">
											{event.entity_name ??
												formatAuditEntityType(t, event.entity_type)}
										</span>
										<span className="text-xs text-muted-foreground">
											{formatAuditEntityType(t, event.entity_type)}
										</span>
									</div>
								</TableCell>
							</TableRow>
						))}
					</TableBody>
				</Table>
			) : (
				<EmptyState
					icon={<Icon name="Scroll" className="h-10 w-10" />}
					title={t("overview_recent_events_empty")}
					description={t("overview_recent_events_empty_desc")}
				/>
			)}
		</AdminSurface>
	);
}
