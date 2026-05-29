import { Badge } from "@/components/ui/badge";
import { Icon } from "@/components/ui/icon";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { StoragePolicyCapacityInfo } from "@/types/api";
import type {
	StoragePolicyDriverOption,
	Translate,
} from "./StoragePolicyFieldTypes";

export function StorageDriverVisual({
	className,
	option,
}: {
	className?: string;
	option: StoragePolicyDriverOption;
}) {
	return option.iconSrc ? (
		<img
			src={option.iconSrc}
			alt=""
			className={cn(
				"w-auto object-contain",
				option.type === "local" ? "max-h-7" : "max-h-9",
				className,
			)}
		/>
	) : (
		<Icon
			name={option.iconName ?? "Globe"}
			className={cn("size-8 text-amber-600 dark:text-amber-300", className)}
		/>
	);
}

export function PolicySectionIntro({
	description,
	title,
}: {
	description: string;
	title: string;
}) {
	return (
		<div className="mb-5">
			<h3 className="text-base font-semibold text-foreground">{title}</h3>
			<p className="mt-1 text-sm text-muted-foreground">{description}</p>
		</div>
	);
}

export function PolicySummaryCard({
	currentStorageOption,
	description,
	formName,
	items,
	t,
}: {
	currentStorageOption: StoragePolicyDriverOption;
	description: string;
	formName: string;
	items: Array<{ label: string; value: string }>;
	t: Translate;
}) {
	return (
		<div
			data-testid="policy-summary-card"
			className="rounded-3xl border border-border/70 bg-muted/20 p-5"
		>
			<div className="flex items-center gap-3">
				<div className="flex size-14 items-center justify-center rounded-2xl bg-white shadow-sm ring-1 ring-black/5">
					<StorageDriverVisual option={currentStorageOption} />
				</div>
				<div>
					<p className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">
						{t("policy_wizard_summary_title")}
					</p>
					<h3 className="mt-1 text-base font-semibold">
						{formName || t("new_policy")}
					</h3>
				</div>
			</div>
			<p className="mt-4 text-sm leading-6 text-muted-foreground">
				{description}
			</p>
			<div className="mt-4 overflow-hidden rounded-2xl border border-border/70 bg-background/85">
				<dl className="divide-y divide-border/70">
					{items.map((item) => (
						<div
							key={item.label}
							className="grid grid-cols-[96px_minmax(0,1fr)] items-start gap-3 px-4 py-3"
						>
							<dt className="pt-0.5 text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
								{item.label}
							</dt>
							<dd className="min-w-0 break-all text-sm font-medium leading-5 text-foreground">
								{item.value}
							</dd>
						</div>
					))}
				</dl>
			</div>
		</div>
	);
}

function capacityStatusTone(
	status: StoragePolicyCapacityInfo["capacity"]["status"],
) {
	if (status === "supported") {
		return "border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900 dark:bg-emerald-950/40 dark:text-emerald-300";
	}
	if (status === "unsupported") {
		return "border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-300";
	}
	return "border-border bg-muted/40 text-muted-foreground";
}

