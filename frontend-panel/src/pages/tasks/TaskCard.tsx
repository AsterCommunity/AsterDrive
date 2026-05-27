import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Icon, type IconName } from "@/components/ui/icon";
import type { TaskInfo } from "@/types/api";
import { AnimatedTaskDetails } from "./AnimatedTaskDetails";
import {
	TaskDetailsContent,
	TaskStepsPreview,
	taskHasExpandableDetails,
} from "./TaskDetailsPanel";
import {
	currentTaskStep,
	formatTaskKind,
	formatTaskStatus,
	parseTaskResult,
	statusBadgeVariant,
	taskMetaTextClass,
	taskSummaryTimestamp,
} from "./taskPresentation";

type SummaryPart =
	| { kind: "text"; value: string }
	| { icon: IconName; kind: "chip"; value: string };

interface TaskCardProps {
	detailsExpanded: boolean;
	onOpenTargetFolder: (targetFolderId: number | null) => void;
	onRetry: (taskId: number) => void;
	onToggleDetails: (taskId: number) => void;
	retrying: boolean;
	task: TaskInfo;
}

function taskIcon(task: TaskInfo): IconName {
	switch (task.kind) {
		case "archive_extract":
			return "FileZip";
		case "archive_compress":
			return "FileZip";
		case "archive_preview_generate":
			return "Eye";
		case "thumbnail_generate":
			return "FileImage";
		case "media_metadata_extract":
			return "Info";
		case "trash_purge_all":
			return "Trash";
		case "storage_policy_migration":
			return "HardDrive";
		case "storage_policy_temp_cleanup":
			return "Clock";
		case "system_runtime":
			return "Gear";
		default:
			return "Queue";
	}
}

function summaryParts(
	t: (key: string, options?: Record<string, unknown>) => string,
	task: TaskInfo,
): SummaryPart[] {
	switch (task.payload.kind) {
		case "archive_extract":
			return [
				{ kind: "text", value: t("tasks:summary_action_prefix") },
				{
					icon: "FileZip",
					kind: "chip",
					value: task.payload.source_file_name,
				},
				{ kind: "text", value: t("tasks:summary_archive_extract_to") },
				{
					icon: "FolderOpen",
					kind: "chip",
					value:
						task.payload.output_folder_name || t("tasks:summary_root_folder"),
				},
			];
		case "archive_compress": {
			const selectedCount =
				task.payload.file_ids.length + task.payload.folder_ids.length;
			return [
				{ kind: "text", value: t("tasks:summary_action_prefix") },
				{
					icon: "Folder",
					kind: "chip",
					value: t("tasks:summary_selected_items", { count: selectedCount }),
				},
				{ kind: "text", value: t("tasks:summary_archive_compress_to") },
				{
					icon: "FileZip",
					kind: "chip",
					value: task.payload.archive_name,
				},
			];
		}
		case "archive_preview_generate":
			return [
				{ kind: "text", value: t("tasks:summary_generate_preview_for") },
				{
					icon: "FileZip",
					kind: "chip",
					value: task.payload.source_file_name,
				},
			];
		case "thumbnail_generate":
			return [
				{ kind: "text", value: t("tasks:summary_generate_thumbnail_for") },
				{
					icon: "FileImage",
					kind: "chip",
					value: task.payload.source_file_name || task.display_name,
				},
			];
		case "trash_purge_all":
			return [{ kind: "text", value: t("tasks:summary_purge_trash") }];
		case "storage_policy_migration":
			return [
				{ kind: "text", value: t("tasks:summary_migrate_storage_policy") },
				{
					icon: "HardDrive",
					kind: "chip",
					value: t("tasks:summary_policy_id", {
						id: task.payload.source_policy_id,
					}),
				},
				{ kind: "text", value: t("tasks:summary_archive_extract_to") },
				{
					icon: "HardDrive",
					kind: "chip",
					value: t("tasks:summary_policy_id", {
						id: task.payload.target_policy_id,
					}),
				},
			];
		case "storage_policy_temp_cleanup":
			return [{ kind: "text", value: t("tasks:summary_cleanup_temp_files") }];
		case "system_runtime":
			return [
				{
					kind: "text",
					value: t("tasks:summary_system_runtime", {
						name: task.payload.task_name,
					}),
				},
			];
		default:
			return [{ kind: "text", value: task.display_name }];
	}
}

