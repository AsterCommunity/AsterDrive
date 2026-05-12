import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, useLocation } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AdminUsersPage from "@/pages/admin/AdminUsersPage";
import type { UpdateUserRequest } from "@/types/api";

const mockState = vi.hoisted(() => ({
	create: vi.fn(),
	deleteUser: vi.fn(),
	handleApiError: vi.fn(),
	list: vi.fn(),
	toastSuccess: vi.fn(),
	update: vi.fn(),
}));

vi.mock("i18next", () => ({
	default: {
		t: (key: string) => key.replace(/^core:/, ""),
	},
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) => {
			if (key === "entries_page") {
				return `entries:${options?.current}/${options?.pages}/${options?.total}`;
			}
			if (key === "page_size_option") {
				return `page-size:${options?.count}`;
			}
			return key.replace(/^core:/, "");
		},
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/lib/adminPolicyGroupLookup", () => ({
	loadAdminPolicyGroupLookup: vi.fn().mockResolvedValue([]),
}));

vi.mock("@/lib/idleTask", () => ({
	runWhenIdle: () => () => undefined,
}));

vi.mock("@/components/admin/UserDetailDialog", () => ({
	UserDetailDialog: ({
		onUpdate,
		open,
		user,
	}: {
		onUpdate: (id: number, data: UpdateUserRequest) => Promise<void>;
		open: boolean;
		user: { id: number; username: string } | null;
	}) =>
		open && user ? (
			<div>
				<div>{`detail:${user.username}`}</div>
				<button
					type="button"
					onClick={() => void onUpdate(user.id, { role: "admin" })}
				>
					detail-update
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/common/ConfirmDialog", () => ({
	ConfirmDialog: ({
		open,
		title,
		description,
		confirmLabel,
		onConfirm,
	}: {
		confirmLabel: string;
		description?: string;
		onConfirm: () => void;
		open: boolean;
		title: string;
	}) =>
		open ? (
			<div>
				<div>{title}</div>
				<div>{description}</div>
				<button type="button" onClick={onConfirm}>
					{confirmLabel}
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/common/EmptyState", () => ({
	EmptyState: ({
		action,
		description,
		icon,
		title,
	}: {
		action?: React.ReactNode;
		description?: string;
		icon?: React.ReactNode;
		title: string;
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

vi.mock("@/components/common/UserAvatarImage", () => ({
	UserAvatarImage: ({ name }: { name: string }) => (
		<div data-testid={`avatar:${name}`} aria-hidden="true" />
	),
}));

vi.mock("@/components/common/UserStatusBadge", () => ({
	getRoleBadgeClass: (role: string) => `role:${role}`,
	getStatusBadgeClass: (status: string) => `status:${status}`,
}));

vi.mock("@/components/layout/AdminLayout", () => ({
	AdminLayout: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/layout/AdminPageHeader", () => ({
	AdminPageHeader: ({
		actions,
		description,
		title,
		toolbar,
	}: {
		actions?: React.ReactNode;
		description: string;
		title: string;
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

vi.mock("@/components/ui/badge", () => ({
	Badge: ({ children }: { children: React.ReactNode }) => (
		<span>{children}</span>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		"aria-label": ariaLabel,
		children,
		className,
		disabled,
		onClick,
		title,
		type,
		variant,
	}: {
		"aria-label"?: string;
		children: React.ReactNode;
		className?: string;
		disabled?: boolean;
		onClick?: () => void;
		title?: string;
		type?: "button" | "submit";
		variant?: string;
	}) => (
		<button
			aria-label={ariaLabel}
			type={type ?? "button"}
			className={className}
			data-variant={variant}
			disabled={disabled}
			onClick={onClick}
			title={title}
		>
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
		<p>{children}</p>
	),
	DialogFooter: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({
		ariaInvalid,
		autoComplete,
		className,
		id,
		onChange,
		placeholder,
		required,
		type,
		value,
	}: {
		ariaInvalid?: boolean;
		autoComplete?: string;
		className?: string;
		id?: string;
		onChange?: (event: { target: { value: string } }) => void;
		placeholder?: string;
		required?: boolean;
		type?: string;
		value?: string;
	}) => (
		<input
			aria-invalid={ariaInvalid}
			autoComplete={autoComplete}
			className={className}
			id={id}
			onChange={(event) =>
				onChange?.({ target: { value: event.target.value } })
			}
			placeholder={placeholder}
			required={required}
			type={type}
			value={value}
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

vi.mock("@/components/ui/progress", () => ({
	Progress: ({ value }: { value: number }) => <div>{`progress:${value}`}</div>,
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

vi.mock("@/components/ui/select", () => {
	const { createContext, useContext } =
		require("react") as typeof import("react");

	const SelectContext = createContext<{
		disabled?: boolean;
		onValueChange?: (value: string) => void;
	}>({});

	return {
		Select: ({
			children,
			disabled,
			onValueChange,
		}: {
			children: React.ReactNode;
			disabled?: boolean;
			onValueChange?: (value: string) => void;
		}) => (
			<SelectContext.Provider value={{ disabled, onValueChange }}>
				<div>{children}</div>
			</SelectContext.Provider>
		),
		SelectContent: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
		SelectItem: ({
			children,
			value,
		}: {
			children: React.ReactNode;
			value: string;
		}) => {
			const context = useContext(SelectContext);

			return (
				<button
					type="button"
					aria-label={`select-item:${value}`}
					disabled={context.disabled}
					onClick={() => context.onValueChange?.(value)}
				>
					{children}
				</button>
			);
		},
		SelectTrigger: ({
			children,
			className,
		}: {
			children: React.ReactNode;
			className?: string;
		}) => <div className={className}>{children}</div>,
		SelectValue: () => <span>select-value</span>,
	};
});

vi.mock("@/components/ui/table", () => ({
	Table: ({ children }: { children: React.ReactNode }) => (
		<table>{children}</table>
	),
	TableBody: ({ children }: { children: React.ReactNode }) => (
		<tbody>{children}</tbody>
	),
	TableCell: ({
		children,
		className,
		onClick,
		onKeyDown,
	}: {
		children: React.ReactNode;
		className?: string;
		onClick?: (event: { stopPropagation?: () => void }) => void;
		onKeyDown?: (event: {
			key: string;
			preventDefault?: () => void;
			stopPropagation?: () => void;
		}) => void;
	}) => (
		<td
			data-slot="table-cell"
			className={className}
			onClick={onClick}
			onKeyDown={onKeyDown}
		>
			{children}
		</td>
	),
	TableHead: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => (
		<th data-slot="table-head" className={className}>
			{children}
		</th>
	),
	TableHeader: ({ children }: { children: React.ReactNode }) => (
		<thead data-slot="table-header">{children}</thead>
	),
	TableRow: ({
		children,
		className,
		onClick,
		onKeyDown,
		tabIndex,
	}: {
		children: React.ReactNode;
		className?: string;
		onClick?: () => void;
		onKeyDown?: (event: {
			key: string;
			preventDefault?: () => void;
			stopPropagation?: () => void;
		}) => void;
		tabIndex?: number;
	}) => (
		<tr
			data-slot="table-row"
			className={className}
			onClick={onClick}
			onKeyDown={onKeyDown}
			tabIndex={tabIndex}
		>
			{children}
		</tr>
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
		children?: React.ReactNode;
		render?: React.ReactNode;
	}) => render ?? children,
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/lib/format", () => ({
	formatBytes: (value: number) => `bytes:${value}`,
}));

vi.mock("@/services/adminService", () => ({
	adminPolicyGroupService: {
		listAll: vi.fn().mockResolvedValue([]),
	},
	adminUserService: {
		create: (...args: unknown[]) => mockState.create(...args),
		delete: (...args: unknown[]) => mockState.deleteUser(...args),
		list: (...args: unknown[]) => mockState.list(...args),
		update: (...args: unknown[]) => mockState.update(...args),
	},
}));

function createUser(overrides: Record<string, unknown> = {}) {
	return {
		created_at: "2026-03-28T00:00:00Z",
		email: "alice@example.com",
		id: 11,
		profile: {
			avatar: {
				source: "none",
				url_512: null,
				url_1024: null,
				version: 0,
			},
		},
		role: "user",
		status: "active",
		storage_quota: 10 * 1024 * 1024,
		storage_used: 5 * 1024 * 1024,
		updated_at: "2026-03-28T00:00:00Z",
		username: "alice",
		...overrides,
	};
}

function renderPage(initialEntry = "/admin/users") {
	return render(
		<MemoryRouter initialEntries={[initialEntry]}>
			<LocationProbe />
			<AdminUsersPage />
		</MemoryRouter>,
	);
}

function LocationProbe() {
	const location = useLocation();

	return <div data-testid="location-search">{location.search}</div>;
}

describe("AdminUsersPage", () => {
	beforeEach(() => {
		mockState.create.mockReset();
		mockState.deleteUser.mockReset();
		mockState.handleApiError.mockReset();
		mockState.list.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.update.mockReset();

		mockState.create.mockResolvedValue(createUser());
		mockState.deleteUser.mockResolvedValue(undefined);
		mockState.list.mockResolvedValue({
			items: [createUser()],
			total: 1,
		});
		mockState.update.mockImplementation(async (id, data) =>
			createUser({
				...(data as Record<string, unknown>),
				id,
			}),
		);
	});

	it("loads from search params, refreshes, opens the detail dialog, and updates the selected user", async () => {
		mockState.list
			.mockResolvedValueOnce({
				items: [createUser()],
				total: 21,
			})
			.mockResolvedValueOnce({
				items: [createUser()],
				total: 21,
			});

		renderPage(
			"/admin/users?keyword=alice&role=admin&status=disabled&offset=10&pageSize=10",
		);

		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledWith({
				keyword: "alice",
				limit: 10,
				offset: 10,
				role: "admin",
				sort_by: "created_at",
				sort_order: "desc",
				status: "disabled",
			});
		});

		expect(screen.getByText("entries:2/3/21")).toBeInTheDocument();
		expect(screen.getByText("alice")).toBeInTheDocument();
		expect(screen.getByTestId("avatar:alice")).toBeInTheDocument();
		const storageSummary = screen.getByText("bytes:5242880 / bytes:10485760");
		expect(storageSummary).toBeInTheDocument();
		fireEvent.click(storageSummary);

		expect(screen.getByText("detail:alice")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: /refresh/i }));

		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledTimes(2);
		});

		fireEvent.click(screen.getByRole("button", { name: "detail-update" }));

		await waitFor(() => {
			expect(mockState.update).toHaveBeenCalledWith(11, { role: "admin" });
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("user_updated");
	});

	it("clears keyword and select filters from the url in one update", async () => {
		renderPage("/admin/users?keyword=alice&role=admin&status=disabled");

		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledWith({
				keyword: "alice",
				limit: 20,
				offset: 0,
				role: "admin",
				sort_by: "created_at",
				sort_order: "desc",
				status: "disabled",
			});
		});

		fireEvent.click(screen.getByRole("button", { name: "clear_filters" }));

		expect(screen.getByTestId("location-search").textContent).toBe("");
	});

	it("validates the create form, trims inputs, creates the user, and reloads the list", async () => {
		mockState.list
			.mockResolvedValueOnce({
				items: [],
				total: 0,
			})
			.mockResolvedValueOnce({
				items: [createUser()],
				total: 1,
			});

		renderPage();

		expect(await screen.findByText("no_users")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: /new_user/i }));
		expect(screen.getByLabelText("username")).toHaveAttribute(
			"autocomplete",
			"off",
		);
		expect(screen.getByLabelText("email")).toHaveAttribute(
			"autocomplete",
			"off",
		);
		expect(screen.getByLabelText("password")).toHaveAttribute(
			"autocomplete",
			"new-password",
		);

		fireEvent.change(screen.getByLabelText("username"), {
			target: { value: "abc" },
		});
		fireEvent.change(screen.getByLabelText("email"), {
			target: { value: "bad" },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "1234567" },
		});

		fireEvent.click(screen.getByRole("button", { name: /create/i }));

		expect(mockState.create).not.toHaveBeenCalled();
		expect(screen.getByText("username_length")).toBeInTheDocument();
		expect(screen.getByText("email_format")).toBeInTheDocument();
		expect(screen.getByText("password_min")).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("username"), {
			target: { value: " alice1 " },
		});
		fireEvent.change(screen.getByLabelText("email"), {
			target: { value: " alice@example.com " },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "secret12" },
		});

		fireEvent.click(screen.getByRole("button", { name: /create/i }));

		await waitFor(() => {
			expect(mockState.create).toHaveBeenCalledWith({
				email: "alice@example.com",
				password: "secret12",
				username: "alice1",
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("user_created");
		expect(mockState.list).toHaveBeenCalledTimes(2);
	});

	it("deletes the last user on a page and rolls the offset back before reloading", async () => {
		mockState.list
			.mockResolvedValueOnce({
				items: [createUser({ id: 21, username: "page-two-user" })],
				total: 11,
			})
			.mockResolvedValueOnce({
				items: [createUser({ id: 5, username: "page-one-user" })],
				total: 10,
			});

		renderPage("/admin/users?offset=10&pageSize=10");

		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledWith({
				keyword: undefined,
				limit: 10,
				offset: 10,
				role: undefined,
				sort_by: "created_at",
				sort_order: "desc",
				status: undefined,
			});
		});

		fireEvent.click(screen.getByRole("button", { name: "delete_user" }));

		expect(screen.getByText("confirm_force_delete")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "delete" }));

		await waitFor(() => {
			expect(mockState.deleteUser).toHaveBeenCalledWith(21);
		});
		await waitFor(() => {
			expect(mockState.list).toHaveBeenLastCalledWith({
				keyword: undefined,
				limit: 10,
				offset: 0,
				role: undefined,
				sort_by: "created_at",
				sort_order: "desc",
				status: undefined,
			});
		});
		expect(await screen.findByText("page-one-user")).toBeInTheDocument();
		expect(mockState.toastSuccess).toHaveBeenCalledWith("user_deleted");
	});
});
