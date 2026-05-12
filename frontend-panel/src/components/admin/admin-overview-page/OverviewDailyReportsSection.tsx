import { useTranslation } from "react-i18next";
import {
	AdminTable as Table,
	AdminTableBody as TableBody,
	AdminTableCell as TableCell,
	AdminTableHead as TableHead,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { SkeletonTable } from "@/components/common/SkeletonTable";
import { AdminSurface } from "@/components/layout/AdminSurface";
import { PAGE_SECTION_PADDING_CLASS } from "@/lib/constants";
import { cn } from "@/lib/utils";
import type { AdminOverview } from "@/types/api";

interface OverviewDailyReportsSectionProps {
	defaultDays: number;
	loading: boolean;
	overview: AdminOverview | null;
}

export function OverviewDailyReportsSection({
	defaultDays,
	loading,
	overview,
}: OverviewDailyReportsSectionProps) {
	const { t } = useTranslation("admin");

	return (
		<AdminSurface padded={false} className="flex-none min-h-0 overflow-hidden">
			<div className={cn("border-b py-4", PAGE_SECTION_PADDING_CLASS)}>
				<h3 className="text-base font-semibold">
					{t("overview_daily_reports")}
				</h3>
				<p className="mt-1 text-sm text-muted-foreground">
					{t("overview_daily_reports_desc", {
						days: overview?.days ?? defaultDays,
					})}
				</p>
			</div>

			{loading && !overview ? (
				<div className="py-4 md:py-6">
					<SkeletonTable columns={7} rows={7} />
				</div>
			) : (
				<Table>
					<TableHeader>
						<TableRow>
							<TableHead>{t("overview_report_date")}</TableHead>
							<TableHead>{t("overview_report_sign_ins")}</TableHead>
							<TableHead>{t("overview_report_new_users")}</TableHead>
							<TableHead>{t("overview_report_uploads")}</TableHead>
							<TableHead>{t("overview_report_shares")}</TableHead>
							<TableHead>{t("overview_report_deletions")}</TableHead>
							<TableHead>{t("overview_report_total_events")}</TableHead>
						</TableRow>
					</TableHeader>
					<TableBody>
						{overview?.daily_reports.map((report) => (
							<TableRow key={report.date}>
								<TableCell className="font-medium">{report.date}</TableCell>
								<TableCell>{report.sign_ins}</TableCell>
								<TableCell>{report.new_users}</TableCell>
								<TableCell>{report.uploads}</TableCell>
								<TableCell>{report.share_creations}</TableCell>
								<TableCell>{report.deletions}</TableCell>
								<TableCell>{report.total_events}</TableCell>
							</TableRow>
						))}
					</TableBody>
				</Table>
			)}
		</AdminSurface>
	);
}
