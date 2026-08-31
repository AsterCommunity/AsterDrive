import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createContext, use } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invalidateAdminPolicyGroupLookup } from "@/lib/adminPolicyGroupLookup";
import { invalidateAdminPolicyLookup } from "@/lib/adminPolicyLookup";
import AdminPolicyGroupEditPage from "@/pages/admin/AdminPolicyGroupEditPage";

const MB = 1024 * 1024;

const mockState = vi.hoisted(() => ({
	createGroup: vi.fn(),
	getGroup: vi.fn(),
	groupId: "new",
	handleApiError: vi.fn(),
	listPolicies: vi.fn(),
	navigate: vi.fn(),
	policies: [] as Array<Record<string, unknown>>,
	simulateGroup: vi.fn(),
	toastSuccess: vi.fn(),
	updateGroup: vi.fn(),
}));

vi.mock("react-router-dom", () => ({
	Navigate: () => null,
	useNavigate: () => mockState.navigate,
	useParams: () => ({ groupId: mockState.groupId }),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, params?: Record<string, unknown>) => {
			if (params?.index != null) {
				return `${key}:${params.index}`;
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
		description?: string;
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
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <span className={className}>{children}</span>,
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		className,
		disabled,
		onClick,
		title,
		type,
	}: {
		[key: string]: unknown;
		children?: React.ReactNode;
		className?: string;
		disabled?: boolean;
		onClick?: () => void;
		title?: string;
		type?: "button" | "submit";
	}) => (
		<button
			type={type ?? "button"}
			className={className}
			disabled={disabled}
			onClick={onClick}
			aria-label={title}
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
		<div>{children}</div>
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
		"aria-label": ariaLabel,
		"aria-invalid": ariaInvalid,
		className,
		id,
		onChange,
		placeholder,
		type,
		value,
	}: {
		"aria-label"?: string;
		"aria-invalid"?: boolean;
		className?: string;
		id?: string;
		onChange?: (event: { target: { value: string } }) => void;
		placeholder?: string;
		type?: string;
		value?: string;
	}) => (
		<input
			aria-label={ariaLabel}
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
		SelectContent: ({ children }: { children: React.ReactNode }) => (
			<div data-testid="select-content">{children}</div>
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
			const context = use(SelectContext);

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
		SelectTrigger: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
		SelectValue: ({ placeholder }: { placeholder?: string }) => {
			const context = use(SelectContext);
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

vi.mock("@/hooks/useApiError", () => ({
	getApiErrorMessage: (error: unknown) =>
		error instanceof Error ? error.message : String(error),
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/services/adminService", () => ({
	adminPolicyGroupService: {
		create: (...args: unknown[]) => mockState.createGroup(...args),
		get: (...args: unknown[]) => mockState.getGroup(...args),
		simulate: (...args: unknown[]) => mockState.simulateGroup(...args),
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
		connector_id: "asterdrive.storage.local",
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

function createRule(
	id: number,
	priority: number,
	policyId: number,
	overrides: Record<string, unknown> = {},
) {
	return {
		id,
		name: `Rule ${priority}`,
		description: "",
		priority,
		is_enabled: true,
		matcher: {
			min_file_size: 0,
			max_file_size: 0,
			extensions: [],
			compound_extensions: [],
			extensionless: null,
			categories: [],
		},
		selection_mode: "first_available",
		unavailable_behavior: "next_rule",
		targets: [
			{
				id: id * 10,
				policy_id: policyId,
				weight: 100,
				is_enabled: true,
				accepting_new_writes: true,
				stable_order: 1,
				policy: createPolicy({ id: policyId }),
			},
		],
		...overrides,
	};
}

function createGroup(overrides: Record<string, unknown> = {}) {
	return {
		created_at: "2026-03-28T00:00:00Z",
		description: "",
		id: 7,
		is_default: false,
		is_enabled: true,
		rules: [createRule(11, 1, 1)],
		name: "Default Group",
		updated_at: "2026-03-28T00:00:00Z",
		...overrides,
	};
}

describe("AdminPolicyGroupEditPage", () => {
	beforeEach(() => {
		invalidateAdminPolicyGroupLookup();
		invalidateAdminPolicyLookup();
		mockState.createGroup.mockReset();
		mockState.getGroup.mockReset();
		mockState.groupId = "new";
		mockState.handleApiError.mockReset();
		mockState.listPolicies.mockReset();
		mockState.navigate.mockReset();
		mockState.policies = [createPolicy()];
		mockState.simulateGroup.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.updateGroup.mockReset();

		mockState.createGroup.mockResolvedValue(createGroup({ id: 99 }));
		mockState.getGroup.mockResolvedValue(createGroup());
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
		mockState.updateGroup.mockImplementation(async (id, payload) =>
			createGroup({
				...(payload as Record<string, unknown>),
				id,
			}),
		);
	});

	it("creates a policy group and converts size inputs from MB to bytes", async () => {
		mockState.policies = [
			createPolicy({ id: 1, name: "Hot Storage" }),
			createPolicy({ id: 2, name: "Archive Storage" }),
		];

		render(<AdminPolicyGroupEditPage />);

		await waitFor(() => {
			expect(screen.getByLabelText("core:name")).toBeInTheDocument();
		});

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Tiered Group" },
		});
		fireEvent.change(screen.getByLabelText(/policy_group_description/), {
			target: { value: "Route uploads by size" },
		});
		fireEvent.change(screen.getByLabelText("policy_group_min_size_mb"), {
			target: { value: "10" },
		});
		fireEvent.change(screen.getByLabelText("policy_group_max_size_mb"), {
			target: { value: "512" },
		});

		fireEvent.click(screen.getAllByRole("button", { name: /core:create/i })[0]);

		await waitFor(() => {
			expect(mockState.createGroup).toHaveBeenCalledWith({
				description: "Route uploads by size",
				is_default: false,
				is_enabled: true,
				admission: {
					allowed_extensions: [],
					denied_extensions: [],
					accept_extensionless: true,
					allowed_categories: [],
					denied_categories: [],
					max_file_size: 0,
				},
				execution_preference: "automatic",
				rules: [
					{
						name: "Rule 1",
						description: "",
						priority: 1,
						is_enabled: true,
						matcher: {
							min_file_size: 10 * MB,
							max_file_size: 512 * MB,
							extensions: [],
							compound_extensions: [],
							extensionless: null,
							categories: [],
						},
						selection_mode: "first_available",
						unavailable_behavior: "next_rule",
						targets: [
							{
								policy_id: 1,
								weight: 100,
								is_enabled: true,
								accepting_new_writes: true,
								stable_order: 1,
							},
						],
					},
				],
				name: "Tiered Group",
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("policy_group_created");
		expect(mockState.navigate).toHaveBeenCalledWith("/admin/policy-groups", {
			viewTransition: false,
		});
	});

	it("blocks submitting a default policy group when it is disabled", async () => {
		render(<AdminPolicyGroupEditPage />);

		await waitFor(() => {
			expect(screen.getByLabelText("core:name")).toBeInTheDocument();
		});

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
		fireEvent.click(screen.getAllByRole("button", { name: /core:create/i })[0]);

		expect(mockState.createGroup).not.toHaveBeenCalled();
		expect(
			screen.getByText("policy_group_default_requires_enabled"),
		).toBeInTheDocument();
	});

	it("loads all policy lookup pages before rendering rule targets", async () => {
		mockState.policies = Array.from({ length: 120 }, (_, index) =>
			createPolicy({
				id: index + 1,
				name: `Policy ${index + 1}`,
			}),
		);

		render(<AdminPolicyGroupEditPage />);

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

		expect((await screen.findAllByText("Policy 101")).length).toBeGreaterThan(
			0,
		);
	});

	it("filters policy options with the search input while keeping the selected policy visible", async () => {
		mockState.policies = [
			createPolicy({ id: 1, name: "Hot Storage" }),
			createPolicy({
				id: 2,
				name: "Archive Storage",
				connector_id: "asterdrive.storage.s3",
			}),
			createPolicy({ id: 3, name: "Cold Storage" }),
		];

		render(<AdminPolicyGroupEditPage />);

		await waitFor(() => {
			expect(screen.getByLabelText("core:name")).toBeInTheDocument();
		});

		fireEvent.change(
			screen.getByPlaceholderText("policy_group_policy_search_placeholder"),
			{
				target: { value: "archive" },
			},
		);

		expect(
			(await screen.findAllByText("Archive Storage")).length,
		).toBeGreaterThan(0);
		expect(screen.getAllByText("Hot Storage").length).toBeGreaterThan(0);
		expect(screen.queryByText("Cold Storage")).not.toBeInTheDocument();
	});

	it("loads an existing group ordered by priority and submits updates", async () => {
		mockState.groupId = "7";
		mockState.policies = [
			createPolicy({ id: 1, name: "Hot Storage" }),
			createPolicy({ id: 2, name: "Archive Storage" }),
		];
		mockState.getGroup.mockResolvedValue(
			createGroup({
				name: "Existing Group",
				description: "Already there",
				rules: [createRule(12, 2, 2), createRule(11, 1, 1)],
			}),
		);

		render(<AdminPolicyGroupEditPage />);

		await waitFor(() => {
			expect(screen.getByDisplayValue("Existing Group")).toBeInTheDocument();
		});
		expect(screen.getByDisplayValue("Already there")).toBeInTheDocument();
		// 表单按 priority 排序渲染，卡片只展示顺序，不重复显示“规则 N”标题。
		const ruleOrders = await screen.findAllByText(/policy_group_rule_order/);
		expect(ruleOrders).toHaveLength(2);

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Renamed Group" },
		});
		fireEvent.click(
			screen.getAllByRole("button", { name: /save_changes/i })[0],
		);

		await waitFor(() => {
			expect(mockState.updateGroup).toHaveBeenCalledWith(
				7,
				expect.objectContaining({
					name: "Renamed Group",
					rules: [
						expect.objectContaining({
							priority: 1,
							targets: [expect.objectContaining({ policy_id: 1 })],
						}),
						expect.objectContaining({
							priority: 2,
							targets: [expect.objectContaining({ policy_id: 2 })],
						}),
					],
				}),
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("policy_group_updated");
		expect(mockState.navigate).toHaveBeenCalledWith("/admin/policy-groups", {
			viewTransition: false,
		});
	});

	it("reorders rules with the move buttons and derives priorities from row order", async () => {
		mockState.groupId = "7";
		mockState.policies = [
			createPolicy({ id: 1, name: "Hot Storage" }),
			createPolicy({ id: 2, name: "Archive Storage" }),
		];
		mockState.getGroup.mockResolvedValue(
			createGroup({
				rules: [createRule(11, 1, 1), createRule(12, 2, 2)],
			}),
		);

		render(<AdminPolicyGroupEditPage />);

		await waitFor(() => {
			expect(screen.getByDisplayValue("Default Group")).toBeInTheDocument();
		});

		// 第一条规则下移：Archive Storage 规则变成第一条
		fireEvent.click(
			screen.getAllByRole("button", {
				name: "policy_group_rule_move_down",
			})[0],
		);

		fireEvent.click(
			screen.getAllByRole("button", { name: /save_changes/i })[0],
		);

		await waitFor(() => {
			expect(mockState.updateGroup).toHaveBeenCalledWith(
				7,
				expect.objectContaining({
					rules: [
						expect.objectContaining({
							priority: 1,
							targets: [expect.objectContaining({ policy_id: 2 })],
						}),
						expect.objectContaining({
							priority: 2,
							targets: [expect.objectContaining({ policy_id: 1 })],
						}),
					],
				}),
			);
		});
	});

	it("requires a second click to confirm rule deletion", async () => {
		mockState.groupId = "7";
		mockState.policies = [
			createPolicy({ id: 1, name: "Hot Storage" }),
			createPolicy({ id: 2, name: "Archive Storage" }),
		];
		mockState.getGroup.mockResolvedValue(
			createGroup({
				rules: [createRule(11, 1, 1), createRule(12, 2, 2)],
			}),
		);

		render(<AdminPolicyGroupEditPage />);

		await waitFor(() => {
			expect(screen.getByDisplayValue("Default Group")).toBeInTheDocument();
		});
		expect(screen.getAllByText(/policy_group_rule_order/)).toHaveLength(2);

		// 第一次点击：进入确认态，规则仍在
		fireEvent.click(
			screen.getAllByRole("button", {
				name: "policy_group_remove_rule",
			})[0],
		);
		expect(screen.getAllByText(/policy_group_rule_order/)).toHaveLength(2);
		expect(
			screen.getByRole("button", {
				name: "policy_group_remove_rule_confirm",
			}),
		).toBeInTheDocument();

		// 再按一次：播放收缩动画后真正删除
		fireEvent.click(
			screen.getByRole("button", {
				name: "policy_group_remove_rule_confirm",
			}),
		);
		await waitFor(() => {
			expect(screen.getAllByText(/policy_group_rule_order/)).toHaveLength(1);
		});
	});

	it("shows a not-found state when the group fails to load", async () => {
		mockState.groupId = "7";
		mockState.getGroup.mockRejectedValue(new Error("not found"));

		render(<AdminPolicyGroupEditPage />);

		await waitFor(() => {
			expect(screen.getByText("policy_group_not_found")).toBeInTheDocument();
		});
	});

	it("validates simulation input and routes backend simulation failures", async () => {
		mockState.groupId = "7";
		mockState.getGroup.mockResolvedValue(createGroup({ name: "Simulated" }));
		render(<AdminPolicyGroupEditPage />);
		await waitFor(() =>
			expect(screen.getByDisplayValue("Simulated")).toBeInTheDocument(),
		);
		fireEvent.click(
			screen.getByRole("button", { name: /policy_group_simulator_open/ }),
		);
		fireEvent.change(screen.getByLabelText("policy_group_simulator_filename"), {
			target: { value: " " },
		});
		fireEvent.click(
			screen.getByRole("button", { name: /policy_group_simulator_run/ }),
		);
		expect(
			screen.getByText("policy_group_simulator_filename_required"),
		).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("policy_group_simulator_filename"), {
			target: { value: "photo.jpg" },
		});
		mockState.simulateGroup.mockRejectedValue(new Error("simulation failed"));
		fireEvent.click(
			screen.getByRole("button", { name: /policy_group_simulator_run/ }),
		);
		await waitFor(() =>
			expect(screen.getByText("simulation failed")).toBeInTheDocument(),
		);
	});
});
