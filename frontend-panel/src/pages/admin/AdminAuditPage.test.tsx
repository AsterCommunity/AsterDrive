import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AdminAuditPage from "@/pages/admin/AdminAuditPage";
import type { AuditLogEntry, UserSummary } from "@/types/api";

const mockState = vi.hoisted(() => ({
	handleApiError: vi.fn(),
	list: vi.fn(),
}));

function createUserSummary(): UserSummary {
	return {
		id: 9,
		username: "root",
		profile: {
			display_name: "Root",
			avatar: {
				source: "none",
				url_1024: null,
				url_512: null,
				version: 0,
			},
		},
	};
}

vi.mock("react-i18next", async () => {
	const { createMockTWithInterpolation } = await import("@/test/i18n");
	const translateAudit = createMockTWithInterpolation({
		"admin:audit_action_file_delete": "Deleted file",
		"admin:audit_action_file_upload": "Uploaded file",
		"admin:audit_entity_type_file": "File",
		"admin:audit_entity_type_folder": "Folder",
		"admin:audit_presentation_config_value_updated":
			"Value changed to {{value}}",
		"admin:audit_presentation_file": "{{name}} · File",
		"admin:audit_presentation_file_upload": "Uploaded file",
	});

	return {
		useTranslation: () => ({
			t: (key: string, options?: Record<string, unknown>) => {
				if (key === "entries_page") {
					return `entries:${options?.current}/${options?.pages}/${options?.total}`;
				}
				return translateAudit(key, options);
			},
		}),
	};
});

