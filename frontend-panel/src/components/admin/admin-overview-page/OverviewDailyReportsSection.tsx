import { useTranslation } from "react-i18next";
import {
	AdminTable as Table,
	AdminTableBody as TableBody,
	AdminTableCell as TableCell,
	AdminTableHead as TableHead,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { SettingsSection } from "@/components/common/SettingsScaffold";
import { SkeletonTable } from "@/components/common/SkeletonTable";
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
		<SettingsSection
			title={t("overview_daily_reports")}
			description={t("overview_daily_reports_desc", {
				days: overview?.days ?? defaultDays,
			})}
			className="min-w-0"
		>
			{loading && !overview ? (
				<SkeletonTable frameless columns={7} rows={7} />
			) : (
				<Table frameless>
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
		</SettingsSection>
	);
}
