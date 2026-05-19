import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { cloneElement, isValidElement } from "react";
import { MemoryRouter, useLocation } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AdminTasksPage from "@/pages/admin/AdminTasksPage";
import type { TaskInfo, UserSummary } from "@/types/api";

const mockState = vi.hoisted(() => ({
	cleanupCompleted: vi.fn(),
	handleApiError: vi.fn(),
	list: vi.fn(),
	toastSuccess: vi.fn(),
}));

function createUserSummary(
	id = 7,
	username = "root",
	displayName = "Root",
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

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) => {
			if (key === "admin:entries_page" || key === "entries_page") {
				return `entries:${options?.current}/${options?.pages}/${options?.total}`;
			}
			if (key === "admin:overview_background_tasks_source_system") {
				return "source:system";
			}
			if (key === "admin:overview_background_tasks_source_user") {
				return `source:user:${options?.id}`;
			}
			if (key === "admin:overview_background_tasks_source_team") {
				return `source:team:${options?.id}`;
			}
			if (key === "admin:page_size_option" || key === "page_size_option") {
				return `size:${options?.count}`;
			}
			if (key === "admin:tasks_cleaned" || key === "tasks_cleaned") {
				return `tasks_cleaned:${options?.count}`;
			}
			if (
				key === "admin:task_cleanup_confirm_desc" ||
				key === "task_cleanup_confirm_desc"
			) {
				return `cleanup-desc:${options?.finishedBefore}:${options?.kind}:${options?.status}`;
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
	EmptyState: ({
		action,
		title,
		description,
		icon,
	}: {
		action?: React.ReactNode;
		title: string;
		description?: string;
		icon?: React.ReactNode;
	}) => (
		<div>
			<div>{title}</div>
			<div>{description}</div>
			<div>{icon}</div>
			<div>{action}</div>
		</div>
	),
}));

vi.mock("@/components/common/SkeletonTable", () => ({
	SkeletonTable: ({ columns, rows }: { columns: number; rows: number }) => (
		<div>{`skeleton:${columns}:${rows}`}</div>
	),
}));

vi.mock("@/components/layout/AdminLayout", () => ({
	AdminLayout: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/layout/AdminPageHeader", () => ({
	AdminPageHeader: ({
		title,
		description,
		actions,
		toolbar,
	}: {
		title: string;
		description: string;
		actions?: React.ReactNode;
		toolbar?: React.ReactNode;
	}) => (
		<div>
			<h1>{title}</h1>
			<p>{description}</p>
			<div>{actions}</div>
			<div>{toolbar}</div>
		</div>
	),
}));

vi.mock("@/components/layout/AdminPageShell", () => ({
	AdminPageShell: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/layout/AdminSurface", () => ({
	AdminSurface: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({ children }: { children: React.ReactNode }) => (
		<span>{children}</span>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
		type,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		type?: "button" | "submit" | "reset";
	}) => (
		<button type={type ?? "button"} disabled={disabled} onClick={onClick}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({ children, open }: { children: React.ReactNode; open: boolean }) =>
		open ? <div>{children}</div> : null,
	DialogContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogDescription: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogFooter: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({
		id,
		value,
		onChange,
		placeholder,
		"aria-label": ariaLabel,
		type,
	}: {
		id?: string;
		value?: string;
		onChange?: (event: { target: { value: string } }) => void;
		placeholder?: string;
		"aria-label"?: string;
		type?: string;
	}) => (
		<input
			id={id}
			aria-label={ariaLabel}
			placeholder={placeholder}
			type={type}
			value={value}
			onChange={(event) =>
				onChange?.({ target: { value: event.target.value } })
			}
		/>
	),
}));

vi.mock("@/components/ui/label", () => ({
	Label: ({
		children,
		htmlFor,
	}: {
		children: React.ReactNode;
		htmlFor?: string;
	}) => <label htmlFor={htmlFor}>{children}</label>,
}));

vi.mock("@/components/ui/scroll-area", () => ({
	ScrollArea: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
}));

vi.mock("@/components/ui/select", () => ({
	Select: ({
		children,
		items,
		onValueChange,
		value,
	}: {
		children: React.ReactNode;
		items?: Array<{ label: string; value: string }>;
		onValueChange?: (value: string) => void;
		value?: string;
	}) => (
		<div>
			<div>{`select:${value}`}</div>
			{items?.map((item) => (
				<button
					key={item.value}
					type="button"
					onClick={() => onValueChange?.(item.value)}
				>
					{`select-${item.value}`}
				</button>
			))}
			{children}
		</div>
	),
	SelectContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	SelectItem: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	SelectTrigger: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
	SelectValue: () => <span>select-value</span>,
}));

