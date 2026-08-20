import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon, type IconName } from "@/components/ui/icon";
import type { TaskInfo } from "@/types/api";
import { AnimatedTaskDetails } from "./AnimatedTaskDetails";
import { TaskDetailsContent, TaskStepsPreview } from "./TaskDetailsPanel";
import { taskHasExpandableDetails } from "./taskDetails";
import {
	currentTaskStep,
	formatTaskDisplayName,
	formatTaskKind,
	formatTaskPresentationStatus,
	formatTaskStatus,
	parseTaskResult,
	statusBadgeVariant,
	taskMetaTextClass,
	taskSummaryTimestamp,
	trimTaskStatus,
} from "./taskPresentation";

type SummaryPart =
	| { key: string; kind: "text"; value: string }
	| { icon: IconName; key: string; kind: "chip"; value: string };

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
		case "offline_download":
			return "LinkSimple";
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
	const displayName = formatTaskDisplayName(t, task);
	if (task.kind === "media_metadata_extract") {
		const payload = task.payload as unknown as {
			source_file_name?: string;
		};
		return [
			{
				key: "action",
				kind: "text",
				value: t("tasks:summary_extract_metadata_for"),
			},
			{
				icon: "Info",
				key: "source-file",
				kind: "chip",
				value: payload.source_file_name || displayName,
			},
		];
	}
	switch (task.payload.kind) {
		case "archive_extract":
			return [
				{
					key: "action",
					kind: "text",
					value: t("tasks:summary_action_prefix"),
				},
				{
					icon: "FileZip",
					key: "source-file",
					kind: "chip",
					value: task.payload.source_file_name,
				},
				{
					key: "target-label",
					kind: "text",
					value: t("tasks:summary_migrate_to"),
				},
				{
					icon: "FolderOpen",
					key: "target-folder",
					kind: "chip",
					value:
						task.payload.output_folder_name || t("tasks:summary_root_folder"),
				},
			];
		case "archive_compress": {
			const selectedCount =
				task.payload.file_ids.length + task.payload.folder_ids.length;
			return [
				{
					key: "action",
					kind: "text",
					value: t("tasks:summary_action_prefix"),
				},
				{
					icon: "Folder",
					key: "selection",
					kind: "chip",
					value: t("tasks:summary_selected_items", { count: selectedCount }),
				},
				{
					key: "target-label",
					kind: "text",
					value: t("tasks:summary_archive_compress_to"),
				},
				{
					icon: "FileZip",
					key: "archive-file",
					kind: "chip",
					value: task.payload.archive_name,
				},
			];
		}
		case "archive_preview_generate":
			return [
				{
					key: "action",
					kind: "text",
					value: t("tasks:summary_generate_preview_for"),
				},
				{
					icon: "FileZip",
					key: "source-file",
					kind: "chip",
					value: task.payload.source_file_name,
				},
			];
		case "thumbnail_generate":
			return [
				{
					key: "action",
					kind: "text",
					value: t("tasks:summary_generate_thumbnail_for"),
				},
				{
					icon: "FileImage",
					key: "source-file",
					kind: "chip",
					value: task.payload.source_file_name || displayName,
				},
			];
		case "trash_purge_all":
			return [
				{
					key: "action",
					kind: "text",
					value: t("tasks:summary_purge_trash"),
				},
			];
		case "storage_policy_migration":
			return [
				{
					key: "action",
					kind: "text",
					value: t("tasks:summary_migrate_storage_policy"),
				},
				{
					icon: "HardDrive",
					key: "source-policy",
					kind: "chip",
					value: t("tasks:summary_policy_id", {
						id: task.payload.source_policy_id,
					}),
				},
				{
					key: "target-label",
					kind: "text",
					value: t("tasks:summary_archive_extract_to"),
				},
				{
					icon: "HardDrive",
					key: "target-policy",
					kind: "chip",
					value: t("tasks:summary_policy_id", {
						id: task.payload.target_policy_id,
					}),
				},
			];
		case "storage_policy_temp_cleanup":
			return [
				{
					key: "action",
					kind: "text",
					value: t("tasks:summary_cleanup_temp_files"),
				},
			];
		case "offline_download": {
			const parts: SummaryPart[] = [
				{
					key: "action",
					kind: "text",
					value: t("tasks:summary_import_from_link"),
				},
				{
					icon: "LinkSimple",
					key: "source",
					kind: "chip",
					value: task.payload.source_display_url,
				},
			];
			if (task.payload.filename) {
				parts.push(
					{
						key: "filename-label",
						kind: "text",
						value: t("tasks:summary_filename"),
					},
					{
						icon: "File",
						key: "filename",
						kind: "chip",
						value: task.payload.filename,
					},
				);
			}
			return parts;
		}
		case "system_runtime":
			return [
				{
					key: "action",
					kind: "text",
					value: t("tasks:summary_system_runtime", {
						name: displayName,
					}),
				},
			];
		default:
			return [{ key: "display-name", kind: "text", value: displayName }];
	}
}

