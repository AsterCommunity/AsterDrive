import {
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { UserDetailDialog } from "@/components/admin/UserDetailDialog";

const mockState = vi.hoisted(() => ({
	handleApiError: vi.fn(),
	listPolicies: vi.fn(),
	onUpdate: vi.fn(),
	resetMfa: vi.fn(),
	revokeSessions: vi.fn(),
	resetPassword: vi.fn(),
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key.replace(/^core:/, ""),
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		error: (...args: unknown[]) => mockState.toastError(...args),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/common/SkeletonTable", () => ({
	SkeletonTable: ({ columns, rows }: { columns: number; rows: number }) => (
		<div>{`skeleton:${columns}:${rows}`}</div>
	),
}));

vi.mock("@/components/common/UserAvatarImage", () => ({
	UserAvatarImage: ({ name }: { name: string }) => (
		<div>{`avatar:${name}`}</div>
	),
}));

vi.mock("@/components/common/UserStatusBadge", () => ({
	getRoleBadgeClass: (role: string) => `role:${role}`,
	getStatusBadgeClass: (status: string) => `status:${status}`,
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
		variant,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		type?: "button" | "submit";
		variant?: string;
	}) => (
		<button
			type={type ?? "button"}
			data-variant={variant}
			disabled={disabled}
			onClick={onClick}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({ children, open }: { children: React.ReactNode; open: boolean }) =>
		open ? <div>{children}</div> : null,
	DialogContent: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
	DialogFooter: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
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
		autoComplete,
		className,
		disabled,
		id,
		onChange,
		placeholder,
		readOnly,
		type,
		value,
	}: {
		autoComplete?: string;
		className?: string;
		disabled?: boolean;
		id?: string;
		onChange?: (event: { target: { value: string } }) => void;
		placeholder?: string;
		readOnly?: boolean;
		type?: string;
		value?: string;
	}) => (
		<input
			autoComplete={autoComplete}
			className={className}
			disabled={disabled}
			id={id}
			onChange={(event) =>
				onChange?.({ target: { value: event.target.value } })
			}
			placeholder={placeholder}
			readOnly={readOnly}
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

vi.mock("@/components/ui/select", () => {
	const { createContext, useContext, useEffect, useState } =
		require("react") as typeof import("react");

	const SelectContext = createContext<{
		disabled?: boolean;
		onValueChange?: (value: string) => void;
		options: Record<string, React.ReactNode>;
		registerOption: (value: string, label: React.ReactNode) => void;
		value?: string;
	}>({
		options: {},
		registerOption: () => {},
	});

	return {
		Select: ({
			children,
			disabled,
			onValueChange,
			value,
		}: {
			children: React.ReactNode;
			disabled?: boolean;
			onValueChange?: (value: string) => void;
			value?: string;
		}) => {
			const [options, setOptions] = useState<Record<string, React.ReactNode>>(
				{},
			);

			return (
				<SelectContext.Provider
					value={{
						disabled,
						onValueChange,
						options,
						registerOption: (optionValue, label) => {
							setOptions((prev) =>
								prev[optionValue] === label
									? prev
									: { ...prev, [optionValue]: label },
							);
						},
						value,
					}}
				>
					<div>{children}</div>
				</SelectContext.Provider>
			);
		},
		SelectContent: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
		SelectItem: ({
			children,
			disabled,
			value,
		}: {
			children: React.ReactNode;
			disabled?: boolean;
			value: string;
		}) => {
			const context = useContext(SelectContext);

			useEffect(() => {
				context.registerOption(value, children);
			}, [children, context, value]);

			return (
				<button
					type="button"
					aria-label={`select-item:${value}`}
					data-selected={context.value === value ? "true" : "false"}
					disabled={disabled || context.disabled}
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
		}) => (
			<button
				type="button"
				role="combobox"
				aria-controls="select-options"
				aria-expanded="false"
				className={className}
			>
				{children}
			</button>
		),
		SelectValue: ({ placeholder }: { placeholder?: string }) => {
			const context = useContext(SelectContext);
			return (
				<span>
					{(context.value ? context.options[context.value] : null) ??
						placeholder ??
						"select-value"}
				</span>
			);
		},
	};
});

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
	TooltipTrigger: ({ children }: { children: React.ReactNode }) => children,
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/lib/adminPolicyGroupLookup", () => ({
	readAdminPolicyGroupLookup: () => null,
	loadAdminPolicyGroupLookup: async () => {
		const allGroups: Array<Record<string, unknown>> = [];
		let offset = 0;
		let total = 0;

		do {
			const page = await mockState.listPolicies({
				limit: 100,
				offset,
			});
			allGroups.push(...page.items);
			total = page.total;
			offset += page.items.length;
			if (page.items.length === 0) {
				break;
			}
		} while (allGroups.length < total);

		return allGroups;
	},
}));