function TaskSummaryChip({ icon, value }: { icon: IconName; value: string }) {
	return (
		<span className="inline-flex max-w-full items-center gap-1.5 rounded-lg border border-border/70 bg-background/55 px-2.5 py-1 font-medium text-foreground">
			<Icon name={icon} className="size-4 shrink-0 text-muted-foreground" />
			<span className="truncate">{value}</span>
		</span>
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
	const activeStepDetail = activeStep?.detail?.trim() ?? null;
	const statusText = task.status_text?.trim() ?? null;
	const summaryTimestamp = taskSummaryTimestamp(t, task);
	const detailsSectionId = `task-details-${task.id}`;
	const hasExpandableDetails = taskHasExpandableDetails(task);
	const parts = summaryParts(t, task);
	const taskSummaryText =
		statusText && activeStepDetail
			? statusText.toLocaleLowerCase() === activeStepDetail.toLocaleLowerCase()
				? activeStepDetail
				: statusText
			: statusText || activeStepDetail;

	return (
		<Card className="gap-0 p-0">
			<div className="flex min-h-16 flex-col gap-3 px-4 py-3 md:flex-row md:items-center md:justify-between md:px-5">
				<button
					type="button"
					className="flex min-w-0 flex-1 items-center gap-3 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/45"
					aria-controls={hasExpandableDetails ? detailsSectionId : undefined}
					aria-expanded={hasExpandableDetails ? detailsExpanded : undefined}
					onClick={() => {
						if (hasExpandableDetails) {
							onToggleDetails(task.id);
						}
					}}
				>
					<span className="flex size-8 shrink-0 items-center justify-center rounded-lg border border-border/70 bg-muted/25 text-muted-foreground">
						<Icon name={taskIcon(task)} className="size-4" />
					</span>
					<span className="flex min-w-0 flex-wrap items-center gap-2 text-sm md:text-base">
						{parts.map((part) =>
							part.kind === "chip" ? (
								<TaskSummaryChip
									key={`${part.kind}-${part.icon}-${part.value}`}
									icon={part.icon}
									value={part.value}
								/>
							) : (
								<span
									key={`${part.kind}-${part.value}`}
									className="text-foreground"
								>
									{part.value}
								</span>
							),
						)}
					</span>
				</button>

				<div className="flex shrink-0 items-center justify-between gap-2 md:justify-end">
					<div className="flex items-center gap-2">
						<Badge variant={statusBadgeVariant(task.status)}>
							{formatTaskStatus(t, task.status)}
						</Badge>
						<Badge variant="outline" className="max-md:hidden">
							{formatTaskKind(t, task.kind)}
						</Badge>
					</div>
					{hasExpandableDetails ? (
						<Button
							variant="ghost"
							size="icon-sm"
							aria-controls={detailsSectionId}
							aria-expanded={detailsExpanded}
							aria-label={
								detailsExpanded
									? t("tasks:hide_details")
									: t("tasks:show_details")
							}
							title={
								detailsExpanded
									? t("tasks:hide_details")
									: t("tasks:show_details")
							}
							onClick={() => onToggleDetails(task.id)}
						>
							<Icon
								name={detailsExpanded ? "CaretUp" : "CaretDown"}
								className="size-4"
							/>
						</Button>
					) : null}
				</div>
			</div>

			<AnimatedTaskDetails open={detailsExpanded} className="border-t">
				<div id={detailsSectionId} className="space-y-5 px-4 py-4 md:px-5">
					{task.last_error ? (
						<div className="flex gap-3 rounded-lg border border-destructive/20 bg-destructive/8 px-3 py-2.5 text-sm text-destructive">
							<Icon name="CircleAlert" className="mt-0.5 size-4 shrink-0" />
							<div>{task.last_error}</div>
						</div>
					) : null}

					<div className="space-y-3">
						<h3 className="text-sm font-semibold">
							{t("tasks:task_progress_title")}
						</h3>
						<TaskStepsPreview task={task} />
					</div>

					{taskSummaryText ? (
						<p
							className={`text-sm ${task.last_error ? "text-destructive" : "text-muted-foreground"}`}
						>
							{t("tasks:status_text_label")}: {taskSummaryText}
						</p>
					) : null}

					<div className="space-y-3 border-t pt-4">
						<h3 className="text-sm font-semibold">
							{t("tasks:task_details_title")}
						</h3>
						<TaskDetailsContent task={task} />
					</div>

					<div className="flex flex-wrap items-center gap-2 border-t pt-4">
						<span className="text-xs text-muted-foreground">
							{t("tasks:task_id_label", { id: task.id })}
						</span>
						{summaryTimestamp ? (
							<span
								className={`text-xs font-medium ${taskMetaTextClass(task.status)}`}
							>
								{summaryTimestamp}
							</span>
						) : null}
						{task.status === "succeeded" && parsedResult ? (
							<Button
								variant="outline"
								size="sm"
								onClick={() =>
									onOpenTargetFolder(parsedResult.target_folder_id ?? null)
								}
							>
								<Icon name="FolderOpen" className="mr-1 size-4" />
								{t("tasks:open_target_folder")}
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
									className={`mr-1 size-4 ${retrying ? "animate-spin" : ""}`}
								/>
								{t("tasks:retry_task")}
							</Button>
						) : null}
					</div>
				</div>
			</AnimatedTaskDetails>
		</Card>
	);
}
