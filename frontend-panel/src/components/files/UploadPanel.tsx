import { useVirtualizer } from "@tanstack/react-virtual";
import { useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { TransferActivitySection } from "@/components/files/TransferActivitySection";
import { UploadTaskItem } from "@/components/files/UploadTaskItem";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Switch } from "@/components/ui/switch";
import {
	DEFAULT_UPLOAD_CONCURRENCY,
	MAX_UPLOAD_CONCURRENCY,
	MIN_UPLOAD_CONCURRENCY,
} from "@/lib/uploadSettings";
import { cn } from "@/lib/utils";

export interface UploadTaskView {
	id: string;
	title: string;
	status: string;
	mode: string;
	progress: number;
	group?: string;
	targetLabel?: string;
	batchStatus?: "active" | "done" | "partial_failed";
	detail?: string;
	speed?: string;
	completed?: boolean;
	cancelled?: boolean;
	actions?: {
		label: string;
		icon: "X" | "ArrowsClockwise" | "Upload";
		onClick: () => void;
		variant?: "outline" | "ghost";
	}[];
}

interface UploadPanelProps {
	embedded?: boolean;
	open: boolean;
	onToggle: () => void;
	title: string;
	summary: string;
	tasks: UploadTaskView[];
	emptyText: string;
	totalCount?: number;
	successCount?: number;
	failedCount?: number;
	activeCount?: number;
	overallProgress?: number;
	concurrency?: number;
	autoClearCompleted?: boolean;
	onConcurrencyChange?: (value: number) => void;
	onAutoClearCompletedChange?: (value: boolean) => void;
	onRetryFailed?: () => void;
	retryFailedLabel?: string;
	onClearCompleted?: () => void;
	clearCompletedLabel?: string;
}

type FlatRow =
	| {
			type: "group-header";
			key: string;
			group: string;
			batchStatus: string;
			total: number;
			success: number;
			failed: number;
			active: number;
	  }
	| { type: "task"; key: string; task: UploadTaskView };

const ROW_HEIGHT_TASK_COMPACT = 56;
const ROW_HEIGHT_TASK_PROGRESS = 74;
const ROW_HEIGHT_GROUP = 38;
const PANEL_EXPANDED_BODY_CLASS = "h-[min(26rem,calc(100dvh-15rem))]";

function taskShowsProgress(task: UploadTaskView) {
	const failed = task.actions?.some(
		(action) => action.icon === "ArrowsClockwise",
	);
	const waitingForFile = task.actions?.some(
		(action) => action.icon === "Upload",
	);
	return (
		!task.completed &&
		!task.cancelled &&
		!failed &&
		!waitingForFile &&
		task.progress < 100
	);
}

function taskRowKey(task: UploadTaskView) {
	return `${task.id}-${taskShowsProgress(task) ? "progress" : "compact"}`;
}

