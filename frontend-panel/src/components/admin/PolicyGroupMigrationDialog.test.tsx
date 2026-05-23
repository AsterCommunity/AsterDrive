import { fireEvent, render, screen } from "@testing-library/react";
import { createContext, use } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PolicyGroupMigrationDialog } from "@/components/admin/PolicyGroupMigrationDialog";

const mockState = vi.hoisted(() => ({
	onConfirm: vi.fn(),
	onOpenChange: vi.fn(),
	onTargetGroupChange: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
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
	DialogContent: ({ children }: { children: React.ReactNode }) => (
		<div role="dialog">{children}</div>
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
		onValueChange?: (value: string) => void;
		value?: string;
	}>({});

	return {
		Select: ({
			children,
			disabled,
			items,
			onValueChange,
			value,
		}: {
			children: React.ReactNode;
			disabled?: boolean;
			items?: SelectOption[];
			onValueChange?: (value: string) => void;
			value?: string;
		}) => (
			<SelectContext.Provider value={{ disabled, items, onValueChange, value }}>
				<div>{children}</div>
			</SelectContext.Provider>
		),
		SelectContent: () => null,
		SelectItem: () => null,
		SelectTrigger: () => {
			const context = use(SelectContext);

			return (
				<select
					aria-label="policy_group_migration_target"
					disabled={context.disabled}
					value={context.value}
					onChange={(event) => context.onValueChange?.(event.target.value)}
				>
					<option value="">policy_group_migration_target_placeholder</option>
					{context.items?.map((item) => (
						<option key={item.value} value={item.value}>
							{item.label}
						</option>
					))}
				</select>
			);
		},
		SelectValue: () => null,
	};
});

function createProps(
	overrides: Partial<
		React.ComponentProps<typeof PolicyGroupMigrationDialog>
	> = {},
): React.ComponentProps<typeof PolicyGroupMigrationDialog> {
	return {
		open: true,
		error: null,
		loading: false,
		sourceGroupName: "Source Group",
		submitting: false,
		targetGroupId: "",
		targetOptions: [
			{ label: "Target Alpha", value: "1" },
			{ label: "Target Beta", value: "2" },
		],
		onConfirm: mockState.onConfirm,
		onOpenChange: mockState.onOpenChange,
		onTargetGroupChange: mockState.onTargetGroupChange,
		...overrides,
	};
}

describe("PolicyGroupMigrationDialog", () => {
	beforeEach(() => {
		mockState.onConfirm.mockReset();
		mockState.onOpenChange.mockReset();
		mockState.onTargetGroupChange.mockReset();
	});

	it("shows the source group, updates the selected target, and wires the action buttons", () => {
		render(<PolicyGroupMigrationDialog {...createProps()} />);

		expect(screen.getByText("Source Group")).toBeInTheDocument();

		fireEvent.change(screen.getByRole("combobox"), {
			target: { value: "2" },
		});
		fireEvent.click(
			screen.getByRole("button", {
				name: /policy_group_migration_confirm/,
			}),
		);
		fireEvent.click(screen.getByRole("button", { name: "core:cancel" }));

		expect(mockState.onTargetGroupChange).toHaveBeenCalledWith("2");
		expect(mockState.onConfirm).toHaveBeenCalledTimes(1);
		expect(mockState.onOpenChange).toHaveBeenCalledWith(false);
	});

	it("disables selection and confirmation while loading or when no targets are available", () => {
		render(
			<PolicyGroupMigrationDialog
				{...createProps({
					error: "No migration target available",
					loading: true,
					targetOptions: [],
				})}
			/>,
		);

		expect(screen.getByRole("combobox")).toBeDisabled();
		expect(
			screen.getByRole("button", {
				name: /policy_group_migration_confirm/,
			}),
		).toBeDisabled();
		expect(
			screen.getByText("No migration target available"),
		).toBeInTheDocument();
	});
});
