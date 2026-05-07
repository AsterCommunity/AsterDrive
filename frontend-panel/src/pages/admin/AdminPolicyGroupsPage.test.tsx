import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createContext, useContext } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invalidateAdminPolicyGroupLookup } from "@/lib/adminPolicyGroupLookup";
import { invalidateAdminPolicyLookup } from "@/lib/adminPolicyLookup";
import AdminPolicyGroupsPage from "@/pages/admin/AdminPolicyGroupsPage";

const MB = 1024 * 1024;

const mockState = vi.hoisted(() => ({
	createGroup: vi.fn(),
	deleteGroup: vi.fn(),
	groupItems: [] as Array<Record<string, unknown>>,
	handleApiError: vi.fn(),
	listGroups: vi.fn(),
	listPolicies: vi.fn(),
	migrateUsers: vi.fn(),
	policies: [] as Array<Record<string, unknown>>,
	searchParams: "",
	setSearchParams: vi.fn(),
	toastSuccess: vi.fn(),
	updateGroup: vi.fn(),
}));

vi.mock("react-router-dom", () => ({
	useSearchParams: () => [
		new URLSearchParams(mockState.searchParams),
		mockState.setSearchParams,
	],
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, params?: Record<string, unknown>) => {
			if (key === "policy_group_more_rules" && params?.count != null) {
				return `+${params.count} more`;
			}
			if (key === "policy_group_priority_short" && params?.priority != null) {
				return `Priority ${params.priority}`;
			}
			return key;
		},
	}),
	initReactI18next: {
		type: "3rdParty",
		init: () => undefined,
	},
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/common/AdminTableList", () => ({
	AdminTableList: ({
		items,
		loading,
		emptyTitle,
		emptyDescription,
		headerRow,
		renderRow,
	}: {
		items: unknown[];
		loading: boolean;
		emptyTitle: string;
		emptyDescription?: string;
		headerRow: React.ReactNode;
		renderRow: (item: never) => React.ReactNode;
	}) =>
		loading ? (
			<div>loading</div>
		) : items.length === 0 ? (
			<div>{`${emptyTitle}:${emptyDescription}`}</div>
		) : (
			<table>
				{headerRow}
				<tbody>{items.map((item) => renderRow(item as never))}</tbody>
			</table>
		),
}));

vi.mock("@/components/common/ConfirmDialog", () => ({
	ConfirmDialog: ({
		open,
		title,
		description,
		confirmLabel,
		onConfirm,
	}: {
		open: boolean;
		title: string;
		description?: string;
		confirmLabel?: string;
		onConfirm: () => void;
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
	}: {
		title: string;
		description: string;
		actions?: React.ReactNode;
	}) => (
		<div>
			<h1>{title}</h1>
			<p>{description}</p>
			<div>{actions}</div>
		</div>
	),
}));

