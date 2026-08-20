import { useTranslation } from "react-i18next";
import { EmptyState } from "@/components/common/EmptyState";
import { SettingsSection } from "@/components/common/SettingsScaffold";
import { Icon, type IconName } from "@/components/ui/icon";
import { Skeleton } from "@/components/ui/skeleton";
import { formatBytes } from "@/lib/format";
import type { AdminOverview } from "@/types/api";
import { COUNT_FORMATTER } from "./overviewPresentation";

interface StatBlockProps {
	icon: IconName;
	label: string;
	value: string;
}

// D9 层级重建：数字是主角（3xl tabular-nums），图标并入 label 行——
// 独立的 icon 井在亮色下是「白上灰」亮度倒挂，且不构成信息。
function StatBlock({ icon, label, value }: StatBlockProps) {
	return (
		<div className="rounded-xl bg-muted/30 p-4">
			<div className="flex items-center gap-1.5 text-xs text-muted-foreground">
				<Icon name={icon} className="size-3.5" />
				{label}
			</div>
			<p className="mt-2 text-3xl font-semibold tracking-tight tabular-nums">
				{value}
			</p>
		</div>
	);
}

function StatBlockSkeleton() {
	return (
		<div className="rounded-xl bg-muted/30 p-4">
			<Skeleton className="h-3.5 w-24" />
			<Skeleton className="mt-3 h-8 w-20" />
		</div>
	);
}

interface OverviewStatsSectionProps {
	loading: boolean;
	overview: AdminOverview | null;
}

export function OverviewStatsSection({
	loading,
	overview,
}: OverviewStatsSectionProps) {
	const { t } = useTranslation("admin");
	const stats = overview?.stats;
	const statBlocks = stats
		? [
				{
					label: t("overview_total_users"),
					value: COUNT_FORMATTER.format(stats.total_users),
					icon: "Shield" as const,
				},
				{
					label: t("overview_total_files"),
					value: COUNT_FORMATTER.format(stats.total_files),
					icon: "File" as const,
				},
				{
					label: t("overview_total_blobs"),
					value: COUNT_FORMATTER.format(stats.total_blobs),
					icon: "HardDrive" as const,
				},
				{
					label: t("overview_total_shares"),
					value: COUNT_FORMATTER.format(stats.total_shares),
					icon: "Link" as const,
				},
				{
					label: t("overview_total_file_bytes"),
					value: formatBytes(Math.max(stats.total_file_bytes, 0)),
					icon: "Cloud" as const,
				},
				{
					label: t("overview_total_blob_bytes"),
					value: formatBytes(Math.max(stats.total_blob_bytes, 0)),
					icon: "Cloud" as const,
				},
			]
		: [];

	return (
		<SettingsSection
			title={t("overview_summary")}
			description={t("overview_summary_desc")}
			className="min-w-0"
		>
			{loading && !overview ? (
				<div className="grid gap-3 sm:grid-cols-2 md:grid-cols-3 xl:grid-cols-6">
					{Array.from({ length: 6 }).map((_, index) => (
						<StatBlockSkeleton
							// biome-ignore lint/suspicious/noArrayIndexKey: static loading placeholders
							key={`overview-stat-skeleton-${index}`}
						/>
					))}
				</div>
			) : overview ? (
				<div className="grid gap-3 sm:grid-cols-2 md:grid-cols-3 xl:grid-cols-6">
					{statBlocks.map((block) => (
						<StatBlock
							key={block.label}
							label={block.label}
							value={block.value}
							icon={block.icon}
						/>
					))}
				</div>
			) : (
				<EmptyState
					icon={<Icon name="Presentation" className="size-10" />}
					title={t("overview_empty_title")}
					description={t("overview_empty_desc")}
				/>
			)}
		</SettingsSection>
	);
}