vi.mock("@/lib/format", () => ({
	formatBytes: (value: number) => `bytes:${value}`,
	formatDateAbsolute: (value: string) => `date:${value}`,
}));

vi.mock("@/services/adminService", () => ({
	adminUserService: {
		resetMfa: (...args: unknown[]) => mockState.resetMfa(...args),
		revokeSessions: (...args: unknown[]) => mockState.revokeSessions(...args),
		resetPassword: (...args: unknown[]) => mockState.resetPassword(...args),
	},
}));

function createUser(overrides: Record<string, unknown> = {}) {
	return {
		created_at: "2026-03-28T00:00:00Z",
		email: "alice@example.com",
		email_verified: true,
		id: 2,
		pending_email: null,
		policy_group_id: 1,
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

function createPolicyGroup(overrides: Record<string, unknown> = {}) {
	return {
		created_at: "2026-03-28T00:00:00Z",
		description: "",
		id: 1,
		is_default: false,
		is_enabled: true,
		items: [
			{
				id: 101,
				max_file_size: 0,
				min_file_size: 0,
				policy: {
					id: 201,
					name: "Primary Policy",
				},
				policy_id: 201,
				priority: 1,
			},
		],
		name: "Primary",
		updated_at: "2026-03-28T00:00:00Z",
		...overrides,
	};
}

function renderDialog(userOverrides: Record<string, unknown> = {}) {
	return render(
		<UserDetailDialog
			user={createUser(userOverrides)}
			open
			onOpenChange={vi.fn()}
			onUpdate={mockState.onUpdate}
		/>,
	);
}

async function waitForPolicyLoad(selectedPolicyLabel = "Primary") {
	await waitFor(
		() => {
			expect(mockState.listPolicies).toHaveBeenCalledWith({
				limit: 100,
				offset: 0,
			});

			const selectedTrigger = screen
				.queryAllByRole("combobox")
				.some(
					(trigger) =>
						within(trigger).queryByText(selectedPolicyLabel) !== null,
				);
			const matchingLabels = screen.queryAllByText(selectedPolicyLabel);
			const selectedOption = matchingLabels.some(
				(node) => node.closest('button[data-selected="true"]') !== null,
			);
			expect(selectedTrigger || selectedOption).toBe(true);
		},
		{ timeout: 3000 },
	);
}

describe("UserDetailDialog", () => {
	beforeEach(() => {
		mockState.handleApiError.mockReset();
		mockState.listPolicies.mockReset();
		mockState.onUpdate.mockReset();
		mockState.resetMfa.mockReset();
		mockState.revokeSessions.mockReset();
		mockState.resetPassword.mockReset();
		mockState.toastError.mockReset();
		mockState.toastSuccess.mockReset();

		mockState.listPolicies.mockResolvedValue({
			items: [
				createPolicyGroup(),
				createPolicyGroup({
					id: 2,
					name: "Archive",
				}),
			],
			total: 2,
		});
		mockState.onUpdate.mockResolvedValue(undefined);
		mockState.resetMfa.mockResolvedValue(undefined);
		mockState.revokeSessions.mockResolvedValue(undefined);
		mockState.resetPassword.mockResolvedValue(undefined);
	});

	it("loads policy groups on open and saves changed profile values", async () => {
		renderDialog();

		await waitForPolicyLoad();

		fireEvent.click(
			screen.getByRole("button", { name: "select-item:unverified" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "select-item:disabled" }),
		);
		fireEvent.click(screen.getByRole("button", { name: "select-item:admin" }));
		fireEvent.click(screen.getByRole("button", { name: "select-item:2" }));
		fireEvent.change(screen.getByLabelText("quota_mb"), {
			target: { value: "20" },
		});

		fireEvent.click(screen.getByRole("button", { name: /save_changes/i }));

		await waitFor(() => {
			expect(mockState.onUpdate).toHaveBeenCalledWith(2, {
				email_verified: false,
				role: "admin",
				status: "disabled",
				storage_quota: 20 * 1024 * 1024,
				policy_group_id: 2,
			});
		});
	});

	it("renders email verification details from the user model", async () => {
		renderDialog({
			email_verified: false,
			pending_email: "alice+next@example.com",
		});

		await waitForPolicyLoad();

		expect(
			screen.getByRole("button", { name: "select-item:unverified" }),
		).toHaveAttribute("data-selected", "true");
		expect(
			screen.getByDisplayValue("alice+next@example.com"),
		).toBeInTheDocument();
	});

	it("shows the invalid assignment warning when the current policy group is unavailable", async () => {
		mockState.listPolicies.mockResolvedValue({
			items: [createPolicyGroup({ id: 2, name: "Archive" })],
			total: 1,
		});

		renderDialog();

		await waitForPolicyLoad("#1");
		expect(
			screen.getByText("policy_group_invalid_assignment"),
		).toBeInTheDocument();
		expect(screen.getAllByText("#1")).not.toHaveLength(0);
	});

	it("loads additional policy group pages before treating the current assignment as invalid", async () => {
		const allGroups = Array.from({ length: 101 }, (_, index) =>
			createPolicyGroup({
				id: index + 1,
				name: `Group ${index + 1}`,
			}),
		);
		allGroups[100] = createPolicyGroup({
			id: 101,
			name: "Overflow Group",
		});
		mockState.listPolicies.mockImplementation(
			async (params?: { limit?: number; offset?: number }) => {
				const limit = params?.limit ?? 100;
				const offset = params?.offset ?? 0;
				return {
					items: allGroups.slice(offset, offset + limit),
					limit,
					offset,
					total: allGroups.length,
				};
			},
		);

		renderDialog({ policy_group_id: 101 });

		await waitFor(() => {
			expect(mockState.listPolicies).toHaveBeenCalledWith({
				limit: 100,
				offset: 100,
			});
			expect(screen.getAllByText("Overflow Group")).not.toHaveLength(0);
		});

		expect(
			screen.queryByText("policy_group_invalid_assignment"),
		).not.toBeInTheDocument();
		expect(screen.getAllByText("Overflow Group")).not.toHaveLength(0);
	});

	it("shows initial admin protections and keeps save hidden without changes", async () => {
		mockState.listPolicies.mockResolvedValue({
			items: [],
			total: 0,
		});

		renderDialog({
			id: 1,
			email_verified: true,
			policy_group_id: null,
			role: "admin",
			username: "root",
		});

		await waitForPolicyLoad("select_policy_group");

		expect(screen.getAllByText("initial_admin_protected")).toHaveLength(3);
		expect(
			screen.getByRole("button", { name: "select-item:unverified" }),
		).toBeDisabled();
		expect(
			screen.getByRole("button", { name: "select-item:disabled" }),
		).toBeDisabled();
		expect(
			screen.getByRole("button", { name: "select-item:user" }),
		).toBeDisabled();
		expect(screen.queryByRole("button", { name: /save_changes/i })).toBeNull();
		expect(
			screen.getByText("policy_group_no_assignable_groups"),
		).toBeInTheDocument();
	});

	it("shows unlimited instead of 0 for an unlimited quota user", async () => {
		renderDialog({
			storage_quota: 0,
		});

		await waitForPolicyLoad();

		const quotaInput = screen.getByLabelText("quota_mb");
		expect(quotaInput).toHaveValue(null);
		expect(quotaInput).toHaveAttribute("placeholder", "quota_unlimited_short");
	});

	it("caps the dialog height and keeps the two columns independently scrollable on desktop", async () => {
		const { container } = renderDialog();

		await waitForPolicyLoad();

		const shell = container.querySelector(
			".flex.min-h-0.flex-1.flex-col.overflow-y-auto.lg\\:overflow-hidden",
		);
		const leftColumn = container.querySelector(
			".border-b.bg-muted\\/20.lg\\:min-h-0.lg\\:w-80.lg\\:flex-none.lg\\:overflow-y-auto",
		);
		const rightColumn = container.querySelector(
			".min-h-0.min-w-0.lg\\:flex-1.lg\\:overflow-y-auto",
		);
		const footer = screen.getByText("user_details_footer_hint").parentElement;

		expect(
			container.querySelector(
				".overflow-hidden.max-h-\\[min\\(860px\\,calc\\(100vh-2rem\\)\\)\\]",
			),
		).not.toBeNull();
		expect(shell).not.toBeNull();
		expect(
			container.querySelector(
				".flex.min-h-full.flex-col.lg\\:h-full.lg\\:min-h-0.lg\\:flex-1.lg\\:flex-row",
			),
		).not.toBeNull();
		expect(leftColumn).not.toBeNull();
		expect(rightColumn).not.toBeNull();
		expect(container.querySelector(".space-y-4.p-6")).not.toBeNull();
		expect(
			container.querySelector(".min-w-0.p-6.lg\\:overflow-y-auto"),
		).toBeNull();
		expect(
			container.querySelector(
				".mx-0.mb-0.w-full.shrink-0.border-t.bg-muted\\/10.px-6.py-4",
			),
		).not.toBeNull();
		expect(footer).not.toBeNull();
		expect(shell?.contains(footer as Node)).toBe(false);
		expect(leftColumn?.contains(footer as Node)).toBe(false);
		expect(rightColumn?.contains(footer as Node)).toBe(false);
	});

	it("resets the user's password from the detail dialog", async () => {
		renderDialog();

		await waitForPolicyLoad();
		expect(screen.getByLabelText("password")).toHaveAttribute(
			"autocomplete",
			"new-password",
		);
		expect(screen.getByLabelText("confirm_password")).toHaveAttribute(
			"autocomplete",
			"new-password",
		);

		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "newsecret456" },
		});
		fireEvent.change(screen.getByLabelText("confirm_password"), {
			target: { value: "newsecret456" },
		});
		fireEvent.click(
			screen.getAllByRole("button", { name: "reset_password" })[0],
		);

		await waitFor(() => {
			expect(mockState.resetPassword).toHaveBeenCalledWith(2, {
				password: "newsecret456",
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"password_reset_success",
		);
	});

	it("validates admin password reset fields before submitting", async () => {
		renderDialog();

		await waitForPolicyLoad();
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "short" },
		});
		fireEvent.change(screen.getByLabelText("confirm_password"), {
			target: { value: "different" },
		});
		fireEvent.click(
			screen.getAllByRole("button", { name: "reset_password" })[0],
		);

		await waitFor(() => {
			expect(screen.getByText("password_confirm_mismatch")).toBeInTheDocument();
		});
		expect(mockState.resetPassword).not.toHaveBeenCalled();
	});

	it("resets user MFA from the detail dialog", async () => {
		renderDialog();

		await waitForPolicyLoad();

		const resetMfaButton = screen.getByRole("button", {
			name: /reset_mfa/i,
		});

		expect(resetMfaButton).toHaveAttribute("data-variant", "destructive");

		fireEvent.click(resetMfaButton);

		await waitFor(() => {
			expect(mockState.resetMfa).toHaveBeenCalledWith(2);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("reset_mfa_success");
	});

	it("revokes user sessions from the detail dialog", async () => {
		renderDialog();

		await waitForPolicyLoad();

		const revokeButton = screen.getByRole("button", {
			name: /revoke_sessions/i,
		});

		expect(revokeButton).toHaveAttribute("data-variant", "destructive");

		fireEvent.click(revokeButton);

		await waitFor(() => {
			expect(mockState.revokeSessions).toHaveBeenCalledWith(2);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"revoke_sessions_success",
		);
	});

	it("routes revoke session failures through handleApiError", async () => {
		const error = new Error("revoke failed");
		mockState.revokeSessions.mockRejectedValueOnce(error);

		renderDialog();

		await waitForPolicyLoad();
		fireEvent.click(
			screen.getByRole("button", {
				name: /revoke_sessions/i,
			}),
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
	});
});