vi.mock("@/components/layout/AdminPageShell", () => ({
	AdminPageShell: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({
		children,
		className,
		variant,
	}: {
		children: React.ReactNode;
		className?: string;
		variant?: string;
	}) => (
		<span className={className} data-variant={variant}>
			{children}
		</span>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		className,
		disabled,
		onClick,
		type,
		variant,
		...props
	}: {
		[key: string]: unknown;
		children: React.ReactNode;
		className?: string;
		disabled?: boolean;
		onClick?: () => void;
		type?: "button" | "submit";
		variant?: string;
	}) => (
		<button
			type={type ?? "button"}
			className={className}
			data-variant={variant}
			disabled={disabled}
			onClick={onClick}
			{...props}
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
	DialogDescription: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogFooter: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
	DialogHeader: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({
		"aria-invalid": ariaInvalid,
		className,
		id,
		onChange,
		placeholder,
		type,
		value,
	}: {
		"aria-invalid"?: boolean;
		className?: string;
		id?: string;
		onChange?: (event: { target: { value: string } }) => void;
		placeholder?: string;
		type?: string;
		value?: string;
	}) => (
		<input
			aria-invalid={ariaInvalid}
			className={className}
			id={id}
			onChange={(event) =>
				onChange?.({ target: { value: event.target.value } })
			}
			placeholder={placeholder}
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

vi.mock("@/components/ui/select", () => {
	type SelectOption = {
		label: string;
		value: string;
	};
	const SelectContext = createContext<{
		onValueChange?: (value: string) => void;
		onOpenChange?: (open: boolean) => void;
		disabled?: boolean;
		items?: SelectOption[];
		value?: string;
	}>({});

	return {
		Select: ({
			children,
			disabled,
			items,
			onOpenChange,
			onValueChange,
			value,
		}: {
			children: React.ReactNode;
			disabled?: boolean;
			items?: SelectOption[];
			onOpenChange?: (open: boolean) => void;
			onValueChange?: (value: string) => void;
			value?: string;
		}) => (
			<SelectContext.Provider
				value={{ disabled, items, onOpenChange, onValueChange, value }}
			>
				<div data-value={value}>{children}</div>
			</SelectContext.Provider>
		),
		SelectContent: ({
			children,
			className,
			onScroll,
		}: {
			children: React.ReactNode;
			className?: string;
			onScroll?: (event: React.UIEvent<HTMLDivElement>) => void;
		}) => (
			<div
				data-testid="select-content"
				className={className}
				onScroll={onScroll}
			>
				{children}
			</div>
		),
		SelectGroup: ({ children }: { children: React.ReactNode }) => (
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
		SelectValue: ({ placeholder }: { placeholder?: string }) => {
			const context = useContext(SelectContext);
			const selectedLabel = context.items?.find(
				(option) => option.value === context.value,
			)?.label;
			return <span>{selectedLabel ?? placeholder ?? "select-value"}</span>;
		},
		SelectLabel: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
		SelectSeparator: () => <hr />,
	};
});

vi.mock("@/components/ui/switch", () => ({
	Switch: ({
		checked,
		id,
		onCheckedChange,
	}: {
		checked: boolean;
		id?: string;
		onCheckedChange?: (checked: boolean) => void;
	}) => (
		<button
			type="button"
			aria-label={`switch:${id ?? "toggle"}:${checked}`}
			onClick={() => onCheckedChange?.(!checked)}
		/>
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
	}) => <>{render ?? children}</>,
}));

vi.mock("@/components/ui/table", () => ({
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
		<td className={className} onClick={onClick} onKeyDown={onKeyDown}>
			{children}
		</td>
	),
	TableHead: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <th className={className}>{children}</th>,
	TableHeader: ({ children }: { children: React.ReactNode }) => (
		<thead>{children}</thead>
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
			className={className}
			onClick={onClick}
			onKeyDown={onKeyDown}
			tabIndex={tabIndex}
		>
			{children}
		</tr>
	),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/lib/format", () => ({
	formatBytes: (value: number) => `${value} B`,
	formatDateAbsolute: (value: string) => `formatted:${value}`,
}));

vi.mock("@/services/adminService", () => ({
	adminPolicyGroupService: {
		create: (...args: unknown[]) => mockState.createGroup(...args),
		delete: (...args: unknown[]) => mockState.deleteGroup(...args),
		list: (...args: unknown[]) => mockState.listGroups(...args),
		listAll: async (pageSize = 100) => {
			const allGroups: Array<Record<string, unknown>> = [];
			let offset = 0;
			let total = 0;

			do {
				const page = await mockState.listGroups({
					limit: pageSize,
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
		migrateUsers: (...args: unknown[]) => mockState.migrateUsers(...args),
		update: (...args: unknown[]) => mockState.updateGroup(...args),
	},
	adminPolicyService: {
		list: (...args: unknown[]) => mockState.listPolicies(...args),
	},
}));

function createPolicy(overrides: Record<string, unknown> = {}) {
	return {
		allowed_types: [],
		base_path: "",
		bucket: "",
		chunk_size: 5 * MB,
		created_at: "2026-03-28T00:00:00Z",
		driver_type: "local",
		endpoint: "",
		id: 1,
		is_default: false,
		max_file_size: 0,
		name: "Local Policy",
		options: {},
		updated_at: "2026-03-28T00:00:00Z",
		...overrides,
	};
}

function createGroup(overrides: Record<string, unknown> = {}) {
	return {
		created_at: "2026-03-28T00:00:00Z",
		description: "",
		id: 1,
		is_default: false,
		is_enabled: true,
		items: [
			{
				id: 11,
				max_file_size: 0,
				min_file_size: 0,
				policy: createPolicy(),
				policy_id: 1,
				priority: 1,
			},
		],
		name: "Default Group",
		updated_at: "2026-03-28T00:00:00Z",
		...overrides,
	};
}

describe("AdminPolicyGroupsPage", () => {
	beforeEach(() => {
		mockState.createGroup.mockReset();
		mockState.deleteGroup.mockReset();
		invalidateAdminPolicyGroupLookup();
		invalidateAdminPolicyLookup();
		mockState.groupItems = [];
		mockState.handleApiError.mockReset();
		mockState.listGroups.mockReset();
		mockState.listPolicies.mockReset();
		mockState.migrateUsers.mockReset();
		mockState.policies = [createPolicy()];
		mockState.searchParams = "";
		mockState.setSearchParams.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.updateGroup.mockReset();

		mockState.createGroup.mockResolvedValue(createGroup({ id: 99 }));
		mockState.deleteGroup.mockResolvedValue(undefined);
		mockState.listGroups.mockImplementation(
			async (params?: { limit?: number; offset?: number }) => {
				const limit = params?.limit ?? 20;
				const offset = params?.offset ?? 0;
				return {
					items: mockState.groupItems.slice(offset, offset + limit),
					limit,
					offset,
					total: mockState.groupItems.length,
				};
			},
		);
		mockState.listPolicies.mockImplementation(
			async (params?: { limit?: number; offset?: number }) => {
				const limit = params?.limit ?? 100;
				const offset = params?.offset ?? 0;
				return {
					items: mockState.policies.slice(offset, offset + limit),
					limit,
					offset,
					total: mockState.policies.length,
				};
			},
		);
		mockState.migrateUsers.mockResolvedValue({
			affected_users: 2,
			migrated_assignments: 3,
			source_group_id: 1,
			target_group_id: 2,
		});
		mockState.updateGroup.mockImplementation(async (id, payload) =>
			createGroup({
				...(payload as Record<string, unknown>),
				id,
			}),
		);
	});

	it("renders groups and opens edit when a row is clicked", async () => {
		mockState.groupItems = [
			createGroup({
				name: "Default Group",
				description: "Primary uploads",
				is_default: true,
			}),
		];

		render(<AdminPolicyGroupsPage />);

		await waitFor(() => {
			expect(screen.getByText("Default Group")).toBeInTheDocument();
		});
		expect(screen.getByText("Primary uploads")).toBeInTheDocument();
		expect(
			screen.getByText("formatted:2026-03-28T00:00:00Z"),
		).toBeInTheDocument();
		expect(
			screen.getByText("policy_group_delete_default_blocked"),
		).toBeInTheDocument();

		fireEvent.click(screen.getByText("Default Group"));

		expect(screen.getByDisplayValue("Default Group")).toBeInTheDocument();
		expect(screen.getByDisplayValue("Primary uploads")).toBeInTheDocument();
	});

	it("creates a policy group and converts size inputs from MB to bytes", async () => {
		mockState.policies = [
			createPolicy({ id: 1, name: "Hot Storage" }),
			createPolicy({ id: 2, name: "Archive Storage" }),
		];

		render(<AdminPolicyGroupsPage />);

		await waitFor(() => {
			expect(
				screen.getByRole("button", { name: /new_policy_group/i }),
			).toBeEnabled();
		});

		fireEvent.click(screen.getByRole("button", { name: /new_policy_group/i }));

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Tiered Group" },
		});
		fireEvent.change(screen.getByLabelText("policy_group_description"), {
			target: { value: "Route uploads by size" },
		});
		fireEvent.change(screen.getByLabelText("policy_group_min_size_mb"), {
			target: { value: "10" },
		});
		fireEvent.change(screen.getByLabelText("policy_group_max_size_mb"), {
			target: { value: "512" },
		});

		fireEvent.click(screen.getByRole("button", { name: /core:create/i }));

		await waitFor(() => {
			expect(mockState.createGroup).toHaveBeenCalledWith({
				description: "Route uploads by size",
				is_default: false,
				is_enabled: true,
				items: [
					{
						max_file_size: 512 * MB,
						min_file_size: 10 * MB,
						policy_id: 1,
						priority: 1,
					},
				],
				name: "Tiered Group",
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("policy_group_created");
	});

	it("blocks submitting a default policy group when it is disabled", async () => {
		render(<AdminPolicyGroupsPage />);

		await waitFor(() => {
			expect(
				screen.getByRole("button", { name: /new_policy_group/i }),
			).toBeEnabled();
		});

		fireEvent.click(screen.getByRole("button", { name: /new_policy_group/i }));
		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Invalid Default Group" },
		});
		fireEvent.click(
			screen.getByRole("button", {
				name: "switch:policy-group-default:false",
			}),
		);
		fireEvent.click(
			screen.getByRole("button", {
				name: "switch:policy-group-enabled:true",
			}),
		);
		fireEvent.click(screen.getByRole("button", { name: /core:create/i }));

		expect(mockState.createGroup).not.toHaveBeenCalled();
		expect(
			screen.getByText("policy_group_default_requires_enabled"),
		).toBeInTheDocument();
	});

	it("prevents deleting the default group from the table", async () => {
		mockState.groupItems = [
			createGroup({
				name: "System Default",
				is_default: true,
			}),
		];

		render(<AdminPolicyGroupsPage />);

		const deleteButton = await screen.findByRole("button", {
			name: "delete_policy_group",
		});
		expect(deleteButton).toBeDisabled();

		fireEvent.click(deleteButton);

		expect(mockState.deleteGroup).not.toHaveBeenCalled();
		expect(
			screen.queryByText('delete_policy_group "System Default"?'),
		).not.toBeInTheDocument();
	});

	it("migrates users from one policy group to another", async () => {
		mockState.groupItems = [
			createGroup({ id: 1, name: "Hot Group" }),
			createGroup({ id: 2, name: "Archive Group" }),
		];

		render(<AdminPolicyGroupsPage />);

		const migrateButtons = await screen.findAllByRole("button", {
			name: "migrate_policy_group_users",
		});
		fireEvent.click(migrateButtons[0]);

		expect(screen.getByText("policy_group_migration_desc")).toBeInTheDocument();
		expect(screen.getAllByText("Hot Group")).not.toHaveLength(0);

		fireEvent.click(screen.getByRole("button", { name: "select-item:2" }));
		fireEvent.click(
			screen.getByRole("button", {
				name: /policy_group_migration_confirm/i,
			}),
		);

		await waitFor(() => {
			expect(mockState.migrateUsers).toHaveBeenCalledWith(1, {
				target_group_id: 2,
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"policy_group_migration_success",
		);
	});

	it("loads all policy groups for migration targets when the current page is incomplete", async () => {
		mockState.searchParams = "offset=20";
		mockState.groupItems = Array.from({ length: 21 }, (_, index) =>
			createGroup({
				id: index + 1,
				name: `Group ${index + 1}`,
			}),
		);

		render(<AdminPolicyGroupsPage />);

		const migrateButton = await screen.findByRole("button", {
			name: "migrate_policy_group_users",
		});
		expect(migrateButton).not.toBeDisabled();

		fireEvent.click(migrateButton);

		await waitFor(() => {
			expect(
				screen.getByRole("button", { name: "select-item:1" }),
			).toBeInTheDocument();
		});

		fireEvent.click(screen.getByRole("button", { name: "select-item:1" }));
		fireEvent.click(
			screen.getByRole("button", {
				name: /policy_group_migration_confirm/i,
			}),
		);

		await waitFor(() => {
			expect(mockState.migrateUsers).toHaveBeenCalledWith(21, {
				target_group_id: 1,
			});
		});
	});

	it("disables migration when there is no target policy group", async () => {
		mockState.groupItems = [createGroup({ id: 1, name: "Only Group" })];

		render(<AdminPolicyGroupsPage />);

		const migrateButton = await screen.findByRole("button", {
			name: "migrate_policy_group_users",
		});

		expect(migrateButton).toBeDisabled();
	});

	it("loads all policy lookup pages before opening the rule dropdown", async () => {
		mockState.policies = Array.from({ length: 120 }, (_, index) =>
			createPolicy({
				id: index + 1,
				name: `Policy ${index + 1}`,
			}),
		);

		render(<AdminPolicyGroupsPage />);

		await waitFor(() => {
			expect(mockState.listPolicies).toHaveBeenCalledWith({
				limit: 100,
				offset: 0,
			});
		});
		await waitFor(() => {
			expect(mockState.listPolicies).toHaveBeenCalledWith({
				limit: 100,
				offset: 100,
			});
		});

		fireEvent.click(screen.getByRole("button", { name: /new_policy_group/i }));

		expect(screen.getByText("Policy 101")).toBeInTheDocument();
	});

	it("filters policy options with the dialog search input while keeping the selected policy visible", async () => {
		mockState.policies = [
			createPolicy({ id: 1, name: "Hot Storage" }),
			createPolicy({ id: 2, name: "Archive Storage", driver_type: "s3" }),
			createPolicy({ id: 3, name: "Cold Storage" }),
		];

		render(<AdminPolicyGroupsPage />);

		await waitFor(() => {
			expect(
				screen.getByRole("button", { name: /new_policy_group/i }),
			).toBeEnabled();
		});

		fireEvent.click(screen.getByRole("button", { name: /new_policy_group/i }));
		fireEvent.change(
			screen.getByPlaceholderText("policy_group_policy_search_placeholder"),
			{
				target: { value: "archive" },
			},
		);

		expect(screen.getByText("Archive Storage")).toBeInTheDocument();
		expect(screen.getAllByText("Hot Storage")).not.toHaveLength(0);
		expect(screen.queryByText("Cold Storage")).not.toBeInTheDocument();
	});
});
