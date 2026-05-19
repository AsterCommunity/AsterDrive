import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import TasksPage from "@/pages/TasksPage";
import type { TaskInfo, TaskStepInfo, UserSummary } from "@/types/api";

const mockState = vi.hoisted(() => ({
	handleApiError: vi.fn(),
	listInWorkspace: vi.fn(),
	navigate: vi.fn(),
	retryTask: vi.fn(),
	toastSuccess: vi.fn(),
}));

vi.mock("react-router-dom", () => ({
	useNavigate: () => mockState.navigate,
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) => {
			if (
				key === "tasks:pagination_desc" ||
				key === "tasks:progress_ratio" ||
				key === "tasks:task_id_label" ||
				key === "tasks:summary_created_at" ||
				key === "tasks:summary_started_at" ||
				key === "tasks:summary_finished_at" ||
				key === "tasks:summary_failed_at" ||
				key === "tasks:summary_canceled_at" ||
				key === "tasks:created_at" ||
				key === "tasks:started_at" ||
				key === "tasks:finished_at"
			) {
				return `${key}:${JSON.stringify(options ?? {})}`;
			}
			return key;
		},
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/common/EmptyState", () => ({
	EmptyState: (props: { description: string; title: string }) => (
		<div>{`${props.title}:${props.description}`}</div>
	),
}));