export function PolicyCapacityCard({
	capacity,
	loading,
	t,
}: {
	capacity: StoragePolicyCapacityInfo | null;
	loading: boolean;
	t: Translate;
}) {
	const info = capacity?.capacity;
	const available = info?.available_bytes;
	const total = info?.total_bytes;
	const used = info?.used_bytes;
	const blobTotalBytes = capacity?.blob_total_bytes;
	const blobCount = capacity?.blob_count;
	const capacitySegments =
		typeof blobTotalBytes === "number" &&
		typeof used === "number" &&
		typeof available === "number" &&
		typeof total === "number" &&
		total > 0
			? (() => {
					const blobClamped = Math.min(Math.max(blobTotalBytes, 0), used);
					return {
						blob: (blobClamped / total) * 100,
						other:
							(Math.min(Math.max(used - blobClamped, 0), total) / total) * 100,
						available: (Math.min(Math.max(available, 0), total) / total) * 100,
					};
				})()
			: null;

	return (
		<div className="rounded-3xl border border-border/70 bg-background/80 p-4">
			<div className="flex items-start justify-between gap-3">
				<div>
					<p className="text-xs font-medium uppercase tracking-[0.18em] text-muted-foreground">
						{t("policy_capacity_title")}
					</p>
				</div>
				<span
					className={cn(
						"rounded-full border px-2 py-0.5 text-[11px] font-medium",
						info
							? capacityStatusTone(info.status)
							: capacityStatusTone("unavailable"),
					)}
				>
					{loading
						? t("policy_capacity_checking")
						: info
							? t(`policy_capacity_status_${info.status}`)
							: t("policy_capacity_status_unavailable")}
				</span>
			</div>

			{loading || typeof blobTotalBytes !== "number" ? (
				<p className="mt-4 text-sm leading-6 text-muted-foreground">
					{loading
						? t("policy_capacity_loading")
						: info?.status === "unsupported"
							? t("policy_capacity_unsupported_desc")
							: t("policy_capacity_unavailable_desc")}
				</p>
			) : (
				<div className="mt-4 space-y-3">
					<div>
						<p className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
							{t("policy_capacity_blob_usage")}
						</p>
						<div className="mt-1 flex items-baseline justify-between gap-3">
							<p className="text-lg font-semibold tabular-nums text-foreground">
								{formatBytes(blobTotalBytes)}
							</p>
							{typeof blobCount === "number" ? (
								<p className="text-xs text-muted-foreground">
									{t("policy_capacity_blob_count", { count: blobCount })}
								</p>
							) : null}
						</div>
					</div>

					{typeof available === "number" && typeof total === "number" ? (
						<div className="rounded-2xl border border-border/70 bg-muted/20 p-3">
							<div className="grid grid-cols-2 gap-3">
								<div>
									<p className="text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
										{t("policy_capacity_system_used")}
									</p>
									<p className="mt-1 text-sm font-medium tabular-nums text-foreground">
										{typeof used === "number" ? formatBytes(used) : "—"}
									</p>
								</div>
								<div>
									<p className="text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
										{t("policy_capacity_available")}
									</p>
									<p className="mt-1 text-sm font-medium tabular-nums text-foreground">
										{formatBytes(available)}
									</p>
								</div>
							</div>
							<p className="mt-2 text-xs text-muted-foreground">
								{t("policy_capacity_total", { total: formatBytes(total) })}
							</p>
							{capacitySegments != null ? (
								<>
									<div className="mt-3 flex h-2 w-full overflow-hidden rounded-full bg-muted">
										<div
											className="h-full min-w-px bg-emerald-500"
											style={{ width: `${capacitySegments.other}%` }}
										/>
										<div
											className="h-full min-w-px bg-blue-500"
											style={{ width: `${capacitySegments.blob}%` }}
										/>
										<div
											className="h-full bg-muted"
											style={{ width: `${capacitySegments.available}%` }}
										/>
									</div>
									<div className="mt-2 grid gap-1 text-xs text-muted-foreground">
										<div className="flex items-center gap-2">
											<span className="size-2 rounded-full bg-emerald-500" />
											<span>{t("policy_capacity_other_system_used")}</span>
										</div>
										<div className="flex items-center gap-2">
											<span className="size-2 rounded-full bg-blue-500" />
											<span>{t("policy_capacity_blob_usage")}</span>
										</div>
										<div className="flex items-center gap-2">
											<span className="size-2 rounded-full bg-muted ring-1 ring-border" />
											<span>{t("policy_capacity_available")}</span>
										</div>
									</div>
								</>
							) : null}
						</div>
					) : (
						<p className="text-sm leading-6 text-muted-foreground">
							{info?.status === "unsupported"
								? t("policy_capacity_unsupported_desc")
								: t("policy_capacity_unavailable_desc")}
						</p>
					)}
				</div>
			)}
		</div>
	);
}

export function DriverTypeBadge({
	className,
	title,
}: {
	className: string;
	title: string;
}) {
	return (
		<Badge
			variant="outline"
			data-testid="policy-driver-badge"
			className={className}
		>
			{title}
		</Badge>
	);
}