export function UploadPanel({
	embedded = false,
	open,
	onToggle,
	title,
	summary,
	tasks,
	emptyText,
	successCount = 0,
	failedCount = 0,
	activeCount = 0,
	overallProgress = 0,
	concurrency = DEFAULT_UPLOAD_CONCURRENCY,
	autoClearCompleted = false,
	onConcurrencyChange,
	onAutoClearCompletedChange,
	onRetryFailed,
	retryFailedLabel,
	onClearCompleted,
	clearCompletedLabel,
}: UploadPanelProps) {
	const { t } = useTranslation("files");
	const scrollRef = useRef<HTMLDivElement>(null);

	// ── 展平分组为虚拟列表行 ──
	const flatRows = useMemo<FlatRow[]>(() => {
		const grouped: Record<string, UploadTaskView[]> = {};
		for (const task of tasks) {
			const key = task.group ?? "";
			if (!grouped[key]) grouped[key] = [];
			grouped[key].push(task);
		}

		const rows: FlatRow[] = [];
		for (const [group, groupTasks] of Object.entries(grouped)) {
			let success = 0;
			let failed = 0;
			let active = 0;
			for (const task of groupTasks) {
				if (task.completed) success++;
				else if (
					task.cancelled ||
					task.actions?.some((a) => a.icon === "ArrowsClockwise")
				)
					failed++;
				else active++;
			}
			const batchStatus =
				active > 0 ? "active" : failed > 0 ? "partial_failed" : "done";

			rows.push({
				type: "group-header",
				key: `gh-${group || "root"}`,
				group,
				batchStatus,
				total: groupTasks.length,
				success,
				failed,
				active,
			});
			for (const task of groupTasks) {
				rows.push({ type: "task", key: taskRowKey(task), task });
			}
		}
		return rows;
	}, [tasks]);

	const virtualizer = useVirtualizer({
		count: flatRows.length,
		getItemKey: (index) => flatRows[index]?.key ?? index,
		getScrollElement: () => scrollRef.current,
		estimateSize: (index) => {
			const row = flatRows[index];
			if (row.type === "group-header") return ROW_HEIGHT_GROUP;
			return taskShowsProgress(row.task)
				? ROW_HEIGHT_TASK_PROGRESS
				: ROW_HEIGHT_TASK_COMPACT;
		},
		overscan: 5,
	});
	const canRetryFailed = Boolean(
		onRetryFailed && retryFailedLabel && failedCount > 0,
	);
	const canClearCompleted = Boolean(
		onClearCompleted && clearCompletedLabel && successCount > 0,
	);
	const showOverallProgress = activeCount > 0;
	const generatedSettingsPanelId = useId();
	const settingsPanelId = `upload-panel-settings-${generatedSettingsPanelId}`;
	const [settingsOpen, setSettingsOpen] = useState(false);
	const canDecreaseConcurrency =
		Boolean(onConcurrencyChange) && concurrency > MIN_UPLOAD_CONCURRENCY;
	const canIncreaseConcurrency =
		Boolean(onConcurrencyChange) && concurrency < MAX_UPLOAD_CONCURRENCY;
	const handleSettingsToggle = () => {
		if (!open) {
			setSettingsOpen(true);
			onToggle();
			return;
		}
		setSettingsOpen((prev) => !prev);
	};
	const tone =
		activeCount > 0
			? ("active" as const)
			: failedCount > 0
				? ("error" as const)
				: successCount > 0
					? ("success" as const)
					: ("default" as const);

	return (
		<div
			className={cn(
				"w-full max-w-full",
				embedded
					? "pointer-events-auto"
					: "fixed right-4 bottom-4 z-(--z-fixed) max-w-[calc(100vw-2rem)]",
			)}
		>
			<TransferActivitySection
				open={open}
				onToggle={onToggle}
				title={title}
				summary={summary}
				icon={
					successCount > 0 && activeCount === 0 && failedCount === 0
						? "Check"
						: "Upload"
				}
				tone={tone}
				progress={showOverallProgress ? overallProgress : null}
				toggleLabel={
					open ? t("upload_panel_collapse") : t("upload_panel_expand")
				}
				expandedBodyClassName={PANEL_EXPANDED_BODY_CLASS}
				actions={
					<Button
						variant="ghost"
						size="icon-xs"
						onClick={handleSettingsToggle}
						aria-expanded={settingsOpen}
						aria-controls={settingsPanelId}
						aria-label={t("upload_settings")}
						title={t("upload_settings")}
					>
						<Icon name="Gear" className="size-3" />
					</Button>
				}
			>
				<div className="flex h-full min-h-0 flex-col">
					<UploadSettingsPanel
						autoClearCompleted={autoClearCompleted}
						concurrency={concurrency}
						canDecreaseConcurrency={canDecreaseConcurrency}
						canIncreaseConcurrency={canIncreaseConcurrency}
						open={settingsOpen}
						panelId={settingsPanelId}
						onAutoClearCompletedChange={onAutoClearCompletedChange}
						onConcurrencyChange={onConcurrencyChange}
						t={t}
					/>
					<div className="min-h-0 flex-1 overflow-hidden bg-background/70 dark:bg-background/20">
						{tasks.length === 0 ? (
							<div className="flex h-full min-h-[10rem] items-center justify-center px-6 py-8 text-center text-sm text-muted-foreground">
								{emptyText}
							</div>
						) : (
							<ScrollArea ref={scrollRef} className="h-full w-full">
								<div
									className="relative w-full"
									style={{
										height: virtualizer.getTotalSize(),
									}}
								>
									{virtualizer.getVirtualItems().map((virtualRow) => {
										const row = flatRows[virtualRow.index];
										return (
											<div
												key={row.key}
												className="absolute inset-x-0 w-full overflow-hidden"
												style={{
													height: virtualRow.size,
													transform: `translateY(${virtualRow.start}px)`,
												}}
											>
												{row.type === "group-header" ? (
													<GroupHeader row={row} t={t} />
												) : (
													<UploadTaskItem {...row.task} />
												)}
											</div>
										);
									})}
								</div>
							</ScrollArea>
						)}
					</div>
					{canRetryFailed || canClearCompleted ? (
						<div className="flex shrink-0 justify-end gap-2 border-t border-border/60 bg-card/80 px-4 py-3 dark:bg-card/65">
							{canRetryFailed ? (
								<Button variant="outline" size="sm" onClick={onRetryFailed}>
									<Icon name="ArrowsClockwise" className="size-3.5" />
									{retryFailedLabel}
								</Button>
							) : null}
							{canClearCompleted ? (
								<Button variant="outline" size="sm" onClick={onClearCompleted}>
									<Icon name="X" className="size-3.5" />
									{clearCompletedLabel}
								</Button>
							) : null}
						</div>
					) : null}
				</div>
			</TransferActivitySection>
		</div>
	);
}