vi.mock("@/components/layout/AppLayout", () => ({
	AppLayout: (props: { children: React.ReactNode }) => (
		<div>{props.children}</div>
	),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: (props: { children: React.ReactNode }) => (
		<span>{props.children}</span>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: (props: {
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
			aria-controls={props["aria-controls"]}
			aria-expanded={props["aria-expanded"]}
			aria-label={props["aria-label"]}
			disabled={props.disabled}
			onClick={props.onClick}
			title={props.title}
		>
			{props.children}
		</button>
	),
}));

vi.mock("@/components/ui/card", () => ({
	Card: (props: { children?: React.ReactNode; className?: string }) => (
		<div className={props.className}>{props.children}</div>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: (props: { name: string }) => <span>{`icon:${props.name}`}</span>,
}));

vi.mock("@/components/ui/progress", () => ({
	Progress: (props: { value: number }) => (
		<div data-testid="progress" data-value={String(props.value)} />
	),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/hooks/usePageTitle", () => ({
	usePageTitle: () => undefined,
}));

vi.mock("@/lib/format", () => ({
	formatBytes: (value: number) => `bytes:${value}`,
	formatDateAbsolute: (value: string) => `date:${value}`,
	formatNumber: (value: number) => `num:${value}`,
}));

vi.mock("@/lib/workspace", () => ({
	workspaceFolderPath: (_workspace: unknown, folderId: number | null) =>
		folderId === null ? "/" : `/folder/${folderId}`,
}));

vi.mock("@/services/taskService", () => ({
	taskService: {
		listInWorkspace: (...args: unknown[]) => mockState.listInWorkspace(...args),
		retryTask: (...args: unknown[]) => mockState.retryTask(...args),
	},
}));

vi.mock("@/stores/workspaceStore", () => ({
	useWorkspaceStore: (
		selector: (state: { workspace: { kind: "personal" } }) => unknown,
	) => selector({ workspace: { kind: "personal" } }),
}));

function createTaskSteps(
	kind: TaskInfo["kind"] = "archive_extract",
	status: TaskInfo["status"] = "processing",
): TaskStepInfo[] {
	if (kind === "archive_compress") {
		if (status === "succeeded") {
			return [
				{
					detail: "Worker claimed task",
					finished_at: "2026-04-10T00:01:00Z",
					key: "waiting",
					progress_current: 0,
					progress_total: 0,
					started_at: "2026-04-10T00:01:00Z",
					status: "succeeded",
					title: "Waiting",
				},
				{
					detail: "Archive sources are ready",
					finished_at: "2026-04-10T00:02:00Z",
					key: "prepare_sources",
					progress_current: 0,
					progress_total: 0,
					started_at: "2026-04-10T00:01:10Z",
					status: "succeeded",
					title: "Prepare archive sources",
				},
				{
					detail: "Archive file created",
					finished_at: "2026-04-10T00:03:00Z",
					key: "build_archive",
					progress_current: 50,
					progress_total: 50,
					started_at: "2026-04-10T00:02:00Z",
					status: "succeeded",
					title: "Build archive",
				},
				{
					detail: "Saved archive as bundle.zip",
					finished_at: "2026-04-10T00:03:10Z",
					key: "store_result",
					progress_current: 0,
					progress_total: 0,
					started_at: "2026-04-10T00:03:00Z",
					status: "succeeded",
					title: "Save archive",
				},
			];
		}

		return [
			{
				detail: "Worker claimed task",
				finished_at: "2026-04-10T00:01:00Z",
				key: "waiting",
				progress_current: 0,
				progress_total: 0,
				started_at: "2026-04-10T00:01:00Z",
				status: "succeeded",
				title: "Waiting",
			},
			{
				detail: "Archive sources are ready",
				finished_at: "2026-04-10T00:02:00Z",
				key: "prepare_sources",
				progress_current: 0,
				progress_total: 0,
				started_at: "2026-04-10T00:01:10Z",
				status: "succeeded",
				title: "Prepare archive sources",
			},
			{
				detail: "Packing archive",
				finished_at: null,
				key: "build_archive",
				progress_current: 20,
				progress_total: 50,
				started_at: "2026-04-10T00:02:00Z",
				status: "active",
				title: "Build archive",
			},
			{
				detail: null,
				finished_at: null,
				key: "store_result",
				progress_current: 0,
				progress_total: 0,
				started_at: null,
				status: "pending",
				title: "Save archive",
			},
		];
	}

	if (status === "succeeded") {
		return [
			{
				detail: "Worker claimed task",
				finished_at: "2026-04-10T00:01:00Z",
				key: "waiting",
				progress_current: 0,
				progress_total: 0,
				started_at: "2026-04-10T00:01:00Z",
				status: "succeeded",
				title: "Waiting",
			},
			{
				detail: "Downloaded source archive",
				finished_at: "2026-04-10T00:02:00Z",
				key: "download_source",
				progress_current: 0,
				progress_total: 0,
				started_at: "2026-04-10T00:01:10Z",
				status: "succeeded",
				title: "Download source archive",
			},
			{
				detail: "Archive extracted to staging",
				finished_at: "2026-04-10T00:03:00Z",
				key: "extract_archive",
				progress_current: 50,
				progress_total: 50,
				started_at: "2026-04-10T00:02:00Z",
				status: "succeeded",
				title: "Extract to staging",
			},
			{
				detail: "Imported extracted files",
				finished_at: "2026-04-10T00:03:10Z",
				key: "import_result",
				progress_current: 0,
				progress_total: 0,
				started_at: "2026-04-10T00:03:00Z",
				status: "succeeded",
				title: "Import to workspace",
			},
		];
	}

	if (status === "failed") {
		return [
			{
				detail: "Worker claimed task",
				finished_at: "2026-04-10T00:01:00Z",
				key: "waiting",
				progress_current: 0,
				progress_total: 0,
				started_at: "2026-04-10T00:01:00Z",
				status: "succeeded",
				title: "Waiting",
			},
			{
				detail: "Downloaded source archive",
				finished_at: "2026-04-10T00:02:00Z",
				key: "download_source",
				progress_current: 0,
				progress_total: 0,
				started_at: "2026-04-10T00:01:10Z",
				status: "succeeded",
				title: "Download source archive",
			},
			{
				detail: "Unsupported archive format",
				finished_at: "2026-04-10T00:03:00Z",
				key: "extract_archive",
				progress_current: 10,
				progress_total: 50,
				started_at: "2026-04-10T00:02:00Z",
				status: "failed",
				title: "Extract to staging",
			},
			{
				detail: null,
				finished_at: null,
				key: "import_result",
				progress_current: 0,
				progress_total: 0,
				started_at: null,
				status: "pending",
				title: "Import to workspace",
			},
		];
	}

	return [
		{
			detail: "Worker claimed task",
			finished_at: "2026-04-10T00:01:00Z",
			key: "waiting",
			progress_current: 0,
			progress_total: 0,
			started_at: "2026-04-10T00:01:00Z",
			status: "succeeded",
			title: "Waiting",
		},
		{
			detail: "Downloaded source archive",
			finished_at: null,
			key: "download_source",
			progress_current: 0,
			progress_total: 0,
			started_at: "2026-04-10T00:01:10Z",
			status: "active",
			title: "Download source archive",
		},
		{
			detail: null,
			finished_at: null,
			key: "extract_archive",
			progress_current: 0,
			progress_total: 0,
			started_at: null,
			status: "pending",
			title: "Extract to staging",
		},
		{
			detail: null,
			finished_at: null,
			key: "import_result",
			progress_current: 0,
			progress_total: 0,
			started_at: null,
			status: "pending",
			title: "Import to workspace",
		},
	];
}

function createUserSummary(
	id = 1,
	username = "alice",
	displayName = "Alice",
): UserSummary {
	return {
		id,
		username,
		profile: {
			display_name: displayName,
			avatar: {
				source: "none",
				url_1024: null,
				url_512: null,
				version: 0,
			},
		},
	};
}

function createTask(overrides: Partial<TaskInfo> = {}): TaskInfo {
	const kind = overrides.kind ?? "archive_extract";
	const status = overrides.status ?? "processing";
	const payload =
		kind === "archive_compress"
			? {
					kind: "archive_compress" as const,
					file_ids: [],
					folder_ids: [],
					archive_name: "bundle-export.zip",
					target_folder_id: null,
				}
			: {
					kind: "archive_extract" as const,
					file_id: 99,
					source_file_name: "bundle.zip",
					target_folder_id: null,
					output_folder_name: "bundle",
				};

	return {
		attempt_count: 0,
		can_retry: false,
		created_at: "2026-04-10T00:00:00Z",
		creator: createUserSummary(),
		display_name: "Extract archive",
		expires_at: "2026-04-11T00:00:00Z",
		finished_at: null,
		id: 1,
		kind: "archive_extract",
		last_error: null,
		max_attempts: 3,
		payload,
		progress_current: 20,
		progress_percent: 40,
		progress_total: 50,
		result: null,
		share_id: null,
		started_at: "2026-04-10T00:01:00Z",
		status,
		status_text: "building archive",
		steps: overrides.steps ?? createTaskSteps(kind, status),
		team_id: null,
		updated_at: "2026-04-10T00:02:00Z",
		...overrides,
	};
}

describe("TasksPage", () => {
	beforeEach(() => {
		mockState.handleApiError.mockReset();
		mockState.listInWorkspace.mockReset();
		mockState.navigate.mockReset();
		mockState.retryTask.mockReset();
		mockState.retryTask.mockResolvedValue(undefined);
		mockState.toastSuccess.mockReset();
	});

	it("polls while active tasks are present", async () => {
		mockState.listInWorkspace.mockResolvedValue({
			items: [createTask()],
			total: 1,
		});

		render(<TasksPage />);

		await waitFor(() => {
			expect(mockState.listInWorkspace).toHaveBeenCalledWith({
				limit: 20,
				offset: 0,
			});
		});

		await waitFor(
			() => {
				expect(mockState.listInWorkspace).toHaveBeenCalledTimes(2);
			},
			{ timeout: 4000 },
		);
	});

	it("retries failed tasks", async () => {
		mockState.listInWorkspace
			.mockResolvedValueOnce({
				items: [
					createTask({
						can_retry: true,
						last_error: "failed once",
						status: "failed",
					}),
				],
				total: 1,
			})
			.mockResolvedValueOnce({
				items: [
					createTask({
						can_retry: false,
						status: "retry",
					}),
				],
				total: 1,
			});

		render(<TasksPage />);

		expect(await screen.findByText("Extract archive")).toBeInTheDocument();

		fireEvent.click(screen.getByText("tasks:retry_task"));

		await waitFor(() => {
			expect(mockState.retryTask).toHaveBeenCalledWith(1);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("tasks:retry_success");
		await waitFor(() => {
			expect(mockState.listInWorkspace).toHaveBeenCalledTimes(2);
		});
	});

	it("shows task steps before details are expanded and removes the summary progress bar", async () => {
		mockState.listInWorkspace.mockResolvedValue({
			items: [
				createTask({
					kind: "archive_compress",
					progress_current: 35,
					progress_percent: 35,
					progress_total: 100,
					status_text: "packing archive",
					steps: createTaskSteps("archive_compress", "processing"),
				}),
			],
			total: 1,
		});

		render(<TasksPage />);

		await screen.findByText("Extract archive");
		expect(screen.getByText("1. Waiting")).toBeInTheDocument();
		expect(screen.getByText("2. Prepare archive sources")).toBeInTheDocument();
		expect(screen.getByText("3. Build archive")).toBeInTheDocument();
		expect(screen.getByText("4. Save archive")).toBeInTheDocument();
		expect(screen.getByText(/Packing archive/)).toBeInTheDocument();
		expect(screen.getByText("tasks:step_progress_label")).toBeInTheDocument();
		expect(screen.getByText(/num:20 \/ num:50/)).toBeInTheDocument();
		expect(screen.queryByTestId("progress")).not.toBeInTheDocument();
		expect(
			screen.queryByText("tasks:progress_ratio_label"),
		).not.toBeInTheDocument();

		fireEvent.click(screen.getByText("tasks:show_details"));

		expect(await screen.findByText("tasks:timeline_label")).toBeInTheDocument();
		expect(screen.getAllByText("3. Build archive")).toHaveLength(1);
		expect(screen.getByText("tasks:progress_ratio_label")).toBeInTheDocument();
		expect(screen.getByText("num:35 / num:100")).toBeInTheDocument();
		expect(screen.getByTestId("progress")).toHaveAttribute("data-value", "35");
	});

	it("keeps the task card when a refreshed task no longer has step details", async () => {
		mockState.listInWorkspace
			.mockResolvedValueOnce({
				items: [
					createTask({
						kind: "archive_compress",
						steps: createTaskSteps("archive_compress", "processing"),
					}),
				],
				total: 1,
			})
			.mockResolvedValueOnce({
				items: [
					createTask({
						kind: "archive_compress",
						status_text: "waiting for worker",
						steps: [],
					}),
				],
				total: 1,
			});

		render(<TasksPage />);

		expect(await screen.findByText("Extract archive")).toBeInTheDocument();
		expect(screen.getByText("tasks:steps_label")).toBeInTheDocument();

		fireEvent.click(screen.getByLabelText("core:refresh"));

		await waitFor(() => {
			expect(mockState.listInWorkspace).toHaveBeenCalledTimes(2);
		});
		expect(screen.getByText("Extract archive")).toBeInTheDocument();
		expect(screen.queryByText("tasks:steps_label")).not.toBeInTheDocument();
		expect(screen.getByText(/waiting for worker/)).toBeInTheDocument();
	});

	it("renders summary timestamps by status and keeps detailed times in the expanded panel", async () => {
		mockState.listInWorkspace.mockResolvedValue({
			items: [
				createTask({
					finished_at: "2026-04-10T00:03:10Z",
					progress_current: 4152537914,
					progress_percent: 100,
					progress_total: 4152537914,
					status: "succeeded",
					status_text: null,
				}),
			],
			total: 1,
		});

		render(<TasksPage />);

		expect(
			await screen.findByText(
				'tasks:summary_finished_at:{"date":"date:2026-04-10T00:03:10Z"}',
			),
		).toBeInTheDocument();
		expect(
			screen.queryByText("num:4152537914 / num:4152537914"),
		).not.toBeInTheDocument();

		fireEvent.click(screen.getByText("tasks:show_details"));

		expect(
			await screen.findByText("tasks:timeline_created_label"),
		).toBeInTheDocument();
		expect(
			screen.getByText("tasks:timeline_started_label"),
		).toBeInTheDocument();
		expect(
			screen.getByText("tasks:timeline_finished_label"),
		).toBeInTheDocument();
		expect(
			screen.getByText("num:4152537914 / num:4152537914"),
		).toBeInTheDocument();
	});

	it("opens the target folder for completed tasks with parsed results", async () => {
		mockState.listInWorkspace.mockResolvedValue({
			items: [
				createTask({
					kind: "archive_compress",
					progress_current: 50,
					progress_percent: 100,
					result: {
						kind: "archive_compress",
						target_file_id: 100,
						target_file_name: "bundle.zip",
						target_folder_id: 42,
						target_path: "/Archives/bundle",
					},
					status: "succeeded",
					status_text: null,
				}),
			],
			total: 1,
		});

		render(<TasksPage />);

		await screen.findByText("Extract archive");
		fireEvent.click(screen.getByText("tasks:show_details"));

		expect(
			await screen.findByText("tasks:result_path_label"),
		).toBeInTheDocument();
		expect(screen.getByText("/Archives/bundle")).toBeInTheDocument();

		fireEvent.click(screen.getByText("tasks:open_target_folder"));

		expect(mockState.navigate).toHaveBeenCalledWith("/folder/42", {
			viewTransition: false,
		});
	});

	it("renders trash purge tasks without step details", async () => {
		mockState.listInWorkspace.mockResolvedValue({
			items: [
				createTask({
					display_name: "Empty trash",
					kind: "trash_purge_all",
					payload: { kind: "trash_purge_all" },
					progress_current: 0,
					progress_percent: 0,
					progress_total: 0,
					started_at: null,
					status: "pending",
					status_text: null,
					steps: [],
				}),
			],
			total: 1,
		});

		render(<TasksPage />);

		expect(await screen.findByText("Empty trash")).toBeInTheDocument();
		expect(screen.getByText("tasks:kind_trash_purge_all")).toBeInTheDocument();
		expect(screen.queryByText("tasks:steps_label")).not.toBeInTheDocument();
		expect(screen.queryByText("tasks:show_details")).not.toBeInTheDocument();
	});
});
