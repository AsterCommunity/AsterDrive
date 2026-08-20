import { lazy, Suspense } from "react";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import type {
	DailyOverviewReport,
	OverviewTrendSeries,
	TrendSeriesKey,
} from "./OverviewTrendChart";

const COUNT_FORMATTER = new Intl.NumberFormat();
const DECIMAL_FORMATTER = new Intl.NumberFormat(undefined, {
	maximumFractionDigits: 1,
});
const RESPONSIVE_CONTAINER_RESIZE_DEBOUNCE_MS = 120;

const RechartsTrendPlot = lazy(async () => {
	const {
		CartesianGrid,
		Line,
		LineChart,
		ResponsiveContainer,
		Tooltip,
		XAxis,
		YAxis,
	} = await import("recharts");

	function LoadedRechartsTrendPlot({
		series,
		trendData,
	}: {
		series: OverviewTrendSeries[];
		trendData: TrendPoint[];
	}) {
		return (
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
						content={(props) => <TrendTooltipCard {...props} series={series} />}
					/>
					{series.map((seriesItem) => (
						<Line
							key={seriesItem.key}
							type="monotone"
							dataKey={seriesItem.key satisfies TrendSeriesKey}
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
		);
	}

	return { default: LoadedRechartsTrendPlot };
});

interface TrendPoint {
	date: string;
	label: string;
	newUsers: number;
	shareCreations: number;
	uploads: number;
}

export interface OverviewTrendChartContentProps {
	reports: DailyOverviewReport[];
	averageLabel: string;
	latestLabel: string;
	peakLabel: string;
	series: OverviewTrendSeries[];
}

function formatTrendDayLabel(date: string) {
	const [year, month, day] = date.split("-");
	if (!year || !month || !day) return date;
	return `${Number(month)}/${Number(day)}`;
}

function sortReportsByDateAscending(reports: DailyOverviewReport[]) {
	return reports.toSorted((left, right) => left.date.localeCompare(right.date));
}

function createTrendData(reports: DailyOverviewReport[]): TrendPoint[] {
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

interface TrendTooltipCardProps {
	active?: boolean;
	payload?: ReadonlyArray<{
		dataKey?: unknown;
		payload?: unknown;
		value?: unknown;
	}>;
	series: OverviewTrendSeries[];
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

function TrendMetric({ label, value }: { label: string; value: string }) {
	return (
		<div className="flex items-baseline gap-2">
			<span className="text-xs text-muted-foreground">{label}</span>
			<span className="text-xl font-semibold tracking-tight tabular-nums">
				{value}
			</span>
		</div>
	);
}

export function OverviewTrendChartContent({
	reports,
	averageLabel,
	latestLabel,
	peakLabel,
	series,
}: OverviewTrendChartContentProps) {
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
		<div className="min-w-0 space-y-4">
			{/* 指标行在图表上方：先看数再看曲线；图表裸放全宽（D9 去框） */}
			<div className="flex flex-wrap items-baseline gap-x-8 gap-y-2">
				<TrendMetric
					label={averageLabel}
					value={DECIMAL_FORMATTER.format(averageEvents)}
				/>
				<TrendMetric
					label={latestLabel}
					value={COUNT_FORMATTER.format(latestReport.total_events)}
				/>
				<TrendMetric
					label={peakLabel}
					value={`${formatTrendDayLabel(peakReport.date)} · ${COUNT_FORMATTER.format(peakReport.total_events)}`}
				/>
			</div>

			<div className="min-w-0">
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
					<Suspense fallback={null}>
						<RechartsTrendPlot series={series} trendData={trendData} />
					</Suspense>
				</div>
			</div>
		</div>
	);
}
