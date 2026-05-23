import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createContext, use } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PolicyGroupDialog } from "@/components/admin/PolicyGroupDialog";

const mockState = vi.hoisted(() => ({
	onAddRule: vi.fn(),
	onFieldChange: vi.fn(),
	onLoadMorePolicies: vi.fn(),
	onOpenChange: vi.fn(),
	onRefreshPolicies: vi.fn(),
	onRemoveRule: vi.fn(),
	onRuleFieldChange: vi.fn(),
	onSubmit: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
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
		disabled,
		onClick,
		type,
		...props
	}: {
		children: React.ReactNode;
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
		className,
		id,
		onChange,
		placeholder,
		type,
		value,
		...props
	}: {
		className?: string;
		id?: string;
		onChange?: (event: { target: { value: string } }) => void;
		placeholder?: string;
		type?: string;
		value?: string;
		[key: string]: unknown;
	}) => (
		<input
			className={className}
			id={id}
			onChange={(event) =>
				onChange?.({ target: { value: event.target.value } })
			}
			placeholder={placeholder}
			type={type}
			value={value}
			{...props}
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
		disabled?: boolean;
		items?: SelectOption[];
		onOpenChange?: (open: boolean) => void;
		onValueChange?: (value: string) => void;
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
				<div>{children}</div>
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
		SelectLabel: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
		SelectSeparator: () => <hr />,
		SelectTrigger: ({ children }: { children: React.ReactNode }) => {
			const context = use(SelectContext);

			return (
				<button
					type="button"
					data-testid="select-trigger"
					disabled={context.disabled}
					onClick={() => context.onOpenChange?.(true)}
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
			const context = use(SelectContext);
			const selectedLabel = context.items?.find(
				(item) => item.value === context.value,
			)?.label;

			return <span>{selectedLabel ?? children ?? placeholder ?? ""}</span>;
		},
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

function createForm(
	overrides: Partial<
		React.ComponentProps<typeof PolicyGroupDialog>["form"]
	> = {},
) {
	return {
		name: "Core Group",
		description: "Main routing rules",
		isEnabled: true,
		isDefault: false,
		items: [
			{
				key: "rule-1",
				policyId: "1",
				priority: "1",
				minFileSizeMb: "",
				maxFileSizeMb: "",
			},
		],
		...overrides,
	};
}

function createProps(
	overrides: Partial<React.ComponentProps<typeof PolicyGroupDialog>> = {},
): React.ComponentProps<typeof PolicyGroupDialog> {
	return {
		open: true,
		mode: "create",
		form: createForm(),
		formError: null,
		hasMorePolicies: false,
		policies: [],
		policiesLoading: false,
		policiesLoadingMore: false,
		policiesTotal: 0,
		submitting: false,
		onAddRule: mockState.onAddRule,
		onFieldChange: mockState.onFieldChange,
		onLoadMorePolicies: mockState.onLoadMorePolicies,
		onOpenChange: mockState.onOpenChange,
		onRefreshPolicies: mockState.onRefreshPolicies,
		onRemoveRule: mockState.onRemoveRule,
		onRuleFieldChange: mockState.onRuleFieldChange,
		onSubmit: mockState.onSubmit,
		...overrides,
	};
}

describe("PolicyGroupDialog", () => {
	beforeEach(() => {
		mockState.onAddRule.mockReset();
		mockState.onFieldChange.mockReset();
		mockState.onLoadMorePolicies.mockReset();
		mockState.onOpenChange.mockReset();
		mockState.onRefreshPolicies.mockReset();
		mockState.onRemoveRule.mockReset();
		mockState.onRuleFieldChange.mockReset();
		mockState.onSubmit.mockReset();
	});

	it("resets the search and refreshes policies when an empty select is opened", () => {
		render(<PolicyGroupDialog {...createProps()} />);

		fireEvent.change(screen.getByLabelText("policy_group_policy_search"), {
			target: { value: "local" },
		});
		expect(screen.getByDisplayValue("local")).toBeInTheDocument();

		fireEvent.click(screen.getByTestId("select-trigger"));
		expect(mockState.onRefreshPolicies).toHaveBeenCalledTimes(1);

		fireEvent.click(screen.getByRole("button", { name: "core:cancel" }));

		expect(mockState.onOpenChange).toHaveBeenCalledWith(false);
		expect(screen.getByLabelText("policy_group_policy_search")).toHaveValue("");
	});

	it("keeps the selected policy visible while filtering and loads more on empty matches", async () => {
		render(
			<PolicyGroupDialog
				{...createProps({
					hasMorePolicies: true,
					policies: [
						{ driver_type: "local", id: 1, name: "Alpha Policy" },
						{ driver_type: "s3", id: 2, name: "Beta Policy" },
					],
					policiesTotal: 2,
				})}
			/>,
		);

		fireEvent.change(screen.getByLabelText("policy_group_policy_search"), {
			target: { value: "beta" },
		});
		expect(
			screen.getByRole("button", { name: "select-item:1" }),
		).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "select-item:2" }),
		).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("policy_group_policy_search"), {
			target: { value: "no-match" },
		});

		await waitFor(() => {
			expect(mockState.onLoadMorePolicies).toHaveBeenCalledTimes(1);
		});
		expect(
			screen.getByRole("button", { name: "select-item:1" }),
		).toBeInTheDocument();
	});

	it("wires the main form actions back to the parent callbacks", () => {
		render(
			<PolicyGroupDialog
				{...createProps({
					form: createForm({
						items: [
							{
								key: "rule-1",
								policyId: "1",
								priority: "1",
								minFileSizeMb: "",
								maxFileSizeMb: "",
							},
							{
								key: "rule-2",
								policyId: "2",
								priority: "2",
								minFileSizeMb: "10",
								maxFileSizeMb: "100",
							},
						],
					}),
					policies: [
						{ driver_type: "local", id: 1, name: "Alpha Policy" },
						{ driver_type: "s3", id: 2, name: "Beta Policy" },
					],
					policiesTotal: 2,
				})}
			/>,
		);

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Updated Group" },
		});
		fireEvent.click(
			screen.getByRole("button", {
				name: "switch:policy-group-enabled:true",
			}),
		);
		fireEvent.click(
			screen.getByRole("button", {
				name: /policy_group_add_rule/,
			}),
		);
		fireEvent.change(screen.getAllByLabelText("policy_group_priority")[0], {
			target: { value: "7" },
		});
		fireEvent.click(
			screen.getAllByRole("button", {
				name: "policy_group_remove_rule",
			})[0],
		);
		fireEvent.click(screen.getByRole("button", { name: /core:create/ }));

		expect(mockState.onFieldChange).toHaveBeenCalledWith(
			"name",
			"Updated Group",
		);
		expect(mockState.onFieldChange).toHaveBeenCalledWith("isEnabled", false);
		expect(mockState.onAddRule).toHaveBeenCalledTimes(1);
		expect(mockState.onRuleFieldChange).toHaveBeenCalledWith(
			"rule-1",
			"priority",
			"7",
		);
		expect(mockState.onRemoveRule).toHaveBeenCalledWith("rule-1");
		expect(mockState.onSubmit).toHaveBeenCalledTimes(1);
	});
});
