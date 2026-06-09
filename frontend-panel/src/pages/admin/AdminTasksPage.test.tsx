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
	resumeStoragePolicyMigration: vi.fn(),
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
				key === "admin:storage_migration_resume_queued" ||
				key === "storage_migration_resume_queued"
			) {
				return "resume_queued";
			}
			if (
				key === "admin:task_cleanup_confirm_desc" ||
				key === "task_cleanup_confirm_desc"
			) {
				return `cleanup-desc:${options?.finishedBefore}:${options?.kind}:${options?.status}`;
			}
			if (key === "tasks:step_storage_policy_migration_prepare_sources") {
				return "Prepare source policy";
			}
			if (key === "tasks:step_storage_policy_migration_finish") {
				return "Finish migration";
			}
			if (key === "tasks:storage_migration_migrated_blobs") {
				return "Migrated objects";
			}
			if (key === "tasks:storage_migration_skipped_blobs") {
				return "Skipped objects";
			}
			if (key === "tasks:storage_migration_failed_blobs") {
				return "Failed objects";
			}
			if (key === "tasks:storage_migration_migrated_bytes") {
				return "Bytes";
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
		...props
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: React.MouseEventHandler<HTMLButtonElement>;
		type?: "button" | "submit" | "reset";
	} & React.ButtonHTMLAttributes<HTMLButtonElement>) => (
		<button
			type={type ?? "button"}
			disabled={disabled}
			onClick={onClick}
			{...props}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({
		children,
		onOpenChange,
		open,
	}: {
		children: React.ReactNode;
		onOpenChange?: (open: boolean) => void;
		open: boolean;
	}) =>
		open ? (
			<div>
				{children}
				<button type="button" onClick={() => onOpenChange?.(false)}>
					X
				</button>
			</div>
		) : null,
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

vi.mock("@/components/ui/progress", () => ({
	Progress: ({ value }: { value?: number }) => <div>{`progress:${value}`}</div>,
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
	Table: ({
		children,
		...props
	}: React.HTMLAttributes<HTMLDivElement> & { children: React.ReactNode }) => (
		<div {...props}>{children}</div>
	),
	TableBody: ({
		children,
		...props
	}: React.HTMLAttributes<HTMLDivElement> & { children: React.ReactNode }) => (
		<div {...props}>{children}</div>
	),
	TableCell: ({
		children,
		...props
	}: React.HTMLAttributes<HTMLDivElement> & { children: React.ReactNode }) => (
		<div {...props}>{children}</div>
	),
	TableHead: ({
		children,
		...props
	}: React.HTMLAttributes<HTMLDivElement> & { children: React.ReactNode }) => (
		<div {...props}>{children}</div>
	),
	TableHeader: ({
		children,
		...props
	}: React.HTMLAttributes<HTMLDivElement> & { children: React.ReactNode }) => (
		<div {...props}>{children}</div>
	),
	TableRow: ({
		children,
		...props
	}: React.HTMLAttributes<HTMLDivElement> & { children: React.ReactNode }) => (
		<div {...props}>{children}</div>
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
		resumeStoragePolicyMigration: (...args: unknown[]) =>
			mockState.resumeStoragePolicyMigration(...args),
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
		mockState.resumeStoragePolicyMigration.mockReset();
		mockState.toastSuccess.mockReset();

		mockState.cleanupCompleted.mockResolvedValue({ removed: 2 });
		mockState.list.mockResolvedValue({
			items: [createTask()],
			total: 1,
		});
		mockState.resumeStoragePolicyMigration.mockResolvedValue(
			createTask({
				kind: "storage_policy_migration",
				status: "pending",
			}),
		);
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
		fireEvent.click(screen.getByRole("button", { name: /show_filters/ }));
		expect(screen.getByText("select:trash_purge_all")).toBeInTheDocument();
	});

	it("accepts storage policy migration filters from the url", async () => {
		mockState.list.mockResolvedValueOnce({
			items: [
				createTask({
					display_name: "Migrate blobs",
					kind: "storage_policy_migration",
					payload: {
						kind: "storage_policy_migration",
						source_policy_id: 1,
						target_policy_id: 2,
					} as never,
					progress_current: 12,
					progress_percent: 100,
					progress_total: 12,
					result: {
						kind: "storage_policy_migration",
						migrated_blobs: 12,
						skipped_blobs: 1,
						failed_blobs: 0,
						migrated_bytes: 4096,
						merged_blobs: 0,
						renamed_opaque_blobs: 2,
						scanned_blobs: 13,
						source_policy_id: 1,
						target_policy_id: 2,
					} as never,
					status: "succeeded",
					status_text: "Migration completed",
					steps: [
						{
							key: "prepare_sources",
							title: "Prepare storage policies",
							status: "succeeded",
							progress_current: 1,
							progress_total: 1,
						},
						{
							key: "finish",
							title: "Finish storage migration",
							status: "succeeded",
							progress_current: 1,
							progress_total: 1,
						},
					],
				}),
			],
			total: 1,
		});

		renderPage("/admin/tasks?kind=storage_policy_migration");

		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledWith({
				kind: "storage_policy_migration",
				limit: 20,
				offset: 0,
				sort_by: "updated_at",
				sort_order: "desc",
			});
		});
		expect(screen.getByText("Migrate blobs")).toBeInTheDocument();
		expect(
			screen.getAllByText("tasks:kind_storage_policy_migration").length,
		).toBeGreaterThan(0);
		fireEvent.click(screen.getByRole("button", { name: /show_filters/ }));
		expect(
			screen.getByText("select:storage_policy_migration"),
		).toBeInTheDocument();
		expect(screen.queryByText("Prepare source policy")).not.toBeInTheDocument();

		fireEvent.click(screen.getByText("Migrate blobs"));

		expect(
			await screen.findByText(/Prepare source policy/),
		).toBeInTheDocument();
		expect(screen.getByText(/Finish migration/)).toBeInTheDocument();
		expect(screen.getByText("Migrated objects")).toBeInTheDocument();
		expect(screen.getByText("Skipped objects")).toBeInTheDocument();
		expect(screen.getByText("Failed objects")).toBeInTheDocument();
		expect(screen.getByText("Bytes")).toBeInTheDocument();
		expect(screen.getByText("4096")).toBeInTheDocument();
	});

	it("resumes a failed storage migration from its detail dialog", async () => {
		mockState.list
			.mockResolvedValueOnce({
				items: [
					createTask({
						can_retry: true,
						display_name: "Resume migration",
						kind: "storage_policy_migration",
						last_error: "copy failed",
						payload: {
							kind: "storage_policy_migration",
							source_policy_id: 1,
							target_policy_id: 2,
						} as never,
						status: "failed",
						steps: [
							{
								key: "finish",
								title: "Finish storage migration",
								status: "failed",
								progress_current: 0,
								progress_total: 1,
							},
						],
					}),
				],
				total: 1,
			})
			.mockResolvedValueOnce({
				items: [
					createTask({
						display_name: "Resume migration",
						kind: "storage_policy_migration",
						status: "pending",
					}),
				],
				total: 1,
			});

		renderPage("/admin/tasks?kind=storage_policy_migration");

		fireEvent.click(await screen.findByText("Resume migration"));
		fireEvent.click(
			await screen.findByRole("button", {
				name: /admin:storage_migration_resume/,
			}),
		);

		await waitFor(() => {
			expect(mockState.resumeStoragePolicyMigration).toHaveBeenCalledWith(31);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("resume_queued");
		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledTimes(2);
		});
	});

	it("does not show resume for storage migrations that are not retryable", async () => {
		mockState.list.mockResolvedValueOnce({
			items: [
				createTask({
					can_retry: false,
					display_name: "Terminal migration",
					kind: "storage_policy_migration",
					last_error: "not retryable",
					status: "failed",
				}),
			],
			total: 1,
		});

		renderPage("/admin/tasks?kind=storage_policy_migration");

		fireEvent.click(await screen.findByText("Terminal migration"));

		expect(
			screen.queryByRole("button", {
				name: /admin:storage_migration_resume/,
			}),
		).not.toBeInTheDocument();
	});

	it("opens task details in a dialog from mouse and keyboard without rendering the old inline expander", async () => {
		mockState.list.mockResolvedValueOnce({
			items: [
				createTask({
					display_name: "Task with timeline",
					progress_current: 3,
					progress_total: 5,
					steps: [
						{
							key: "extract",
							title: "Extract archive",
							status: "active",
							progress_current: 3,
							progress_total: 5,
						},
					],
				}),
			],
			total: 1,
		});

		renderPage();

		const rowText = await screen.findByText("Task with timeline");
		expect(
			screen.queryByRole("button", { name: "tasks:show_details" }),
		).not.toBeInTheDocument();
		expect(screen.queryByText("#31")).not.toBeInTheDocument();
		expect(screen.queryByText(/Extract archive/)).not.toBeInTheDocument();

		fireEvent.click(rowText);

		expect(await screen.findByText("#31")).toBeInTheDocument();
		expect(screen.getByText(/Extract archive/)).toBeInTheDocument();
		expect(screen.getByText("3 / 5")).toBeInTheDocument();

		fireEvent.click(screen.getByText("X"));
		await waitFor(() => {
			expect(screen.queryByText("#31")).not.toBeInTheDocument();
		});

		fireEvent.keyDown(rowText.closest("div") as HTMLElement, { key: "Enter" });
		expect(await screen.findByText("#31")).toBeInTheDocument();
	});

	it("does not open a detail dialog for tasks without extra detail payload", async () => {
		mockState.list.mockResolvedValueOnce({
			items: [
				createTask({
					display_name: "Plain pending task",
					last_error: null,
					result: null,
					status: "pending",
					status_text: null,
					steps: [],
				}),
			],
			total: 1,
		});

		renderPage();

		const rowText = await screen.findByText("Plain pending task");
		fireEvent.click(rowText);
		fireEvent.keyDown(rowText.closest("div") as HTMLElement, { key: "Enter" });

		expect(screen.queryByText("#31")).not.toBeInTheDocument();
		expect(screen.queryByText("tasks:timeline_label")).not.toBeInTheDocument();
	});

	it("clears the selected task detail when pagination replaces the current row", async () => {
		mockState.list
			.mockResolvedValueOnce({
				items: [
					createTask({
						display_name: "First page failed task",
						last_error: "first page failure",
						status: "failed",
					}),
				],
				total: 25,
			})
			.mockResolvedValueOnce({
				items: [
					createTask({
						id: 77,
						display_name: "Second page failed task",
						last_error: "second page failure",
						status: "failed",
					}),
				],
				total: 25,
			});

		renderPage();

		fireEvent.click(await screen.findByText("First page failed task"));
		expect(await screen.findByText("#31")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "CaretRight" }));

		await waitFor(() => {
			expect(mockState.list).toHaveBeenNthCalledWith(2, {
				limit: 20,
				offset: 20,
				sort_by: "updated_at",
				sort_order: "desc",
			});
		});
		expect(
			await screen.findByText("Second page failed task"),
		).toBeInTheDocument();
		expect(screen.queryByText("#31")).not.toBeInTheDocument();
		expect(screen.queryByText("first page failure")).not.toBeInTheDocument();
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

	it("builds cleanup requests with kind and status filters and rejects invalid dates", async () => {
		renderPage();

		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledTimes(1);
		});

		fireEvent.click(
			screen.getByRole("button", { name: /task_cleanup_action/ }),
		);
		const finishedBefore = "2026-04-22T09:15";
		const finishedBeforeIso = new Date(finishedBefore).toISOString();
		fireEvent.change(
			screen.getByLabelText("admin:task_cleanup_finished_before"),
			{
				target: { value: finishedBefore },
			},
		);
		fireEvent.click(
			screen.getAllByRole("button", {
				name: "select-storage_policy_migration",
			})[0],
		);
		fireEvent.click(
			screen.getAllByRole("button", { name: "select-failed" })[0],
		);
		expect(
			screen.getByText(
				`cleanup-desc:time:${finishedBeforeIso}:tasks:kind_storage_policy_migration:tasks:status_failed`,
			),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: /cleanup_completed_tasks/ }),
		);

		await waitFor(() => {
			expect(mockState.cleanupCompleted).toHaveBeenCalledWith({
				finished_before: finishedBeforeIso,
				kind: "storage_policy_migration",
				status: "failed",
			});
		});

		fireEvent.click(
			screen.getByRole("button", { name: /task_cleanup_action/ }),
		);
		fireEvent.change(
			screen.getByLabelText("admin:task_cleanup_finished_before"),
			{
				target: { value: "not-a-date" },
			},
		);
		expect(
			screen.getByText("admin:task_cleanup_confirm_desc_invalid"),
		).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: /cleanup_completed_tasks/ }),
		).toBeDisabled();
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
