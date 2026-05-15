import {
	CartesianGrid,
	Line,
	LineChart,
	ResponsiveContainer,
	Tooltip,
	type TooltipContentProps,
	XAxis,
	YAxis,
} from "recharts";
import { EmptyState } from "@/components/common/EmptyState";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription } from "@/components/ui/card";
import { Icon } from "@/components/ui/icon";
import { cn } from "@/lib/utils";
import type { AdminOverview } from "@/types/api";

const COUNT_FORMATTER = new Intl.NumberFormat();
const DECIMAL_FORMATTER = new Intl.NumberFormat(undefined, {
	maximumFractionDigits: 1,
});
const RESPONSIVE_CONTAINER_RESIZE_DEBOUNCE_MS = 120;

type DailyReport = AdminOverview["daily_reports"][number];
type TrendSeriesKey = "newUsers" | "shareCreations" | "uploads";

interface TrendPoint {
	date: string;
	label: string;
	newUsers: number;
	shareCreations: number;
	uploads: number;
}

export interface TrendSeries {
	badgeClass: string;
	key: TrendSeriesKey;
	label: string;
	stroke: string;
	strokeWidth: number;
}

interface OverviewTrendChartProps {
	reports: DailyReport[];
	emptyTitle: string;
	emptyDescription: string;
	averageLabel: string;
	latestLabel: string;
	peakLabel: string;
	series: TrendSeries[];
}

function formatTrendDayLabel(date: string) {
	const [year, month, day] = date.split("-");
	if (!year || !month || !day) return date;
	return `${Number(month)}/${Number(day)}`;
}

function sortReportsByDateAscending(reports: DailyReport[]) {
	return [...reports].sort((left, right) =>
		left.date.localeCompare(right.date),
	);
}

function createTrendData(reports: DailyReport[]): TrendPoint[] {
	return reports.map((report) => ({
		date: report.date,
		label: formatTrendDayLabel(report.date),
		newUsers: report.new_users,
		shareCreations: report.share_creations,
		uploads: report.uploads,
	}));
}

function resolveTooltipValue(rawValue: unknown) {
	const numericValue = Array.isArray(rawValue)
		? Number(rawValue[0] ?? 0)
		: Number(rawValue ?? 0);

	return Number.isFinite(numericValue) ? numericValue : 0;
}

interface TrendTooltipCardProps extends TooltipContentProps {
	series: TrendSeries[];
}

function TrendTooltipCard({ active, payload, series }: TrendTooltipCardProps) {
	if (!active || !payload?.length) return null;

	const point = payload[0]?.payload as TrendPoint | undefined;

	return (
		<div className="rounded-xl border border-border/70 bg-card/95 px-3 py-2 shadow-lg shadow-black/8 backdrop-blur dark:shadow-none">
			<p className="text-xs text-muted-foreground">{point?.date ?? "---"}</p>
			<div className="mt-2 space-y-1.5">
				{series.map((seriesItem) => {
					const currentPayload = payload.find(
						(entry) => entry.dataKey === seriesItem.key,
					);

					return (
						<div
							key={seriesItem.key}
							className="flex items-center justify-between gap-4 text-xs"
						>
							<div className="flex items-center gap-2 text-muted-foreground">
								<span
									className="inline-flex size-2 rounded-full"
									style={{ backgroundColor: seriesItem.stroke }}
								/>
								<span>{seriesItem.label}</span>
							</div>
							<span className="font-semibold text-foreground">
								{COUNT_FORMATTER.format(
									resolveTooltipValue(currentPayload?.value),
								)}
							</span>
						</div>
					);
				})}
			</div>
		</div>
	);
}