vi.mock("@/components/common/EmptyState", () => ({
	EmptyState: ({ title, icon }: { title: string; icon?: React.ReactNode }) => (
		<div>
			<div>{title}</div>
			<div>{icon}</div>
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
		actions,
		title,
		description,
		toolbar,
	}: {
		actions?: React.ReactNode;
		title: string;
		description: string;
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
	AdminSurface: ({ children }: { children: React.ReactNode }) => (
		<div data-testid="admin-surface">{children}</div>
	),
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

vi.mock("@/components/ui/badge", () => ({
	Badge: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <span className={className}>{children}</span>,
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
		render: React.ReactElement;
	}) => (
		<button
			type="button"
			disabled={render.props.disabled}
			onClick={render.props.onClick}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({
		className,
		onChange,
		placeholder,
		value,
	}: {
		className?: string;
		onChange?: (event: { target: { value: string } }) => void;
		placeholder?: string;
		value?: string;
	}) => (
		<input
			className={className}
			onChange={(event) =>
				onChange?.({ target: { value: event.target.value } })
			}
			placeholder={placeholder}
			value={value}
		/>
	),
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
			<button type="button" onClick={() => onValueChange?.("__all__")}>
				select-all
			</button>
			<button type="button" onClick={() => onValueChange?.("file")}>
				select-file
			</button>
			<button type="button" onClick={() => onValueChange?.("folder")}>
				select-folder
			</button>
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

vi.mock("@/components/common/AdminTable", async () => {
	const actual = await vi.importActual<
		typeof import("@/components/common/AdminTable")
	>("@/components/common/AdminTable");

	return {
		...actual,
		AdminTable: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
		AdminTableBody: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
		AdminTableCell: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
		AdminTableHeader: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
		AdminTableRow: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
	};
});

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/lib/format", () => ({
	formatDateAbsolute: (value: string) => `date:${value}`,
	formatDateAbsoluteWithOffset: (value: string) => `date-with-offset:${value}`,
}));

vi.mock("@/services/auditService", () => ({
	auditService: {
		list: (...args: unknown[]) => mockState.list(...args),
	},
}));

function createEntry(overrides: Partial<AuditLogEntry> = {}): AuditLogEntry {
	return {
		action: "file_upload",
		created_at: "2026-03-28T00:00:00Z",
		details: null,
		entity_id: 1,
		entity_name: "report.pdf",
		entity_type: "file",
		id: 1,
		ip_address: "127.0.0.1",
		user_agent: "Vitest",
		user: createUserSummary(),
		...overrides,
	};
}

function renderPage(initialEntry = "/admin/audit") {
	render(
		<MemoryRouter initialEntries={[initialEntry]}>
			<AdminAuditPage />
		</MemoryRouter>,
	);
}

describe("AdminAuditPage", () => {
	beforeEach(() => {
		mockState.handleApiError.mockReset();
		mockState.list.mockReset();
		mockState.list.mockResolvedValue({
			items: [createEntry()],
			total: 1,
		});
	});

	it("shows a loading skeleton while the audit request is pending", () => {
		mockState.list.mockImplementationOnce(() => new Promise(() => undefined));

		renderPage();

		expect(screen.getByText("skeleton:6:6")).toBeInTheDocument();
	});

	it("renders the empty state when there are no audit entries", async () => {
		mockState.list.mockResolvedValueOnce({
			items: [],
			total: 0,
		});

		renderPage();

		expect(await screen.findByText("no_audit_logs")).toBeInTheDocument();
		expect(screen.getByText("Scroll")).toBeInTheDocument();
	});

	it("renders entries, paginates, and refetches when filters change", async () => {
		mockState.list
			.mockResolvedValueOnce({
				items: [createEntry()],
				total: 21,
			})
			.mockResolvedValueOnce({
				items: [createEntry({ id: 2, entity_name: null, ip_address: null })],
				total: 21,
			})
			.mockResolvedValueOnce({
				items: [createEntry({ id: 3, action: "file_delete" })],
				total: 1,
			})
			.mockResolvedValueOnce({
				items: [createEntry({ id: 4, entity_type: "folder" })],
				total: 1,
			});

		renderPage();

		await waitFor(() => {
			expect(mockState.list).toHaveBeenNthCalledWith(1, {
				action: undefined,
				entity_type: undefined,
				limit: 20,
				offset: 0,
				sort_by: "created_at",
				sort_order: "desc",
			});
		});
		expect(await screen.findByText("report.pdf")).toBeInTheDocument();
		expect(screen.getByText("date:2026-03-28T00:00:00Z")).toBeInTheDocument();
		expect(screen.getByText("entries:1/2/21")).toBeInTheDocument();
		expect(screen.getByText("Uploaded file")).toHaveClass("border-emerald-200");

		fireEvent.click(screen.getByRole("button", { name: "CaretRight" }));

		await waitFor(() => {
			expect(mockState.list).toHaveBeenNthCalledWith(2, {
				action: undefined,
				entity_type: undefined,
				limit: 20,
				offset: 20,
				sort_by: "created_at",
				sort_order: "desc",
			});
		});
		await waitFor(() => {
			expect(screen.getByText("---")).toBeInTheDocument();
		});
		expect(screen.queryByText("report.pdf")).toBeNull();

		fireEvent.click(screen.getByRole("button", { name: /show_filters/ }));
		fireEvent.change(screen.getByPlaceholderText("audit_filter_action"), {
			target: { value: "file_delete" },
		});

		await waitFor(() => {
			expect(mockState.list).toHaveBeenNthCalledWith(3, {
				action: "file_delete",
				entity_type: undefined,
				limit: 20,
				offset: 0,
				sort_by: "created_at",
				sort_order: "desc",
			});
		});
		expect(await screen.findByText("Deleted file")).toBeInTheDocument();
		expect(screen.getByText("Deleted file")).toHaveClass("border-red-200");

		fireEvent.click(
			screen.getAllByRole("button", { name: "select-folder" })[0],
		);

		await waitFor(() => {
			expect(mockState.list).toHaveBeenNthCalledWith(4, {
				action: "file_delete",
				entity_type: "folder",
				limit: 20,
				offset: 0,
				sort_by: "created_at",
				sort_order: "desc",
			});
		});
		await waitFor(() => {
			expect(screen.getAllByText("Folder").length).toBeGreaterThan(0);
		});
	});

	it("renders structured audit presentation detail when the backend provides it", async () => {
		mockState.list.mockResolvedValueOnce({
			items: [
				createEntry({
					entity_name: "legacy.txt",
					entity_type: "folder",
					presentation: {
						detail: {
							code: "config_value_updated",
							params: { value: "enabled" },
						},
						summary: { code: "file_upload" },
						target: {
							code: "file",
							params: { name: "report.pdf" },
						},
					},
				}),
			],
			total: 1,
		});

		renderPage();

		expect(await screen.findByText("Uploaded file")).toBeInTheDocument();
		expect(screen.getByText("report.pdf · File")).toBeInTheDocument();
		expect(screen.getAllByText("File").length).toBeGreaterThan(0);
		expect(screen.getByText("Value changed to enabled")).toBeInTheDocument();
		expect(screen.queryByText("legacy.txt")).not.toBeInTheDocument();
	});

	it("renders audit entry without detail when presentation detail is missing", async () => {
		mockState.list.mockResolvedValueOnce({
			items: [
				createEntry({
					entity_name: "legacy.txt",
					entity_type: "folder",
					presentation: {
						summary: { code: "file_upload" },
						target: {
							code: "file",
							params: { name: "report.pdf" },
						},
					},
				}),
			],
			total: 1,
		});

		renderPage();

		expect(await screen.findByText("Uploaded file")).toBeInTheDocument();
		expect(screen.getByText("report.pdf · File")).toBeInTheDocument();
		expect(screen.queryByText(/Value changed/)).not.toBeInTheDocument();
	});

	it("renders filtered empty state and can clear filters", async () => {
		mockState.list
			.mockResolvedValueOnce({
				items: [],
				total: 0,
			})
			.mockResolvedValueOnce({
				items: [createEntry()],
				total: 1,
			});

		renderPage("/admin/audit?action=file_delete");

		await waitFor(() => {
			expect(mockState.list).toHaveBeenNthCalledWith(1, {
				action: "file_delete",
				entity_type: undefined,
				limit: 20,
				offset: 0,
				sort_by: "created_at",
				sort_order: "desc",
			});
		});
		expect(
			await screen.findByText("no_filtered_audit_logs"),
		).toBeInTheDocument();
		expect(screen.getByText("filters_active")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "clear_filters" }));

		await waitFor(() => {
			expect(mockState.list).toHaveBeenNthCalledWith(2, {
				action: undefined,
				entity_type: undefined,
				limit: 20,
				offset: 0,
				sort_by: "created_at",
				sort_order: "desc",
			});
		});
	});

	it("updates page size, sorting, and reloads the audit table", async () => {
		mockState.list.mockResolvedValue({
			items: [createEntry()],
			total: 51,
		});

		renderPage();

		await waitFor(() => {
			expect(mockState.list).toHaveBeenNthCalledWith(1, {
				action: undefined,
				entity_type: undefined,
				limit: 20,
				offset: 0,
				sort_by: "created_at",
				sort_order: "desc",
			});
		});

		fireEvent.click(
			(await screen.findAllByRole("button", { name: "select-50" }))[0],
		);

		await waitFor(() => {
			expect(mockState.list).toHaveBeenLastCalledWith({
				action: undefined,
				entity_type: undefined,
				limit: 50,
				offset: 0,
				sort_by: "created_at",
				sort_order: "desc",
			});
		});

		fireEvent.click(screen.getByRole("button", { name: /audit_action/i }));

		await waitFor(() => {
			expect(mockState.list).toHaveBeenLastCalledWith({
				action: undefined,
				entity_type: undefined,
				limit: 50,
				offset: 0,
				sort_by: "action",
				sort_order: "asc",
			});
		});

		const callsBeforeReload = mockState.list.mock.calls.length;
		fireEvent.click(screen.getByRole("button", { name: /core:refresh/i }));

		await waitFor(() => {
			expect(mockState.list.mock.calls.length).toBeGreaterThan(
				callsBeforeReload,
			);
		});
		expect(mockState.list).toHaveBeenLastCalledWith({
			action: undefined,
			entity_type: undefined,
			limit: 50,
			offset: 0,
			sort_by: "action",
			sort_order: "asc",
		});
	});

	it("routes loading failures through handleApiError", async () => {
		const error = new Error("audit failed");
		mockState.list.mockRejectedValueOnce(error);

		renderPage();

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
	});
});