function TaskSummaryChip({ icon, value }: { icon: IconName; value: string }) {
	return (
		<span className="inline-flex max-w-full items-center gap-1 rounded-md bg-muted/50 px-2 py-0.5 text-xs font-medium text-foreground">
			<Icon name={icon} className="size-3.5 shrink-0 text-muted-foreground" />
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
	const localizedActiveStepDetail = trimTaskStatus(activeStepDetail);
	const localizedStatusText =
		formatTaskPresentationStatus(t, task) ?? trimTaskStatus(statusText);
	const summaryTimestamp = taskSummaryTimestamp(t, task);
	const detailsSectionId = `task-details-${task.id}`;
	const hasExpandableDetails = taskHasExpandableDetails(task);
	const parts = summaryParts(t, task);
	const taskSummaryText =
		statusText && activeStepDetail
			? statusText.toLocaleLowerCase() === activeStepDetail.toLocaleLowerCase()
				? localizedActiveStepDetail
				: localizedStatusText
			: localizedStatusText || localizedActiveStepDetail;

	return (
		// D9 去框化：任务条目用常态柔和色垫分区（不用描边），hover 加深一档，
		// 展开详情内的分隔线保留（内容分区，非容器框）
		<div
			data-slot="card"
			className="rounded-xl bg-muted/45 transition-colors hover:bg-muted/60 dark:bg-muted/20 dark:hover:bg-muted/30"
		>
			{/* biome-ignore lint/a11y/noStaticElementInteractions: the nested summary button remains the keyboard-accessible control; this extends its mouse hit area to the row padding. */}
			{/* biome-ignore lint/a11y/useKeyWithClickEvents: the nested summary button provides keyboard activation; this handler only extends the pointer hit area. */}
			<div
				className="flex min-h-14 flex-col gap-2 px-3 py-2.5 md:flex-row md:items-center md:justify-between md:px-4"
				onClick={(event) => {
					if (!hasExpandableDetails) {
						return;
					}
					if (
						event.target instanceof Element &&
						event.target.closest("button, a, input, textarea, select")
					) {
						return;
					}
					onToggleDetails(task.id);
				}}
			>
				<button
					type="button"
					className="flex min-w-0 flex-1 items-center gap-2.5 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/45"
					aria-controls={hasExpandableDetails ? detailsSectionId : undefined}
					aria-expanded={hasExpandableDetails ? detailsExpanded : undefined}
					onClick={() => {
						if (hasExpandableDetails) {
							onToggleDetails(task.id);
						}
					}}
				>
					<span className="flex size-7 shrink-0 items-center justify-center rounded-md bg-muted/50 text-muted-foreground">
						<Icon name={taskIcon(task)} className="size-3.5" />
					</span>
					<span className="flex min-w-0 flex-wrap items-center gap-1.5 text-sm md:text-[0.95rem]">
						{parts.map((part) =>
							part.kind === "chip" ? (
								<TaskSummaryChip
									key={part.key}
									icon={part.icon}
									value={part.value}
								/>
							) : (
								<span key={part.key} className="text-foreground">
									{part.value}
								</span>
							),
						)}
					</span>
				</button>

				<div className="flex shrink-0 items-center justify-between gap-1.5 md:justify-end">
					<div className="flex items-center gap-1.5">
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
							size="icon-xs"
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
								className="size-3.5"
							/>
						</Button>
					) : null}
				</div>
			</div>

			<AnimatedTaskDetails open={detailsExpanded} className="border-t">
				<div id={detailsSectionId} className="space-y-4 px-3 py-3 md:px-4">
					{task.last_error ? (
						<div className="flex gap-2.5 rounded-lg border border-destructive/20 bg-destructive/8 px-2.5 py-2 text-sm text-destructive">
							<Icon name="CircleAlert" className="mt-0.5 size-3.5 shrink-0" />
							<div>{task.last_error}</div>
						</div>
					) : null}

					<div className="space-y-2.5">
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

					<div className="space-y-2.5 border-t pt-3">
						<h3 className="text-sm font-semibold">
							{t("tasks:task_details_title")}
						</h3>
						<TaskDetailsContent task={task} />
					</div>

					<div className="flex flex-wrap items-center gap-2 border-t pt-3">
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
								size="xs"
								onClick={() =>
									onOpenTargetFolder(parsedResult.target_folder_id ?? null)
								}
							>
								<Icon name="FolderOpen" className="mr-1 size-3.5" />
								{t("tasks:open_target_folder")}
							</Button>
						) : null}
						{task.can_retry ? (
							<Button
								variant="outline"
								size="xs"
								onClick={() => onRetry(task.id)}
								disabled={retrying}
							>
								<Icon
									name={retrying ? "Spinner" : "ArrowCounterClockwise"}
									className={`mr-1 size-3.5 ${retrying ? "animate-spin" : ""}`}
								/>
								{t("tasks:retry_task")}
							</Button>
						) : null}
					</div>
				</div>
			</AnimatedTaskDetails>
		</div>
	);
}