export function OverviewTrendChart({
	reports,
	emptyTitle,
	emptyDescription,
	averageLabel,
	latestLabel,
	peakLabel,
	series,
}: OverviewTrendChartProps) {
	if (!reports.length) {
		return (
			<EmptyState
				icon={<Icon name="Presentation" className="h-10 w-10" />}
				title={emptyTitle}
				description={emptyDescription}
			/>
		);
	}

	const orderedReports = sortReportsByDateAscending(reports);
	const trendData = createTrendData(orderedReports);
	const latestReport = orderedReports[orderedReports.length - 1];
	const totalEvents = orderedReports.reduce(
		(sum, report) => sum + report.total_events,
		0,
	);
	const averageEvents = totalEvents / orderedReports.length;
	const peakReport = orderedReports.reduce((peak, report) =>
		report.total_events > peak.total_events ? report : peak,
	);

	return (
		<div className="grid min-w-0 gap-4 xl:grid-cols-[minmax(0,1fr)_220px]">
			<div className="min-w-0 overflow-hidden rounded-2xl border bg-linear-to-br from-primary/5 via-background to-background p-4">
				<div className="mb-3 flex flex-wrap items-center gap-2">
					{series.map((seriesItem) => (
						<Badge
							key={seriesItem.key}
							variant="outline"
							className={cn("gap-2 border", seriesItem.badgeClass)}
						>
							<span
								className="inline-flex size-2 rounded-full"
								style={{ backgroundColor: seriesItem.stroke }}
							/>
							{seriesItem.label}
						</Badge>
					))}
				</div>
				<div className="h-[280px] min-w-0 min-h-[280px]">
					<ResponsiveContainer
						width="100%"
						height="100%"
						debounce={RESPONSIVE_CONTAINER_RESIZE_DEBOUNCE_MS}
					>
						<LineChart
							data={trendData}
							margin={{ top: 8, right: 8, left: -24, bottom: 0 }}
						>
							<CartesianGrid
								vertical={false}
								stroke="var(--border)"
								strokeDasharray="4 6"
							/>
							<XAxis
								dataKey="label"
								axisLine={false}
								tickLine={false}
								tickMargin={12}
								interval={0}
								minTickGap={0}
								padding={{ left: 12, right: 12 }}
								tick={{ fill: "var(--muted-foreground)", fontSize: 12 }}
							/>
							<YAxis
								allowDecimals={false}
								axisLine={false}
								tickLine={false}
								tickMargin={12}
								width={36}
								tick={{ fill: "var(--muted-foreground)", fontSize: 12 }}
							/>
							<Tooltip
								cursor={{ stroke: "var(--border)", strokeDasharray: "4 6" }}
								content={(props) => (
									<TrendTooltipCard {...props} series={series} />
								)}
							/>
							{series.map((seriesItem) => (
								<Line
									key={seriesItem.key}
									type="monotone"
									dataKey={seriesItem.key}
									name={seriesItem.label}
									stroke={seriesItem.stroke}
									strokeWidth={seriesItem.strokeWidth}
									dot={false}
									activeDot={{
										r: 4,
										fill: "var(--background)",
										stroke: seriesItem.stroke,
										strokeWidth: 2,
									}}
								/>
							))}
						</LineChart>
					</ResponsiveContainer>
				</div>
			</div>

			<div className="grid content-start gap-3 sm:grid-cols-2 xl:grid-cols-1">
				<Card size="sm" className="border-0 shadow-none ring-1 ring-border/70">
					<CardContent className="space-y-1 p-4">
						<CardDescription className="text-xs">
							{averageLabel}
						</CardDescription>
						<p className="text-xl font-semibold tracking-tight">
							{DECIMAL_FORMATTER.format(averageEvents)}
						</p>
					</CardContent>
				</Card>
				<Card size="sm" className="border-0 shadow-none ring-1 ring-border/70">
					<CardContent className="space-y-1 p-4">
						<CardDescription className="text-xs">{latestLabel}</CardDescription>
						<p className="text-xl font-semibold tracking-tight">
							{COUNT_FORMATTER.format(latestReport.total_events)}
						</p>
						<p className="text-xs text-muted-foreground">
							{peakLabel}: {formatTrendDayLabel(peakReport.date)} ·{" "}
							{COUNT_FORMATTER.format(peakReport.total_events)}
						</p>
					</CardContent>
				</Card>
			</div>
		</div>
	);
}
