import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Icon } from "@/components/ui/icon";
import { Progress } from "@/components/ui/progress";
import type { TaskInfo } from "@/types/api";
import { AnimatedTaskDetails } from "./AnimatedTaskDetails";
import {
	buildTaskTimeline,
	currentTaskStep,
	formatProgressCounts,
	formatTaskKind,
	formatTaskStatus,
	formatTaskStepStatus,
	formatTaskStepTitle,
	parseTaskResult,
	statusBadgeVariant,
	stepCircleClass,
	stepCircleLabel,
	stepConnectorClass,
	stepProgressPercent,
	stepStatusTextClass,
	taskMetaTextClass,
	taskSummaryTimestamp,
} from "./taskPresentation";

interface TaskCardProps {
	detailsExpanded: boolean;
	onOpenTargetFolder: (targetFolderId: number | null) => void;
	onRetry: (taskId: number) => void;
	onToggleDetails: (taskId: number) => void;
	retrying: boolean;
	task: TaskInfo;
}

function TaskStepsPreview({
	activeStep,
	task,
}: {
	activeStep: ReturnType<typeof currentTaskStep>;
	task: TaskInfo;
}) {
	const { t } = useTranslation(["core", "tasks"]);

	if (task.steps.length === 0) {
		return null;
	}

	return (
		<div className="space-y-2.5 rounded-lg border bg-muted/15 px-3 py-3">
			<div className="flex flex-wrap items-start justify-between gap-2">
				<div className="text-sm font-medium">{t("tasks:steps_label")}</div>
				{activeStep && activeStep.progress_total > 0 ? (
					<div className="text-right text-xs text-muted-foreground">
						<div>{t("tasks:step_progress_label")}</div>
						<div className="font-medium tabular-nums text-foreground">
							{stepProgressPercent(activeStep)}% ·{" "}
							{formatProgressCounts(
								activeStep.progress_current,
								activeStep.progress_total,
							)}
						</div>
					</div>
				) : null}
			</div>
			<div className="overflow-x-auto pb-0.5">
				<div className="w-full">
					<div className="mx-auto flex w-fit min-w-max items-start px-0.5 py-1.5">
						{task.steps.map((step, index) => (
							<div key={`${task.id}-${step.key}`} className="contents">
								<div className="w-32 shrink-0 md:w-36 lg:w-40">
									<div className="flex flex-col items-center text-center">
										<div className="relative flex h-10 w-10 items-center justify-center md:h-11 md:w-11">
											{step.status === "active" ? (
												<span className="absolute inset-0 animate-spin rounded-full border-2 border-primary/20 border-t-primary" />
											) : null}
											<span
												className={`relative flex h-8 w-8 items-center justify-center rounded-full border text-xs font-semibold transition-colors md:h-9 md:w-9 md:text-sm ${stepCircleClass(step.status)}`}
											>
												{stepCircleLabel(index, step.status)}
											</span>
										</div>
										<div className="mt-2 space-y-0.5 md:mt-2.5">
											<p className="text-xs font-semibold leading-snug md:text-sm">
												{index + 1}. {formatTaskStepTitle(t, task.kind, step)}
											</p>
											<p
												className={`text-[11px] font-medium ${stepStatusTextClass(step.status)}`}
											>
												{formatTaskStepStatus(t, step.status)}
											</p>
										</div>
									</div>
								</div>
								{index < task.steps.length - 1 ? (
									<div className="flex h-10 w-10 shrink-0 items-center px-1 md:h-11 md:w-12 md:px-1.5">
										<div
											className={`h-1 w-full rounded-full ${stepConnectorClass(step.status)}`}
										/>
									</div>
								) : null}
							</div>
						))}
					</div>
				</div>
			</div>
		</div>
	);
}