function GroupHeader({
	row,
	t,
}: {
	row: Extract<FlatRow, { type: "group-header" }>;
	t: (key: string, opts?: Record<string, unknown>) => string;
}) {
	return (
		<div className="flex h-full w-full items-center gap-2 border-b border-border/70 bg-muted/35 px-4 py-2 text-[11px] text-muted-foreground dark:border-border/55 dark:bg-muted/20">
			<div className="min-w-0 flex-1 truncate font-medium text-foreground/75">
				{row.group || t("root")}
			</div>
			<span className="shrink-0 tabular-nums">
				{t("upload_group_item_count", { count: row.total })}
			</span>
			<span
				className={cn(
					"shrink-0 rounded-full px-1.5 py-0.5 font-medium",
					row.batchStatus === "active" &&
						"bg-primary/10 text-primary dark:bg-primary/15",
					row.batchStatus === "partial_failed" &&
						"bg-destructive/10 text-destructive dark:bg-destructive/15",
					row.batchStatus === "done" &&
						"bg-emerald-500/10 text-emerald-600 dark:text-emerald-400",
				)}
			>
				{row.batchStatus === "active"
					? t("upload_batch_active")
					: row.batchStatus === "partial_failed"
						? t("upload_batch_partial_failed")
						: t("upload_batch_done")}
			</span>
		</div>
	);
}

function UploadSettingsPanel({
	autoClearCompleted,
	canDecreaseConcurrency,
	canIncreaseConcurrency,
	concurrency,
	open,
	panelId,
	onAutoClearCompletedChange,
	onConcurrencyChange,
	t,
}: {
	autoClearCompleted: boolean;
	canDecreaseConcurrency: boolean;
	canIncreaseConcurrency: boolean;
	concurrency: number;
	open: boolean;
	panelId: string;
	onAutoClearCompletedChange?: (value: boolean) => void;
	onConcurrencyChange?: (value: number) => void;
	t: (key: string, opts?: Record<string, unknown>) => string;
}) {
	return (
		<div
			id={panelId}
			aria-hidden={!open}
			inert={open ? undefined : true}
			className={cn(
				"overflow-hidden border-b border-border/60 bg-card/70 transition-[max-height,opacity] duration-150 ease-out dark:bg-card/50",
				open ? "max-h-40 opacity-100" : "max-h-0 opacity-0",
			)}
		>
			<div className="grid gap-3 px-4 py-3">
				<div className="flex items-center justify-between gap-3">
					<div className="min-w-0">
						<div className="truncate text-xs font-medium text-foreground">
							{t("upload_concurrency")}
						</div>
						<div className="truncate text-[11px] text-muted-foreground">
							{t("upload_concurrency_desc")}
						</div>
					</div>
					<div className="flex shrink-0 items-center rounded-lg border border-border/70 bg-background/70 p-0.5 dark:bg-background/25">
						<Button
							variant="ghost"
							size="icon-xs"
							onClick={() => onConcurrencyChange?.(concurrency - 1)}
							disabled={!canDecreaseConcurrency}
							aria-label={t("upload_concurrency_decrease")}
							title={t("upload_concurrency_decrease")}
						>
							<Icon name="Minus" className="size-3" />
						</Button>
						<span className="w-8 text-center text-xs font-medium tabular-nums">
							{concurrency}
						</span>
						<Button
							variant="ghost"
							size="icon-xs"
							onClick={() => onConcurrencyChange?.(concurrency + 1)}
							disabled={!canIncreaseConcurrency}
							aria-label={t("upload_concurrency_increase")}
							title={t("upload_concurrency_increase")}
						>
							<Icon name="Plus" className="size-3" />
						</Button>
					</div>
				</div>
				<div className="flex items-center justify-between gap-3">
					<div className="min-w-0">
						<div className="truncate text-xs font-medium text-foreground">
							{t("upload_auto_clear_completed")}
						</div>
						<div className="truncate text-[11px] text-muted-foreground">
							{t("upload_auto_clear_completed_desc")}
						</div>
					</div>
					<Switch
						size="sm"
						checked={autoClearCompleted}
						onCheckedChange={(value) => onAutoClearCompletedChange?.(value)}
						aria-label={t("upload_auto_clear_completed")}
					/>
				</div>
			</div>
		</div>
	);
}
