import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { cloneElement, isValidElement } from "react";
import { MemoryRouter, useLocation } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AdminUsersPage from "@/pages/admin/AdminUsersPage";
import type { UpdateUserRequest } from "@/types/api";

const mockState = vi.hoisted(() => ({
	create: vi.fn(),
	createInvitation: vi.fn(),
	deleteUser: vi.fn(),
	handleApiError: vi.fn(),
	list: vi.fn(),
	toastSuccess: vi.fn(),
	update: vi.fn(),
	writeTextToClipboard: vi.fn(),
}));

vi.mock("i18next", () => ({
	default: {
		t: (key: string) => key.replace(/^core:/, ""),
	},
}));

vi.mock("@/i18n", () => ({
	default: {
		language: "en",
		t: (key: string) => key.replace(/^core:/, ""),
	},
	ensureAllI18nNamespaces: vi.fn().mockResolvedValue(undefined),
	ensureI18nNamespaces: vi.fn().mockResolvedValue(undefined),
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

vi.mock("@/components/common/AnimatedCollapsible", () => ({
	AnimatedCollapsible: ({
		children,
		open,
	}: {
		children: React.ReactNode;
		open: boolean;
	}) => (open ? <div>{children}</div> : null),
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
	}) => {
		if (render && isValidElement(render)) {
			return cloneElement(render, undefined, children);
		}

		return <>{render ?? children}</>;
	},
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/lib/clipboard", () => ({
	writeTextToClipboard: (...args: unknown[]) =>
		mockState.writeTextToClipboard(...args),
}));

vi.mock("@/lib/format", () => ({
	formatBytes: (value: number) => `bytes:${value}`,
	formatDateAbsolute: (value: string) => `date:${value}`,
	formatDateAbsoluteWithOffset: (value: string) => `offset:${value}`,
}));

vi.mock("@/services/adminService", () => ({
	adminPolicyGroupService: {
		listAll: vi.fn().mockResolvedValue([]),
	},
	adminUserService: {
		create: (...args: unknown[]) => mockState.create(...args),
		createInvitation: (...args: unknown[]) =>
			mockState.createInvitation(...args),
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

function createInvitation(overrides: Record<string, unknown> = {}) {
	return {
		accepted_at: null,
		accepted_user_id: null,
		created_at: "2026-06-07T10:00:00Z",
		email: "invitee@example.com",
		expires_at: "2026-06-10T10:00:00Z",
		id: 101,
		invitation_url: "https://drive.example.test/invite/token",
		invited_by: 1,
		mail_queued: false,
		revoked_at: null,
		status: "pending",
		updated_at: "2026-06-07T10:00:00Z",
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

	return (
		<>
			<div data-testid="location-path">{location.pathname}</div>
			<div data-testid="location-search">{location.search}</div>
		</>
	);
}

describe("AdminUsersPage", () => {
	beforeEach(() => {
		mockState.create.mockReset();
		mockState.createInvitation.mockReset();
		mockState.deleteUser.mockReset();
		mockState.handleApiError.mockReset();
		mockState.list.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.update.mockReset();
		mockState.writeTextToClipboard.mockReset();

		mockState.create.mockResolvedValue({ user: createUser() });
		mockState.createInvitation.mockResolvedValue(createInvitation());
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
		mockState.writeTextToClipboard.mockResolvedValue(undefined);
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

		const refreshButtons = screen.getAllByRole("button", { name: /refresh/i });
		fireEvent.click(refreshButtons[0] as HTMLElement);

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

	it("links to the dedicated invitation management page without loading invitation records", async () => {
		renderPage();

		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledTimes(1);
		});

		fireEvent.click(
			screen.getByRole("button", { name: /invitation_records/i }),
		);

		expect(screen.getByTestId("location-path").textContent).toBe(
			"/admin/users/invitations",
		);
		expect(screen.queryByText("pending_invitations")).not.toBeInTheDocument();
	});

	it("invites a user, displays the generated link, copies it, and clears the result on edit", async () => {
		mockState.createInvitation.mockResolvedValueOnce(
			createInvitation({
				email: "invitee@example.com",
				invitation_url: "https://drive.example.test/invite/generated",
				mail_queued: true,
			}),
		);

		renderPage();

		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledTimes(1);
		});

		fireEvent.click(screen.getByRole("button", { name: /invite_user/i }));
		fireEvent.change(screen.getByLabelText("email"), {
			target: { value: "bad" },
		});
		fireEvent.click(screen.getByRole("button", { name: /send_invitation/i }));

		expect(mockState.createInvitation).not.toHaveBeenCalled();
		expect(screen.getByText("email_format")).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("email"), {
			target: { value: " invitee@example.com " },
		});
		fireEvent.click(screen.getByRole("button", { name: /send_invitation/i }));

		await waitFor(() => {
			expect(mockState.createInvitation).toHaveBeenCalledWith({
				email: "invitee@example.com",
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("invitation_created");
		expect(screen.getByText("invitation_mail_queued")).toBeInTheDocument();
		expect(
			screen.getByDisplayValue("https://drive.example.test/invite/generated"),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "invitation_copy_link" }),
		);

		await waitFor(() => {
			expect(mockState.writeTextToClipboard).toHaveBeenCalledWith(
				"https://drive.example.test/invite/generated",
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("copied_to_clipboard");

		fireEvent.change(screen.getByLabelText("email"), {
			target: { value: "someone@example.com" },
		});

		expect(
			screen.queryByText("invitation_mail_queued"),
		).not.toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "cancel" }));
		expect(screen.queryByLabelText("email")).not.toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: /invite_user/i }));
		expect(screen.getByLabelText("email")).toHaveValue("");
	});

	it("updates the URL when paging through users", async () => {
		mockState.list.mockResolvedValue({
			items: [createUser()],
			total: 45,
		});

		renderPage();

		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledWith({
				keyword: undefined,
				limit: 20,
				offset: 0,
				role: undefined,
				sort_by: "created_at",
				sort_order: "desc",
				status: undefined,
			});
		});

		fireEvent.click(screen.getByRole("button", { name: "CaretRight" }));

		await waitFor(() => {
			expect(screen.getByTestId("location-search").textContent).toBe(
				"?offset=20",
			);
		});
		expect(mockState.list).toHaveBeenCalledWith({
			keyword: undefined,
			limit: 20,
			offset: 20,
			role: undefined,
			sort_by: "created_at",
			sort_order: "desc",
			status: undefined,
		});

		fireEvent.click(screen.getByRole("button", { name: "CaretLeft" }));

		await waitFor(() => {
			expect(screen.getByTestId("location-search").textContent).toBe("");
		});
		expect(mockState.list).toHaveBeenCalledWith({
			keyword: undefined,
			limit: 20,
			offset: 0,
			role: undefined,
			sort_by: "created_at",
			sort_order: "desc",
			status: undefined,
		});
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
		expect(screen.getByLabelText("create_user_password")).toHaveAttribute(
			"autocomplete",
			"new-password",
		);

		fireEvent.change(screen.getByLabelText("username"), {
			target: { value: "abc" },
		});
		fireEvent.change(screen.getByLabelText("email"), {
			target: { value: "bad" },
		});
		fireEvent.change(screen.getByLabelText("create_user_password"), {
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
		fireEvent.change(screen.getByLabelText("create_user_password"), {
			target: { value: "secret12" },
		});
		fireEvent.click(
			screen.getByRole("switch", { name: "force_password_change" }),
		);

		fireEvent.click(screen.getByRole("button", { name: /create/i }));

		await waitFor(() => {
			expect(mockState.create).toHaveBeenCalledWith({
				email: "alice@example.com",
				must_change_password: true,
				password: "secret12",
				username: "alice1",
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("user_created");
		expect(mockState.list).toHaveBeenCalledTimes(2);
	});

	it("creates users with blank passwords and shows the generated password once", async () => {
		mockState.create.mockResolvedValueOnce({
			generated_password: "TempPass-123456789",
			user: createUser({
				email: "blank@example.com",
				must_change_password: true,
				username: "blankuser",
			}),
		});
		mockState.list
			.mockResolvedValueOnce({
				items: [],
				total: 0,
			})
			.mockResolvedValueOnce({
				items: [createUser({ username: "blankuser" })],
				total: 1,
			});

		renderPage();

		await screen.findByText("no_users");
		fireEvent.click(screen.getByRole("button", { name: /new_user/i }));
		fireEvent.change(screen.getByLabelText("username"), {
			target: { value: " blankuser " },
		});
		fireEvent.change(screen.getByLabelText("email"), {
			target: { value: " blank@example.com " },
		});

		fireEvent.click(screen.getByRole("button", { name: /create/i }));

		await waitFor(() => {
			expect(mockState.create).toHaveBeenCalledWith({
				email: "blank@example.com",
				must_change_password: false,
				password: undefined,
				username: "blankuser",
			});
		});
		expect(screen.getByText("generated_password_title")).toBeInTheDocument();
		expect(screen.getByDisplayValue("TempPass-123456789")).toBeInTheDocument();
		expect(screen.queryByLabelText("username")).not.toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "copy_generated_password" }),
		);

		await waitFor(() => {
			expect(mockState.writeTextToClipboard).toHaveBeenCalledWith(
				"TempPass-123456789",
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("copied_to_clipboard");

		fireEvent.click(screen.getByRole("button", { name: "close" }));
		expect(
			screen.queryByDisplayValue("TempPass-123456789"),
		).not.toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: /new_user/i }));
		expect(screen.getByLabelText("username")).toHaveValue("");
		expect(
			screen.queryByDisplayValue("TempPass-123456789"),
		).not.toBeInTheDocument();
	});

	it("trims create-user passwords before validation and submit", async () => {
		mockState.create.mockResolvedValueOnce({
			user: createUser({
				email: "trimmed@example.com",
				username: "trimmeduser",
			}),
		});

		renderPage();

		await screen.findByText("alice");
		fireEvent.click(screen.getByRole("button", { name: /new_user/i }));
		fireEvent.change(screen.getByLabelText("username"), {
			target: { value: "trimmeduser" },
		});
		fireEvent.change(screen.getByLabelText("email"), {
			target: { value: "trimmed@example.com" },
		});
		fireEvent.change(screen.getByLabelText("create_user_password"), {
			target: { value: "  secret12  " },
		});

		expect(screen.getByLabelText("create_user_password")).toHaveValue(
			"secret12",
		);

		fireEvent.click(screen.getByRole("button", { name: /create/i }));

		await waitFor(() => {
			expect(mockState.create).toHaveBeenCalledWith({
				email: "trimmed@example.com",
				must_change_password: false,
				password: "secret12",
				username: "trimmeduser",
			});
		});
		expect(screen.queryByText("password_min")).not.toBeInTheDocument();
	});

	it("treats whitespace-only create passwords as generated-password requests", async () => {
		mockState.create.mockResolvedValueOnce({
			generated_password: "WhitespacePass-123",
			user: createUser({
				email: "space@example.com",
				username: "spaceuser",
			}),
		});

		renderPage();

		await screen.findByText("alice");
		fireEvent.click(screen.getByRole("button", { name: /new_user/i }));
		fireEvent.change(screen.getByLabelText("username"), {
			target: { value: "spaceuser" },
		});
		fireEvent.change(screen.getByLabelText("email"), {
			target: { value: "space@example.com" },
		});
		fireEvent.change(screen.getByLabelText("create_user_password"), {
			target: { value: "   \t" },
		});
		fireEvent.click(screen.getByRole("button", { name: /create/i }));

		await waitFor(() => {
			expect(mockState.create).toHaveBeenCalledWith({
				email: "space@example.com",
				must_change_password: false,
				password: undefined,
				username: "spaceuser",
			});
		});
		expect(screen.queryByText("password_min")).not.toBeInTheDocument();
		expect(screen.getByDisplayValue("WhitespacePass-123")).toBeInTheDocument();
	});

	it("guards create-user submit against rapid duplicate submissions", async () => {
		let resolveCreate: ((value: unknown) => void) | undefined;
		mockState.create.mockReturnValueOnce(
			new Promise((resolve) => {
				resolveCreate = resolve;
			}),
		);

		renderPage();

		await screen.findByText("alice");
		fireEvent.click(screen.getByRole("button", { name: /new_user/i }));
		fireEvent.change(screen.getByLabelText("username"), {
			target: { value: "dupeuser" },
		});
		fireEvent.change(screen.getByLabelText("email"), {
			target: { value: "dupe@example.com" },
		});
		fireEvent.change(screen.getByLabelText("create_user_password"), {
			target: { value: "secret12" },
		});

		const form = screen.getByLabelText("username").closest("form");
		if (!form) throw new Error("create form missing");
		fireEvent.submit(form);
		fireEvent.submit(form);

		expect(mockState.create).toHaveBeenCalledTimes(1);
		resolveCreate?.({ user: createUser({ username: "dupeuser" }) });

		await waitFor(() => {
			expect(mockState.toastSuccess).toHaveBeenCalledWith("user_created");
		});
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
