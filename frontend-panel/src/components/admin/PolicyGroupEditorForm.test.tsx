import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createContext, use } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	PolicyGroupEditorForm,
	type PolicyLookup,
} from "@/components/admin/PolicyGroupEditorForm";
import {
	getDefaultPolicyGroupForm,
	type PolicyGroupFormData,
} from "@/components/admin/policyGroupEditorShared";

const mockState = vi.hoisted(() => ({
	addRule: vi.fn(),
	fieldChange: vi.fn(),
	moveRule: vi.fn(),
	openSimulation: vi.fn(),
	refresh: vi.fn(),
	removeRule: vi.fn(),
	reorder: vi.fn(),
	ruleFieldChange: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("@/components/ui/badge", () => ({
	Badge: ({ children }: { children: React.ReactNode }) => (
		<span>{children}</span>
	),
}));
vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <i>{name}</i>,
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
vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({ children, open }: { children: React.ReactNode; open: boolean }) =>
		open ? <div>{children}</div> : null,
	DialogContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogDescription: ({ children }: { children: React.ReactNode }) => (
		<p>{children}</p>
	),
	DialogHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
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
		children?: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		type?: "button" | "submit";
		[key: string]: unknown;
	}) => (
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
vi.mock("@/components/ui/input", () => ({
	Input: ({
		onChange,
		value,
		...props
	}: {
		onChange?: (event: { target: { value: string } }) => void;
		value?: string;
		[key: string]: unknown;
	}) => (
		<input
			value={value ?? ""}
			onChange={(event) =>
				onChange?.({ target: { value: event.target.value } })
			}
			{...props}
		/>
	),
}));
vi.mock("@/components/ui/item-checkbox", () => ({
	ItemCheckbox: ({
		checked,
		onChange,
	}: {
		checked: boolean;
		onChange: () => void;
	}) => (
		<input
			type="checkbox"
			checked={checked}
			onChange={onChange}
			aria-label={`checkbox:${checked}`}
		/>
	),
}));
vi.mock("@/components/ui/switch", () => ({
	Switch: ({
		checked,
		onCheckedChange,
		id,
	}: {
		checked: boolean;
		onCheckedChange?: (value: boolean) => void;
		id?: string;
	}) => (
		<button
			type="button"
			aria-label={`switch:${id ?? "toggle"}:${checked}`}
			onClick={() => onCheckedChange?.(!checked)}
		/>
	),
}));
vi.mock("@/components/ui/select", () => {
	type Option = { label: string; value: string };
	const Context = createContext<{
		items?: Option[];
		onOpenChange?: (open: boolean) => void;
		onValueChange?: (value: string) => void;
		value?: string;
	}>({});
	return {
		Select: ({
			children,
			items,
			onOpenChange,
			onValueChange,
			value,
		}: {
			children: React.ReactNode;
			items?: Option[];
			onOpenChange?: (open: boolean) => void;
			onValueChange?: (value: string) => void;
			value?: string;
		}) => (
			<Context.Provider value={{ items, onOpenChange, onValueChange, value }}>
				<div>{children}</div>
			</Context.Provider>
		),
		SelectContent: ({
			children,
			onScroll,
		}: {
			children: React.ReactNode;
			onScroll?: (event: React.UIEvent<HTMLDivElement>) => void;
		}) => (
			<div data-testid="select-content" onScroll={onScroll}>
				{children}
			</div>
		),
		SelectGroup: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
		SelectLabel: ({ children }: { children: React.ReactNode }) => (
			<span>{children}</span>
		),
		SelectSeparator: () => <hr />,
		SelectItem: ({
			children,
			value,
		}: {
			children: React.ReactNode;
			value: string;
		}) => {
			const context = use(Context);
			return (
				<button
					type="button"
					aria-label={`select-item:${value}`}
					onClick={() => context.onValueChange?.(value)}
				>
					{children}
				</button>
			);
		},
		SelectTrigger: ({
			children,
			...props
		}: {
			children: React.ReactNode;
			[key: string]: unknown;
		}) => {
			const context = use(Context);
			return (
				<button
					type="button"
					onClick={() => context.onOpenChange?.(true)}
					{...props}
				>
					{children}
				</button>
			);
		},
		SelectValue: ({
			children,
			placeholder,
		}: {
			children?: React.ReactNode;
			placeholder?: string;
		}) => {
			const context = use(Context);
			const selected = context.items?.find(
				(item) => item.value === context.value,
			)?.label;
			return <span>{selected ?? children ?? placeholder}</span>;
		},
	};
});

function createForm(
	overrides: Partial<PolicyGroupFormData> = {},
): PolicyGroupFormData {
	return {
		...getDefaultPolicyGroupForm([{ id: 1 }]),
		name: "Group",
		...overrides,
	};
}

function createProps(
	overrides: Partial<React.ComponentProps<typeof PolicyGroupEditorForm>> = {},
) {
	return {
		mode: "create" as const,
		form: createForm(),
		formError: null,
		policies: [
			{ id: 1, name: "Primary", connector_id: "local" },
		] as PolicyLookup[],
		policiesLoading: false,
		onAddRule: mockState.addRule,
		onFieldChange: mockState.fieldChange,
		onMoveRule: mockState.moveRule,
		onOpenSimulation: mockState.openSimulation,
		onRefreshPolicies: mockState.refresh,
		onRemoveRule: mockState.removeRule,
		onReorderRule: mockState.reorder,
		onRuleFieldChange: mockState.ruleFieldChange,
		...overrides,
	};
}

describe("PolicyGroupEditorForm", () => {
	beforeEach(() => {
		for (const mock of Object.values(mockState)) mock.mockReset();
	});

	it("renders empty policy state and updates basic fields and categories", () => {
		render(
			<PolicyGroupEditorForm
				{...createProps({ policies: [], formError: "bad form" })}
			/>,
		);
		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Renamed" },
		});
		const [selectAll] = screen.getAllByRole("button", {
			name: "policy_group_category_select_all",
		});
		if (!selectAll) throw new Error("select-all button missing");
		fireEvent.click(selectAll);
		const [imageCategory] = screen.getAllByRole("button", {
			name: "policy_group_category_image",
		});
		if (!imageCategory) throw new Error("image category button missing");
		fireEvent.click(imageCategory);
		fireEvent.click(screen.getAllByRole("checkbox")[0]);
		const [, deniedCategory] = screen.getAllByRole("button", {
			name: "policy_group_category_image",
		});
		if (deniedCategory) fireEvent.click(deniedCategory);
		fireEvent.change(screen.getByLabelText("policy_group_allowed_extensions"), {
			target: { value: "jpg, png" },
		});
		fireEvent.change(screen.getByLabelText("policy_group_denied_extensions"), {
			target: { value: "exe" },
		});
		expect(mockState.fieldChange).toHaveBeenCalledWith("name", "Renamed");
		expect(mockState.fieldChange).not.toHaveBeenCalledWith(
			"isDefault",
			expect.anything(),
		);
		expect(
			screen.getByText("policy_group_no_policies_available"),
		).toBeInTheDocument();
		expect(screen.getByText("bad form")).toBeInTheDocument();
		fireEvent.click(
			screen.getByRole("button", {
				name: "policy_group_categories_help_title",
			}),
		);
		expect(
			screen.getByText("policy_group_categories_help_intro"),
		).toBeInTheDocument();
	});

	it("filters policies without a redundant pagination request", async () => {
		render(<PolicyGroupEditorForm {...createProps()} />);
		fireEvent.change(screen.getByLabelText("policy_group_policy_search"), {
			target: { value: "missing" },
		});
		await waitFor(() =>
			expect(
				screen.getByRole("button", { name: "select-item:1" }),
			).toBeInTheDocument(),
		);
		expect(mockState.refresh).not.toHaveBeenCalled();
	});

	it("refreshes an empty policy selector and handles admission controls", () => {
		const initial = createForm({
			admission: {
				allowed_extensions: [],
				denied_extensions: [],
				accept_extensionless: true,
				allowed_categories: [
					"image",
					"video",
					"audio",
					"document",
					"spreadsheet",
					"presentation",
					"archive",
					"code",
					"other",
				],
				denied_categories: [],
				max_file_size: 0,
			},
		});
		const initialRule = initial.items[0];
		if (!initialRule?.targets[0]) throw new Error("initial rule missing");
		const form = {
			...initial,
			items: [
				{
					...initialRule,
					targets: [{ ...initialRule.targets[0], policyId: "" }],
				},
			],
		};
		render(<PolicyGroupEditorForm {...createProps({ policies: [], form })} />);
		const [policyTrigger] = screen.getAllByRole("button", {
			name: "select_policy",
		});
		if (!policyTrigger) throw new Error("policy trigger missing");
		fireEvent.click(policyTrigger);
		fireEvent.change(
			screen.getByLabelText("policy_group_admission_max_size_mb"),
			{ target: { value: "5" } },
		);
		fireEvent.click(screen.getByRole("button", { name: "select_policy" }));
		fireEvent.click(
			screen.getAllByRole("button", {
				name: "policy_group_category_deselect_all",
			})[0] ??
				screen.getAllByRole("button", {
					name: "policy_group_category_select_all",
				})[0],
		);
		expect(mockState.refresh).toHaveBeenCalled();
		expect(mockState.fieldChange).toHaveBeenCalledWith(
			"admission",
			expect.objectContaining({ max_file_size: 5 * 1024 * 1024 }),
		);
	});

	it("supports target removal and drag interactions", () => {
		const form = createForm();
		const first = form.items[0];
		if (!first) throw new Error("first rule missing");
		const target = first.targets[0];
		if (!target) throw new Error("target missing");
		const twoTargetRule = {
			...first,
			targets: [target, { ...target, key: "target-2", policyId: "2" }],
		};
		const secondRule = { ...first, key: "rule-2", name: "Second" };
		const rect = {
			top: 0,
			left: 0,
			right: 100,
			bottom: 100,
			width: 100,
			height: 100,
			x: 0,
			y: 0,
			toJSON: () => ({}),
		};
		vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(
			rect as DOMRect,
		);
		render(
			<PolicyGroupEditorForm
				{...createProps({
					form: { ...form, items: [twoTargetRule, secondRule] },
				})}
			/>,
		);
		const [weight] = screen.getAllByLabelText("policy_group_target_weight");
		if (!weight) throw new Error("weight input missing");
		fireEvent.change(weight, { target: { value: "40" } });
		const [removeTarget] = screen.getAllByRole("button", {
			name: "policy_group_target_remove",
		});
		if (!removeTarget) throw new Error("target remove button missing");
		fireEvent.click(removeTarget);
		const [handle] = screen.getAllByRole("button", {
			name: "policy_group_rule_drag_handle",
		});
		if (!handle) throw new Error("drag handle missing");
		fireEvent.dragStart(handle, { dataTransfer: { effectAllowed: "" } });
		const card = document.querySelector('[data-rule-key="rule-2"]');
		if (!card) throw new Error("second rule card missing");
		const cardContent = card.firstElementChild;
		if (!cardContent) throw new Error("second rule content missing");
		fireEvent.dragOver(cardContent, { clientY: 100 });
		const secondHandle = screen.getAllByRole("button", {
			name: "policy_group_rule_drag_handle",
		})[1];
		if (secondHandle) {
			fireEvent.dragStart(secondHandle, {
				dataTransfer: { effectAllowed: "" },
			});
			fireEvent.dragOver(
				document.querySelector('[data-rule-key="rule-1"]')?.firstElementChild ??
					cardContent,
				{ clientY: 0 },
			);
		}
		expect(mockState.ruleFieldChange).toHaveBeenCalledWith(
			expect.any(String),
			"targets",
			expect.any(Array),
		);
	});

	it("wires rule, target, matcher and selection controls", () => {
		const form = createForm();
		render(
			<PolicyGroupEditorForm
				{...createProps({
					mode: "edit",
					onOpenSimulation: mockState.openSimulation,
					form,
				})}
			/>,
		);
		fireEvent.click(
			screen.getByRole("button", { name: /policy_group_simulator_open/ }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: /policy_group_add_rule/ }),
		);
		fireEvent.change(screen.getByLabelText("policy_group_rule_name"), {
			target: { value: "Images" },
		});
		fireEvent.change(screen.getByLabelText("policy_group_min_size_mb"), {
			target: { value: "2" },
		});
		fireEvent.change(screen.getByLabelText("policy_group_target_weight"), {
			target: { value: "70" },
		});
		fireEvent.click(
			screen.getAllByRole("button", { name: /switch:toggle:true/ })[1] ??
				screen.getAllByRole("button", { name: /switch:toggle:true/ })[0],
		);
		fireEvent.click(
			screen.getByRole("button", { name: /policy_group_add_target/ }),
		);
		fireEvent.click(screen.getByRole("button", { name: "select-item:1" }));
		const [targetSwitch] = screen.getAllByRole("button", {
			name: /switch:toggle:true/,
		});
		if (!targetSwitch) throw new Error("target switch missing");
		fireEvent.click(targetSwitch);
		fireEvent.click(
			screen.getByRole("button", { name: "select-item:weighted_random" }),
		);
		fireEvent.click(screen.getByRole("button", { name: "select-item:reject" }));
		fireEvent.click(
			screen.getByRole("button", { name: "select-item:force_server_stream" }),
		);
		expect(mockState.openSimulation).toHaveBeenCalledTimes(1);
		expect(mockState.addRule).toHaveBeenCalledTimes(1);
		expect(mockState.ruleFieldChange).toHaveBeenCalledWith(
			form.items[0]?.key,
			"name",
			"Images",
		);
		expect(mockState.ruleFieldChange).toHaveBeenCalledWith(
			form.items[0]?.key,
			"minFileSizeMb",
			"2",
		);
		expect(mockState.ruleFieldChange).toHaveBeenCalledWith(
			form.items[0]?.key,
			"selectionMode",
			"weighted_random",
		);
	});

	it("requires a second click before removing a rule and supports move buttons", async () => {
		const base = createForm();
		const first = base.items[0];
		if (!first) throw new Error("first rule missing");
		const second = { ...first, key: "second-rule", name: "Rule 2" };
		render(
			<PolicyGroupEditorForm
				{...createProps({ form: { ...base, items: [first, second] } })}
			/>,
		);
		const [remove] = screen.getAllByRole("button", {
			name: "policy_group_remove_rule",
		});
		if (!remove) throw new Error("remove button missing");
		fireEvent.click(remove);
		await waitFor(() =>
			expect(
				screen.getByRole("button", {
					name: "policy_group_remove_rule_confirm",
				}),
			).toBeInTheDocument(),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "policy_group_remove_rule_confirm" }),
		);
		await waitFor(() =>
			expect(mockState.removeRule).toHaveBeenCalledWith(expect.any(String)),
		);
	});
});
