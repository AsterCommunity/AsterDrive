import { useTranslation } from "react-i18next";
import { Progress } from "@/components/ui/progress";
import type { TaskInfo } from "@/types/api";
import {
	buildTaskTimeline,
	currentTaskStep,
	formatProgressCounts,
	formatTaskStepStatus,
	formatTaskStepTitle,
	parseStoragePolicyMigrationResult,
	parseTaskResult,
	stepCircleClass,
	stepCircleLabel,
	stepConnectorClass,
	stepProgressPercent,
	stepStatusTextClass,
} from "./taskPresentation";

export function TaskStepsPreview({ task }: { task: TaskInfo }) {
	const { t } = useTranslation(["core", "tasks"]);
	const activeStep = currentTaskStep(task);

	if (task.steps.length === 0) {
		return null;
	}

	return (
		<div className="space-y-2.5 rounded-lg border bg-muted/15 p-3">
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
										<div className="relative flex size-10 items-center justify-center md:h-11 md:w-11">
											{step.status === "active" ? (
												<span className="absolute inset-0 animate-spin rounded-full border-2 border-primary/20 border-t-primary" />
											) : null}
											<span
												className={`relative flex size-8 items-center justify-center rounded-full border text-xs font-semibold transition-colors md:h-9 md:w-9 md:text-sm ${stepCircleClass(step.status)}`}
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
									<div className="flex size-10 shrink-0 items-center px-1 md:h-11 md:w-12 md:px-1.5">
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

export function TaskDetailsContent({ task }: { task: TaskInfo }) {
	const { t } = useTranslation(["core", "tasks"]);
	const parsedResult = parseTaskResult(task);
	const parsedStorageMigrationResult = parseStoragePolicyMigrationResult(task);
	const taskTimeline = buildTaskTimeline(t, task);
	const progressValue =
		task.status === "succeeded"
			? 100
			: Math.max(0, Math.min(100, task.progress_percent));
	const hasProgressCounts = task.progress_total > 0;

	return (
		<div className="space-y-2.5">
			<div className="rounded-lg border bg-background/70 p-3">
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
				<div className="rounded-lg border bg-muted/20 p-3 text-sm">
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
			{task.status === "succeeded" && parsedStorageMigrationResult ? (
				<div className="rounded-lg border bg-muted/20 p-3 text-sm">
					<div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
						{[
							{
								label: t("tasks:storage_migration_migrated_blobs"),
								value: parsedStorageMigrationResult.migrated_blobs ?? 0,
							},
							{
								label: t("tasks:storage_migration_renamed_opaque_blobs"),
								value: parsedStorageMigrationResult.renamed_opaque_blobs ?? 0,
							},
							{
								label: t("tasks:storage_migration_skipped_blobs"),
								value: parsedStorageMigrationResult.skipped_blobs ?? 0,
							},
							{
								label: t("tasks:storage_migration_failed_blobs"),
								value: parsedStorageMigrationResult.failed_blobs ?? 0,
							},
							{
								label: t("tasks:storage_migration_migrated_bytes"),
								value: parsedStorageMigrationResult.migrated_bytes ?? 0,
							},
						].map((item) => (
							<div key={item.label} className="min-w-0">
								<div className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">
									{item.label}
								</div>
								<div className="mt-1 text-lg font-semibold tabular-nums text-foreground">
									{item.value}
								</div>
							</div>
						))}
					</div>
				</div>
			) : null}
		</div>
	);
}
