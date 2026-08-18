import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TaskCard } from "@/pages/tasks/TaskCard";
import type { TaskInfo } from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) => {
			if (key === "tasks:summary_selected_items") {
				return `selected:${options?.count}`;
			}
			if (key === "tasks:summary_policy_id") {
				return `policy:${options?.id}`;
			}
			if (key === "tasks:task_id_label") {
				return `task:${options?.id}`;
			}
			if (
				key === "tasks:summary_created_at" ||
				key === "tasks:summary_finished_at"
			) {
				return `${key}:${options?.date}`;
			}
			return key;
		},
	}),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({ children }: { children: React.ReactNode }) => (
		<span>{children}</span>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		"aria-controls": ariaControls,
		"aria-expanded": ariaExpanded,
		"aria-label": ariaLabel,
		children,
		disabled,
		onClick,
		title,
	}: {
		"aria-controls"?: string;
		"aria-expanded"?: boolean;
		"aria-label"?: string;
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		title?: string;
	}) => (
		<button
			type="button"
			aria-controls={ariaControls}
			aria-expanded={ariaExpanded}
			aria-label={ariaLabel}
			disabled={disabled}
			onClick={onClick}
			title={title}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/card", () => ({
	Card: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{`icon:${name}`}</span>,
}));

vi.mock("@/pages/tasks/AnimatedTaskDetails", () => ({
	AnimatedTaskDetails: ({
		children,
		open,
	}: {
		children: React.ReactNode;
		open: boolean;
	}) => (open ? <div>{children}</div> : null),
}));

vi.mock("@/pages/tasks/TaskDetailsPanel", () => ({
	TaskDetailsContent: ({ task }: { task: TaskInfo }) => (
		<div>{`details:${task.id}`}</div>
	),
	TaskStepsPreview: ({ task }: { task: TaskInfo }) => (
		<div>{`steps:${task.steps.length}`}</div>
	),
}));

vi.mock("@/lib/format", () => ({
	formatDateAbsolute: (value: string) => `date:${value}`,
	formatNumber: (value: number) => String(value),
}));

function createTask(overrides: Partial<TaskInfo> = {}): TaskInfo {
	return {
		attempt_count: 0,
		can_retry: false,
		created_at: "2026-04-17T00:00:00Z",
		creator: null,
		display_name: "Download https://example.com/file.bin",
		expires_at: "2026-04-18T00:00:00Z",
		finished_at: null,
		id: 44,
		kind: "offline_download",
		last_error: null,
		max_attempts: 1,
		payload: {
			expected_sha256: null,
			filename: "file.bin",
			kind: "offline_download",
			source_display_url: "https://example.com/file.bin",
			target_folder_id: 9,
		},
		progress_current: 0,
		progress_percent: 0,
		progress_total: 0,
		result: null,
		share_id: null,
		started_at: null,
		status: "pending",
		status_text: null,
		steps: [],
		team_id: null,
		updated_at: "2026-04-17T00:00:00Z",
		...overrides,
	};
}

describe("TaskCard", () => {
	it("renders offline download summaries and opens the result target folder", () => {
		const onOpenTargetFolder = vi.fn();
		const onToggleDetails = vi.fn();

		render(
			<TaskCard
				detailsExpanded
				onOpenTargetFolder={onOpenTargetFolder}
				onRetry={vi.fn()}
				onToggleDetails={onToggleDetails}
				retrying={false}
				task={createTask({
					finished_at: "2026-04-17T00:03:00Z",
					result: {
						content_length: 512,
						file_id: 70,
						file_name: "file.bin",
						file_path: "/Incoming/file.bin",
						folder_id: 9,
						kind: "offline_download",
						sha256: "abc123",
						source_display_url: "https://example.com/file.bin",
					},
					status: "succeeded",
					status_text: "Imported file.bin",
				})}
			/>,
		);

		expect(screen.getAllByText("icon:LinkSimple")).toHaveLength(2);
		expect(
			screen.getByText("tasks:summary_import_from_link"),
		).toBeInTheDocument();
		expect(
			screen.getByText("https://example.com/file.bin"),
		).toBeInTheDocument();
		expect(screen.getByText("tasks:summary_filename")).toBeInTheDocument();
		expect(screen.getByText("file.bin")).toBeInTheDocument();
		expect(screen.getByText("tasks:kind_offline_download")).toBeInTheDocument();
		expect(screen.getByText(/tasks:status_text_label/)).toBeInTheDocument();
		expect(screen.getByText("details:44")).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: /tasks:open_target_folder/ }),
		);
		expect(onOpenTargetFolder).toHaveBeenCalledWith(9);
		fireEvent.click(screen.getByRole("button", { name: /tasks:hide_details/ }));
		expect(onToggleDetails).toHaveBeenCalledWith(44);
	});

	it("uses active step detail when it only differs by casing from status text", () => {
		render(
			<TaskCard
				detailsExpanded
				onOpenTargetFolder={vi.fn()}
				onRetry={vi.fn()}
				onToggleDetails={vi.fn()}
				retrying={false}
				task={createTask({
					status: "processing",
					status_text: "downloading file",
					steps: [
						{
							detail: "Downloading File",
							finished_at: null,
							key: "download",
							progress_current: 1,
							progress_total: 10,
							started_at: "2026-04-17T00:01:00Z",
							status: "active",
							title: "Download",
						},
					],
				})}
			/>,
		);

		expect(screen.getByText(/Downloading File/)).toBeInTheDocument();
		expect(screen.queryByText(/downloading file/)).not.toBeInTheDocument();
	});

	it("toggles details when clicking the summary row padding", () => {
		const onToggleDetails = vi.fn();
		render(
			<TaskCard
				detailsExpanded={false}
				onOpenTargetFolder={vi.fn()}
				onRetry={vi.fn()}
				onToggleDetails={onToggleDetails}
				retrying={false}
				task={createTask({
					steps: [
						{
							detail: "Downloading file",
							finished_at: null,
							key: "download",
							progress_current: 1,
							progress_total: 10,
							started_at: "2026-04-17T00:01:00Z",
							status: "active",
							title: "Download",
						},
					],
				})}
			/>,
		);

		const summaryButton = screen.getByRole("button", {
			name: /tasks:summary_import_from_link/,
		});
		fireEvent.click(summaryButton.parentElement as HTMLElement);
		fireEvent.click(summaryButton.parentElement as HTMLElement);

		expect(onToggleDetails).toHaveBeenCalledTimes(2);
		expect(onToggleDetails).toHaveBeenNthCalledWith(1, 44);
		expect(onToggleDetails).toHaveBeenNthCalledWith(2, 44);
	});

	it("falls back to display names for unknown task kinds and retry actions", () => {
		const onRetry = vi.fn();

		render(
			<TaskCard
				detailsExpanded
				onOpenTargetFolder={vi.fn()}
				onRetry={onRetry}
				onToggleDetails={vi.fn()}
				retrying
				task={createTask({
					can_retry: true,
					display_name: "Future task",
					kind: "future_task" as never,
					last_error: "Task failed",
					payload: { kind: "future_task" } as never,
					status: "failed",
				})}
			/>,
		);

		expect(screen.getByText("icon:Queue")).toBeInTheDocument();
		expect(screen.getByText("Future task")).toBeInTheDocument();
		const retryButton = screen.getByRole("button", {
			name: /tasks:retry_task/,
		});
		expect(retryButton).toBeDisabled();
		fireEvent.click(retryButton);
		expect(onRetry).not.toHaveBeenCalled();
	});
});