export function TaskCard({
	detailsExpanded,
	onOpenTargetFolder,
	onRetry,
	onToggleDetails,
	retrying,
	task,
}: TaskCardProps) {
	const { t } = useTranslation(["core", "tasks"]);
	const parsedResult = parseTaskResult(task);
	const activeStep = currentTaskStep(task);
	const taskTimeline = buildTaskTimeline(t, task);
	const activeStepDetail = activeStep?.detail?.trim() ?? null;
	const statusText = task.status_text?.trim() ?? null;
	const summaryTimestamp = taskSummaryTimestamp(t, task);
	const detailsSectionId = `task-details-${task.id}`;
	const hasExpandableDetails =
		task.steps.length > 0 ||
		task.last_error !== null ||
		(task.status === "succeeded" && parsedResult !== null);
	const taskSummaryText =
		statusText && activeStepDetail
			? statusText.toLocaleLowerCase() === activeStepDetail.toLocaleLowerCase()
				? activeStepDetail
				: statusText
			: statusText || activeStepDetail;
	const progressValue =
		task.status === "succeeded"
			? 100
			: Math.max(0, Math.min(100, task.progress_percent));
	const hasProgressCounts = task.progress_total > 0;

	return (
		<Card className="p-4 md:p-5">
			<div className="flex flex-col gap-4">
				<div className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
					<div className="min-w-0 space-y-2">
						<div className="flex flex-wrap items-center gap-2">
							<h2 className="truncate text-lg font-semibold">
								{task.display_name}
							</h2>
							<Badge variant={statusBadgeVariant(task.status)}>
								{formatTaskStatus(t, task.status)}
							</Badge>
							<Badge variant="outline">{formatTaskKind(t, task.kind)}</Badge>
						</div>
						<div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm">
							<span className="text-muted-foreground">
								{t("tasks:task_id_label", { id: task.id })}
							</span>
							{summaryTimestamp ? (
								<>
									<span className="text-border">·</span>
									<span
										className={`font-medium ${taskMetaTextClass(task.status)}`}
									>
										{summaryTimestamp}
									</span>
								</>
							) : null}
						</div>
					</div>
					<div className="flex shrink-0 items-center gap-2">
						{task.status === "succeeded" && parsedResult ? (
							<Button
								variant="outline"
								size="sm"
								onClick={() =>
									onOpenTargetFolder(parsedResult.target_folder_id ?? null)
								}
							>
								<Icon name="FolderOpen" className="mr-1 h-4 w-4" />
								{t("tasks:open_target_folder")}
							</Button>
						) : null}
						{hasExpandableDetails ? (
							<Button
								variant="outline"
								size="sm"
								aria-controls={detailsSectionId}
								aria-expanded={detailsExpanded}
								onClick={() => onToggleDetails(task.id)}
							>
								<Icon
									name={detailsExpanded ? "CaretUp" : "CaretDown"}
									className="mr-1 h-4 w-4"
								/>
								{detailsExpanded
									? t("tasks:hide_details")
									: t("tasks:show_details")}
							</Button>
						) : null}
						{task.can_retry ? (
							<Button
								variant="outline"
								size="sm"
								onClick={() => onRetry(task.id)}
								disabled={retrying}
							>
								<Icon
									name={retrying ? "Spinner" : "ArrowCounterClockwise"}
									className={`mr-1 h-4 w-4 ${retrying ? "animate-spin" : ""}`}
								/>
								{t("tasks:retry_task")}
							</Button>
						) : null}
					</div>
				</div>

				{task.steps.length > 0 ? (
					<TaskStepsPreview activeStep={activeStep} task={task} />
				) : null}

				{taskSummaryText ? (
					<div className="space-y-2">
						<p className="text-sm text-muted-foreground">
							{t("tasks:status_text_label")}: {taskSummaryText}
						</p>
					</div>
				) : null}

				<AnimatedTaskDetails open={detailsExpanded} className="space-y-2.5">
					<div id={detailsSectionId} className="space-y-2.5 pt-0.5">
						<div className="rounded-lg border bg-background/70 px-3 py-3">
							<div className="grid gap-3 lg:grid-cols-[minmax(0,1.35fr)_minmax(16rem,0.65fr)] lg:items-start">
								<div className="space-y-2">
									<div className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">
										{t("tasks:timeline_label")}
									</div>
									<div className="flex flex-wrap gap-2">
										{taskTimeline.map((entry) => (
											<div
												key={`${task.id}-${entry.label}`}
												className="min-w-[11rem] flex-1 rounded-md bg-muted/25 px-2.5 py-2"
											>
												<div className="text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
													{entry.label}
												</div>
												<div className="mt-1 text-sm font-medium tabular-nums text-foreground">
													{entry.value}
												</div>
											</div>
										))}
									</div>
								</div>
								<div className="space-y-2 rounded-lg bg-muted/20 px-3 py-2.5">
									<div className="flex items-end justify-between gap-3">
										<div className="space-y-1">
											<div className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">
												{t("tasks:progress_label")}
											</div>
											<div className="text-3xl font-semibold tracking-tight tabular-nums">
												{progressValue}%
											</div>
										</div>
										{hasProgressCounts ? (
											<div className="text-right text-xs text-muted-foreground">
												<div>{t("tasks:progress_ratio_label")}</div>
												<div className="font-medium text-foreground tabular-nums">
													{formatProgressCounts(
														task.progress_current,
														task.progress_total,
													)}
												</div>
											</div>
										) : null}
									</div>
									<Progress value={progressValue} className="h-2" />
								</div>
							</div>
						</div>

						{task.last_error ? (
							<div className="rounded-lg border border-destructive/20 bg-destructive/5 px-3 py-2 text-sm text-destructive">
								<span className="font-medium">{t("tasks:error_label")}:</span>{" "}
								{task.last_error}
							</div>
						) : null}

						{task.status === "succeeded" && parsedResult ? (
							<div className="rounded-lg border bg-muted/20 px-3 py-3 text-sm">
								<div className="min-w-0">
									<div className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">
										{t("tasks:result_path_label")}
									</div>
									<div className="mt-1 truncate text-foreground">
										{parsedResult.target_path}
									</div>
								</div>
							</div>
						) : null}
					</div>
				</AnimatedTaskDetails>
			</div>
		</Card>
	);
}