vi.mock("@/components/ui/table", () => ({
	Table: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
	TableBody: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	TableCell: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	TableHead: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	TableHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	TableRow: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/ui/tooltip", () => ({
	Tooltip: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	TooltipContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	TooltipProvider: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	TooltipTrigger: ({
		children,
		render,
	}: {
		children: React.ReactNode;
		render?: React.ReactElement;
	}) =>
		render && isValidElement(render) ? (
			cloneElement(render, undefined, children)
		) : (
			<div>{children}</div>
		),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/lib/format", () => ({
	formatDateAbsolute: (value: string) => `date:${value}`,
	formatDateAbsoluteWithOffset: (value: string) => `date-with-offset:${value}`,
	formatDateTime: (value: string) => `time:${value}`,
	formatNumber: (value: number) => String(value),
}));

vi.mock("@/services/adminService", () => ({
	adminTaskService: {
		cleanupCompleted: (...args: unknown[]) =>
			mockState.cleanupCompleted(...args),
		list: (...args: unknown[]) => mockState.list(...args),
	},
}));

function createTask(overrides: Partial<TaskInfo> = {}): TaskInfo {
	return {
		attempt_count: 0,
		can_retry: false,
		created_at: "2026-04-17T00:00:00Z",
		creator: createUserSummary(),
		display_name: "Extract report archive",
		expires_at: "2026-04-18T00:00:00Z",
		finished_at: null,
		id: 31,
		kind: "archive_extract",
		last_error: null,
		max_attempts: 1,
		payload: {
			kind: "archive_extract",
			file_id: 9,
			output_folder_name: "report",
			source_file_name: "report.zip",
			target_folder_id: 2,
		},
		progress_current: 3,
		progress_percent: 60,
		progress_total: 5,
		result: null,
		share_id: null,
		started_at: "2026-04-17T00:01:00Z",
		status: "processing",
		status_text: "extracting entries",
		steps: [],
		team_id: null,
		updated_at: "2026-04-17T00:03:00Z",
		...overrides,
	};
}

function renderPage(initialEntry = "/admin/tasks") {
	return render(
		<MemoryRouter initialEntries={[initialEntry]}>
			<LocationProbe />
			<AdminTasksPage />
		</MemoryRouter>,
	);
}

function LocationProbe() {
	const location = useLocation();

	return <div data-testid="location-search">{location.search}</div>;
}

