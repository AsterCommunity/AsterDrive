import {
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AdminOverviewPage from "@/pages/admin/AdminOverviewPage";

const mockState = vi.hoisted(() => ({
	displayTimeZoneStore: {
		preference: "America/Los_Angeles",
	},
	get: vi.fn(),
	handleApiError: vi.fn(),
	navigate: vi.fn(),
}));

function createUserSummary(id = 9, username = "root", displayName = "Root") {
	return {
		id,
		username,
		profile: {
			display_name: displayName,
			avatar: {
				source: "none" as const,
				url_1024: null,
				url_512: null,
				version: 0,
			},
		},
	};
}

vi.mock("react-i18next", async () => {
	const { createMockTWithInterpolation } = await import("@/test/i18n");
	const translateOverview = createMockTWithInterpolation({
		"admin:audit_action_share_create": "Created share",
		"admin:audit_entity_type_file": "File",
		"admin:audit_presentation_share_updated":
			"Password {{has_password}}, max downloads {{max_downloads}}",
		"admin:overview_system_health_component_cache": "Cache",
		"admin:overview_system_health_component_remote_nodes": "Remote nodes",
		"admin:overview_system_health_issue_component": "{{component}}: {{status}}",
		"admin:overview_system_health_issue_summary":
			"Issues detected in: {{components}}.",
		"admin:overview_system_health_status_degraded": "Degraded",
		"admin:overview_system_health_status_unhealthy": "Unhealthy",
	});

	return {
		useTranslation: () => ({
			t: (key: string, options?: Record<string, unknown>) => {
				if (key === "overview_generated_at") {
					return `generated:${options?.date}`;
				}
				if (key === "overview_background_tasks_duration") {
					return `duration:${options?.duration}`;
				}
				if (key === "overview_background_tasks_source_system") {
					return "source:system";
				}
				if (key === "overview_background_tasks_source_user") {
					return `source:user:${options?.id}`;
				}
				if (key === "overview_background_tasks_source_team") {
					return `source:team:${options?.id}`;
				}
				if (key === "overview_today_new_users_badge") {
					return `new-users:${options?.count}`;
				}
				if (key === "overview_today_uploads_badge") {
					return `uploads:${options?.count}`;
				}
				if (key === "overview_today_shares_badge") {
					return `shares:${options?.count}`;
				}
				return translateOverview(key, options);
			},
		}),
	};
});

vi.mock("react-router-dom", () => ({
	useNavigate: () => mockState.navigate,
}));

vi.mock("@/components/common/EmptyState", () => ({
	EmptyState: ({
		title,
		description,
		action,
	}: {
		title: string;
		description?: string;
		action?: React.ReactNode;
	}) => (
		<div>
			<div>{title}</div>
			<div>{description}</div>
			<div>{action}</div>
		</div>
	),
}));

vi.mock("@/components/common/SkeletonTable", () => ({
	SkeletonTable: ({ columns, rows }: { columns: number; rows: number }) => (
		<div>{`skeleton-table:${columns}:${rows}`}</div>
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
		toolbar,
		actions,
	}: {
		title: string;
		description?: string;
		toolbar?: React.ReactNode;
		actions?: React.ReactNode;
	}) => (
		<div>
			<h1>{title}</h1>
			<p>{description}</p>
			<div data-testid="admin-page-header-toolbar">{toolbar}</div>
			<div data-testid="admin-page-header-actions">{actions}</div>
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
	}) => (
		<section data-testid="admin-surface" className={className}>
			{children}
		</section>
	),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <span className={className}>{children}</span>,
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
	}) => (
		<button type="button" disabled={disabled} onClick={onClick}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/card", () => ({
	Card: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
	CardContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	CardDescription: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	CardHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	CardTitle: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("recharts", () => ({
	CartesianGrid: () => <div>recharts-grid</div>,
	Line: ({ dataKey, name }: { dataKey?: string; name?: string }) => (
		<div>{`recharts-line:${dataKey ?? ""}:${name ?? ""}`}</div>
	),
	LineChart: ({
		children,
		data,
	}: {
		children: React.ReactNode;
		data?: Array<{ date: string }>;
	}) => (
		<div>
			<div>{`recharts-line-chart:${data?.map((point) => point.date).join(",") ?? ""}`}</div>
			{children}
		</div>
	),
	ResponsiveContainer: ({
		children,
		debounce,
	}: {
		children: React.ReactNode;
		debounce?: number;
	}) => (
		<div>
			<div>{`recharts-responsive-container-debounce:${String(debounce ?? "")}`}</div>
			{children}
		</div>
	),
	Tooltip: () => <div>recharts-tooltip</div>,
	XAxis: ({
		dataKey,
		interval,
		minTickGap,
		padding,
	}: {
		dataKey?: string;
		interval?: number | string;
		minTickGap?: number;
		padding?: { left?: number; right?: number };
	}) => (
		<div>
			{`recharts-x-axis:${dataKey ?? ""}:${String(interval ?? "")}:${String(minTickGap ?? "")}:${String(padding?.left ?? "")}:${String(padding?.right ?? "")}`}
		</div>
	),
	YAxis: () => <div>recharts-y-axis</div>,
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/scroll-area", () => ({
	ScrollArea: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/ui/skeleton", () => ({
	Skeleton: ({ className }: { className?: string }) => (
		<div>{`skeleton:${className ?? ""}`}</div>
	),
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

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/lib/format", () => ({
	formatBytes: (value: number) => `bytes:${value}`,
	formatDateAbsolute: (value: string) => `date:${value}`,
	formatDateAbsoluteWithOffset: (value: string) => `date-with-offset:${value}`,
}));

vi.mock("@/services/adminService", () => ({
	adminOverviewService: {
		get: (...args: unknown[]) => mockState.get(...args),
	},
}));

vi.mock("@/stores/displayTimeZoneStore", () => ({
	resolveActiveDisplayTimeZone: (preference: string) =>
		preference === "browser" ? "UTC" : preference,
	useDisplayTimeZoneStore: (
		selector: (state: typeof mockState.displayTimeZoneStore) => unknown,
	) => selector(mockState.displayTimeZoneStore),
}));

function createOverview() {
	return {
		days: 7,
		daily_reports: [
			{
				date: "2026-03-29",
				deletions: 1,
				new_users: 2,
				share_creations: 3,
				sign_ins: 4,
				total_events: 10,
				uploads: 5,
			},
			{
				date: "2026-03-28",
				deletions: 2,
				new_users: 1,
				share_creations: 1,
				sign_ins: 3,
				total_events: 9,
				uploads: 2,
			},
			{
				date: "2026-03-27",
				deletions: 0,
				new_users: 0,
				share_creations: 0,
				sign_ins: 1,
				total_events: 4,
				uploads: 1,
			},
		],
		generated_at: "2026-03-29T10:00:00Z",
		recent_events: [
			{
				action: "share_create",
				created_at: "2026-03-29T09:30:00Z",
				entity_id: 7,
				entity_name: "report.pdf",
				entity_type: "file",
				id: 1,
				ip_address: "127.0.0.1",
				presentation: {
					detail: {
						code: "share_updated",
						params: { has_password: true, max_downloads: 5 },
					},
					summary: { code: "share_create" },
					target: {
						code: "file",
						params: { name: "report.pdf" },
					},
				},
				user_agent: "Vitest",
				user: createUserSummary(),
			},
		],
		recent_background_tasks: [
			{
				created_at: "2026-03-29T09:20:00Z",
				creator: null,
				display_name: "Trash cleanup",
				duration_ms: 1250,
				finished_at: "2026-03-29T09:21:00Z",
				id: 17,
				kind: "system_runtime",
				last_error: null,
				started_at: "2026-03-29T09:20:58Z",
				status: "succeeded",
				status_text: "cleaned up 2 expired trash entries",
				team_id: null,
				updated_at: "2026-03-29T09:21:00Z",
			},
		],
		stats: {
			active_users: 11,
			audit_events_today: 12,
			disabled_users: 1,
			new_users_today: 2,
			total_blobs: 99,
			total_blob_bytes: 2048,
			total_files: 120,
			total_file_bytes: 4096,
			total_shares: 6,
			total_users: 12,
			shares_today: 3,
			uploads_today: 5,
		},
		system_health: {
			checked_at: "2026-03-29T09:22:00Z",
			components: [
				{
					message: "database ping succeeded",
					name: "database",
					status: "healthy",
				},
				{
					message:
						"configured cache backend 'redis' is using active backend 'memory'",
					name: "cache",
					status: "degraded",
				},
				{
					message: "checked 1 remote node: 1 healthy, 0 failed, 0 skipped",
					name: "remote_nodes",
					status: "healthy",
				},
			],
			details:
				"cache=degraded: configured cache backend 'redis' is using active backend 'memory'",
			status: "degraded",
			summary: "cache degraded",
			task_id: 18,
		},
		timezone: "UTC",
	};
}

describe("AdminOverviewPage", () => {
	beforeEach(() => {
		mockState.displayTimeZoneStore.preference = "America/Los_Angeles";
		mockState.get.mockReset();
		mockState.handleApiError.mockReset();
		mockState.navigate.mockReset();
		mockState.get.mockResolvedValue(createOverview());
	});

	it("shows loading placeholders while the overview request is pending", () => {
		mockState.get.mockImplementationOnce(() => new Promise(() => undefined));

		render(<AdminOverviewPage />);

		expect(screen.getAllByText(/skeleton:/i).length).toBeGreaterThan(0);
		expect(screen.getByText("skeleton-table:7:7")).toBeInTheDocument();
		expect(screen.getByText("skeleton-table:4:8")).toBeInTheDocument();
		expect(screen.getByText("skeleton-table:5:6")).toBeInTheDocument();
	});

	it("renders overview stats and refreshes on demand", async () => {
		render(<AdminOverviewPage />);

		await waitFor(() => {
			expect(mockState.get).toHaveBeenCalledTimes(1);
		});
		expect(mockState.get).toHaveBeenCalledWith({
			days: 7,
			timezone: "America/Los_Angeles",
			event_limit: 10,
		});

		expect(screen.getByText("overview")).toBeInTheDocument();
		expect(screen.queryByText("overview_daily_trend")).not.toBeInTheDocument();
		expect(screen.getByText("overview_summary")).toBeInTheDocument();
		expect(screen.getByText("overview_total_users")).toBeInTheDocument();
		expect(screen.getByText("120")).toBeInTheDocument();
		expect(screen.getByText("99")).toBeInTheDocument();
		expect(screen.getByText("bytes:4096")).toBeInTheDocument();
		expect(screen.getByText("bytes:2048")).toBeInTheDocument();
		expect(screen.getByText("Created share")).toBeInTheDocument();
		expect(screen.getByText("report.pdf · File")).toBeInTheDocument();
		expect(
			screen.getByText("Password true, max downloads 5"),
		).toBeInTheDocument();
		expect(screen.getAllByText("File").length).toBeGreaterThan(0);
		expect(screen.getByText("Trash cleanup")).toBeInTheDocument();
		expect(
			screen.getByText("cleaned up 2 expired trash entries"),
		).toBeInTheDocument();
		expect(screen.getByText("source:system")).toBeInTheDocument();
		expect(screen.getByText("tasks:status_succeeded")).toBeInTheDocument();
		expect(screen.getByText("new-users:2")).toBeInTheDocument();
		expect(screen.getByText("uploads:5")).toBeInTheDocument();
		expect(screen.getByText("shares:3")).toBeInTheDocument();
		expect(
			screen.getByText("overview_system_health_degraded"),
		).toBeInTheDocument();
		expect(screen.getByText("Issues detected in: Cache.")).toBeInTheDocument();
		expect(screen.getByText("Cache: Degraded")).toBeInTheDocument();
		expect(screen.queryByText("database: healthy")).not.toBeInTheDocument();
		expect(screen.queryByText(/database healthy/)).not.toBeInTheDocument();
		expect(
			screen.getByText("overview_system_health_view_history"),
		).toBeInTheDocument();
		expect(
			screen.getByText("generated:date:2026-03-29T10:00:00Z"),
		).toBeInTheDocument();
		expect(
			screen.getByTestId("admin-page-header-toolbar"),
		).toBeEmptyDOMElement();
		expect(screen.queryByText(/^select:/)).not.toBeInTheDocument();
		expect(
			within(screen.getByTestId("admin-page-header-actions")).getByRole(
				"button",
				{ name: /refresh/i },
			),
		).toBeInTheDocument();
		expect(
			screen.queryByText(
				"recharts-line:totalEvents:overview_report_total_events",
			),
		).not.toBeInTheDocument();
		expect(
			await screen.findByText("recharts-line:uploads:overview_report_uploads"),
		).toBeInTheDocument();
		expect(
			screen.getByText("recharts-line:shareCreations:overview_report_shares"),
		).toBeInTheDocument();
		expect(
			screen.getByText("recharts-line:newUsers:overview_report_new_users"),
		).toBeInTheDocument();
		expect(
			screen.getByText("recharts-x-axis:label:0:0:12:12"),
		).toBeInTheDocument();
		expect(
			screen.getByText("recharts-responsive-container-debounce:120"),
		).toBeInTheDocument();
		expect(
			screen.getByText("recharts-line-chart:2026-03-27,2026-03-28,2026-03-29"),
		).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: /refresh/i }));

		await waitFor(() => {
			expect(mockState.get).toHaveBeenCalledTimes(2);
		});
	});

	it("links the system health banner to runtime task history", async () => {
		render(<AdminOverviewPage />);

		const historyButton = await screen.findByRole("button", {
			name: /overview_system_health_view_history/i,
		});

		fireEvent.click(historyButton);

		expect(mockState.navigate).toHaveBeenCalledWith(
			"/admin/tasks?kind=system_runtime",
		);
	});

	it("localizes unhealthy remote node health components", async () => {
		const overview = createOverview();
		overview.system_health = {
			checked_at: "2026-03-29T09:22:00Z",
			components: [
				{
					message: "checked 1 remote node: 0 healthy, 1 failed, 0 skipped",
					name: "remote_nodes",
					status: "unhealthy",
				},
			],
			details:
				"remote_nodes=unhealthy: checked 1 remote node: 0 healthy, 1 failed, 0 skipped",
			status: "unhealthy",
			summary: "remote_nodes unhealthy",
			task_id: 18,
		};
		mockState.get.mockResolvedValueOnce(overview);

		render(<AdminOverviewPage />);

		expect(
			await screen.findByText("overview_system_health_unhealthy"),
		).toBeInTheDocument();
		expect(
			screen.getByText("Issues detected in: Remote nodes."),
		).toBeInTheDocument();
		expect(screen.getByText("Remote nodes: Unhealthy")).toBeInTheDocument();
		expect(
			screen.queryByText("remote_nodes unhealthy"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByText("remote_nodes: unhealthy"),
		).not.toBeInTheDocument();
	});

	it("labels trash purge background tasks in the overview", async () => {
		const overview = createOverview();
		overview.recent_background_tasks = [
			{
				created_at: "2026-03-29T09:20:00Z",
				creator: null,
				display_name: "Empty trash",
				duration_ms: null,
				finished_at: "2026-03-29T09:21:00Z",
				id: 19,
				kind: "trash_purge_all",
				last_error: null,
				started_at: "2026-03-29T09:20:58Z",
				status: "succeeded",
				status_text: "purged 3 items",
				team_id: null,
				updated_at: "2026-03-29T09:21:00Z",
			},
		];
		mockState.get.mockResolvedValueOnce(overview);

		render(<AdminOverviewPage />);

		expect(await screen.findByText("Empty trash")).toBeInTheDocument();
		expect(screen.getByText("tasks:kind_trash_purge_all")).toBeInTheDocument();
		expect(screen.getByText("purged 3 items")).toBeInTheDocument();
		expect(screen.queryByText(/^duration:/)).not.toBeInTheDocument();
	});

	it("shows background task team and creator sources and prefers errors over status text", async () => {
		const overview = createOverview();
		overview.recent_background_tasks = [
			{
				created_at: "2026-03-29T09:20:00Z",
				creator: createUserSummary(12, "builder", "Build User"),
				display_name: "Compress export.zip",
				duration_ms: 500,
				finished_at: "2026-03-29T09:21:00Z",
				id: 20,
				kind: "archive_compress",
				last_error: "archive failed",
				started_at: "2026-03-29T09:20:58Z",
				status: "failed",
				status_text: "archive succeeded",
				team_id: null,
				updated_at: "2026-03-29T09:21:00Z",
			},
			{
				created_at: "2026-03-29T09:22:00Z",
				creator: createUserSummary(13, "ignored", "Ignored User"),
				display_name: "Extract team.zip",
				duration_ms: 1000,
				finished_at: null,
				id: 21,
				kind: "archive_extract",
				last_error: null,
				started_at: "2026-03-29T09:22:10Z",
				status: "processing",
				status_text: null,
				team_id: 44,
				updated_at: "2026-03-29T09:22:10Z",
			},
		];
		mockState.get.mockResolvedValueOnce(overview);

		render(<AdminOverviewPage />);

		expect(await screen.findByText("Compress export.zip")).toBeInTheDocument();
		expect(screen.getByText("archive failed")).toBeInTheDocument();
		expect(screen.queryByText("archive succeeded")).not.toBeInTheDocument();
		expect(screen.getByText("Build User")).toBeInTheDocument();
		expect(screen.getByText("Extract team.zip")).toBeInTheDocument();
		expect(screen.getByText("source:team:44")).toBeInTheDocument();
		expect(screen.queryByText("Ignored User")).not.toBeInTheDocument();
	});

	it("hides noisy component summaries when system health is healthy", async () => {
		const overview = createOverview();
		overview.system_health = {
			checked_at: "2026-03-29T09:22:00Z",
			components: [
				{
					message: "database ping succeeded",
					name: "database",
					status: "healthy",
				},
				{
					message: "cache backend succeeded",
					name: "cache",
					status: "healthy",
				},
				{
					message: "checked 2 remote nodes",
					name: "remote_nodes",
					status: "healthy",
				},
			],
			details:
				"database=healthy: database ping succeeded; cache=healthy: cache backend succeeded; remote_nodes=healthy: checked 2 remote nodes",
			status: "healthy",
			summary: "database healthy, cache healthy, remote_nodes healthy",
			task_id: 18,
		};
		mockState.get.mockResolvedValueOnce(overview);

		render(<AdminOverviewPage />);

		expect(
			await screen.findByText("overview_system_health_healthy"),
		).toBeInTheDocument();
		expect(
			screen.getByText("overview_system_health_healthy_desc"),
		).toBeInTheDocument();
		expect(screen.queryByText(/database healthy/)).not.toBeInTheDocument();
		expect(screen.queryByText(/database=healthy/)).not.toBeInTheDocument();
	});

	it("renders unknown health without checked time or history action", async () => {
		const overview = createOverview();
		overview.system_health = {
			checked_at: null,
			components: [],
			details: null,
			status: "unknown",
			summary: null,
			task_id: null,
		};
		mockState.get.mockResolvedValueOnce(overview);

		render(<AdminOverviewPage />);

		expect(
			await screen.findByText("overview_system_health_unknown"),
		).toBeInTheDocument();
		expect(
			screen.getByText("overview_system_health_unknown_desc"),
		).toBeInTheDocument();
		expect(
			screen.getByText("overview_system_health_not_checked"),
		).toBeInTheDocument();
		expect(
			screen.queryByText("overview_system_health_view_history"),
		).not.toBeInTheDocument();
	});

	it("uses issue details when an unhealthy summary has no component failures", async () => {
		const overview = createOverview();
		overview.system_health = {
			checked_at: null,
			components: [],
			details: "runtime probe failed before component checks",
			status: "unhealthy",
			summary: null,
			task_id: null,
		};
		mockState.get.mockResolvedValueOnce(overview);

		render(<AdminOverviewPage />);

		expect(
			await screen.findByText("overview_system_health_unhealthy"),
		).toBeInTheDocument();
		expect(
			screen.getByText("overview_system_health_no_summary"),
		).toBeInTheDocument();
		expect(
			screen.getByText("runtime probe failed before component checks"),
		).toBeInTheDocument();
	});

	it("keeps the daily reports block naturally expanded", async () => {
		render(<AdminOverviewPage />);

		const dailyReportsHeading = await screen.findByText(
			"overview_daily_reports",
		);
		const dailyReportsSurface = dailyReportsHeading.closest(
			'[data-testid="admin-surface"]',
		);

		expect(dailyReportsSurface).toHaveClass("flex-none");
		expect(dailyReportsSurface).not.toHaveClass("flex-1");
	});
});