describe("AdminTasksPage", () => {
	beforeEach(() => {
		mockState.cleanupCompleted.mockReset();
		mockState.handleApiError.mockReset();
		mockState.list.mockReset();
		mockState.toastSuccess.mockReset();

		mockState.cleanupCompleted.mockResolvedValue({ removed: 2 });
		mockState.list.mockResolvedValue({
			items: [createTask()],
			total: 1,
		});
	});

	it("shows a loading skeleton while the task request is pending", () => {
		mockState.list.mockImplementationOnce(() => new Promise(() => undefined));

		renderPage();

		expect(screen.getByText("skeleton:8:6")).toBeInTheDocument();
	});

	it("renders the empty state when there are no recorded tasks", async () => {
		mockState.list.mockResolvedValueOnce({
			items: [],
			total: 0,
		});

		renderPage();

		expect(await screen.findByText("admin:no_tasks")).toBeInTheDocument();
		expect(screen.getByText("admin:no_tasks_desc")).toBeInTheDocument();
		expect(screen.getByText("Clock")).toBeInTheDocument();
	});

	it("renders tasks, paginates, and refreshes the list", async () => {
		mockState.list
			.mockResolvedValueOnce({
				items: [
					createTask(),
					createTask({
						id: 32,
						creator: null,
						display_name: "Trash cleanup",
						kind: "system_runtime",
						payload: {
							kind: "system_runtime",
							task_name: "trash-cleanup",
						},
						progress_current: 1,
						progress_percent: 100,
						progress_total: 1,
						status: "succeeded",
						status_text: "cleaned up 4 items",
						team_id: null,
						updated_at: "2026-04-17T00:05:00Z",
					}),
				],
				total: 25,
			})
			.mockResolvedValueOnce({
				items: [
					createTask({
						id: 41,
						display_name: "Compress team export",
						kind: "archive_compress",
						last_error: "zip writer failed",
						status: "failed",
						team_id: 8,
						updated_at: "2026-04-17T00:07:00Z",
					}),
				],
				total: 25,
			})
			.mockResolvedValueOnce({
				items: [createTask({ id: 52 })],
				total: 25,
			});

		renderPage();

		await waitFor(() => {
			expect(mockState.list).toHaveBeenNthCalledWith(1, {
				limit: 20,
				offset: 0,
				sort_by: "updated_at",
				sort_order: "desc",
			});
		});
		expect(screen.getByText("Extract report archive")).toBeInTheDocument();
		expect(screen.getByText("Trash cleanup")).toBeInTheDocument();
		expect(screen.getByText("Root")).toBeInTheDocument();
		expect(screen.getByText("source:system")).toBeInTheDocument();
		expect(screen.getByText("60%")).toBeInTheDocument();
		expect(screen.queryByText("#31")).not.toBeInTheDocument();
		expect(screen.queryByText("3 / 5")).not.toBeInTheDocument();
		expect(screen.getAllByText("date:2026-04-17T00:01:00Z")).toHaveLength(2);
		expect(screen.getByText("entries:1/2/25")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "CaretRight" }));

		await waitFor(() => {
			expect(mockState.list).toHaveBeenNthCalledWith(2, {
				limit: 20,
				offset: 20,
				sort_by: "updated_at",
				sort_order: "desc",
			});
		});
		expect(await screen.findByText("source:team:8")).toBeInTheDocument();
		expect(screen.getByText("zip writer failed")).toBeInTheDocument();

		fireEvent.click(screen.getAllByRole("button", { name: "select-50" })[0]);

		await waitFor(() => {
			expect(mockState.list).toHaveBeenNthCalledWith(3, {
				limit: 50,
				offset: 0,
				sort_by: "updated_at",
				sort_order: "desc",
			});
		});
	});

	it("reads filters from the url and clears them in one update", async () => {
		mockState.list.mockResolvedValueOnce({
			items: [createTask({ status: "failed", kind: "archive_compress" })],
			total: 1,
		});

		renderPage("/admin/tasks?kind=archive_compress&status=failed");

		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledWith({
				kind: "archive_compress",
				limit: 20,
				offset: 0,
				sort_by: "updated_at",
				sort_order: "desc",
				status: "failed",
			});
		});

		fireEvent.click(screen.getByRole("button", { name: /clear_filters/ }));

		await waitFor(() => {
			expect(screen.getByTestId("location-search").textContent).toBe("");
		});
	});

	it("accepts trash purge filters from the url", async () => {
		mockState.list.mockResolvedValueOnce({
			items: [
				createTask({
					display_name: "Empty trash",
					kind: "trash_purge_all",
					payload: { kind: "trash_purge_all" },
					progress_current: 0,
					progress_percent: 100,
					progress_total: 0,
					status: "succeeded",
					status_text: "purged 3 items",
				}),
			],
			total: 1,
		});

		renderPage("/admin/tasks?kind=trash_purge_all");

		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledWith({
				kind: "trash_purge_all",
				limit: 20,
				offset: 0,
				sort_by: "updated_at",
				sort_order: "desc",
			});
		});
		expect(screen.getByText("Empty trash")).toBeInTheDocument();
		expect(
			screen.getAllByText("tasks:kind_trash_purge_all").length,
		).toBeGreaterThan(0);
		expect(screen.getByText("select:trash_purge_all")).toBeInTheDocument();
	});

	it("cleans up completed tasks from the dialog and reloads the list", async () => {
		mockState.list
			.mockResolvedValueOnce({
				items: [createTask({ status: "failed" })],
				total: 1,
			})
			.mockResolvedValueOnce({
				items: [createTask({ id: 91, status: "succeeded" })],
				total: 1,
			});

		renderPage();

		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledTimes(1);
		});

		fireEvent.click(
			screen.getByRole("button", { name: /task_cleanup_action/ }),
		);

		expect(screen.getByText("admin:task_cleanup_title")).toBeInTheDocument();
		const finishedBefore = "2026-04-20T12:30";
		const finishedBeforeIso = new Date(finishedBefore).toISOString();
		fireEvent.change(
			screen.getByLabelText("admin:task_cleanup_finished_before"),
			{
				target: { value: finishedBefore },
			},
		);
		expect(
			screen.getByText(
				`cleanup-desc:time:${finishedBeforeIso}:admin:all_task_types:admin:all_completed_task_statuses`,
			),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", {
				name: /cleanup_completed_tasks/,
			}),
		);

		await waitFor(() => {
			expect(mockState.cleanupCompleted).toHaveBeenCalledWith({
				finished_before: finishedBeforeIso,
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("tasks_cleaned:2");
		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledTimes(2);
		});
	});

	it("routes loading failures through handleApiError", async () => {
		const loadError = new Error("tasks failed");
		mockState.list.mockRejectedValueOnce(loadError);

		renderPage();

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(loadError);
		});
	});

	it("routes cleanup failures through handleApiError", async () => {
		const cleanupError = new Error("cleanup failed");
		mockState.cleanupCompleted.mockRejectedValueOnce(cleanupError);

		renderPage();

		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledTimes(1);
		});

		fireEvent.click(
			screen.getByRole("button", { name: /task_cleanup_action/ }),
		);
		const finishedBefore = "2026-04-21T08:00";
		const finishedBeforeIso = new Date(finishedBefore).toISOString();
		fireEvent.change(
			screen.getByLabelText("admin:task_cleanup_finished_before"),
			{
				target: { value: finishedBefore },
			},
		);
		fireEvent.click(
			screen.getByRole("button", { name: /cleanup_completed_tasks/ }),
		);

		await waitFor(() => {
			expect(mockState.cleanupCompleted).toHaveBeenCalledWith({
				finished_before: finishedBeforeIso,
			});
			expect(mockState.handleApiError).toHaveBeenCalledWith(cleanupError);
		});
	});
});
