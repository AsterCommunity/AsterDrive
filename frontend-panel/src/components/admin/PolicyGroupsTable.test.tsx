import { fireEvent, render, screen } from "@testing-library/react";
import { cloneElement, createContext, isValidElement, use } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PolicyGroupsTable } from "@/components/admin/PolicyGroupsTable";

const mockState = vi.hoisted(() => ({
	onNextPage: vi.fn(),
	onOpenEdit: vi.fn(),
	onOpenMigration: vi.fn(),
	onPageSizeChange: vi.fn(),
	onPreviousPage: vi.fn(),
	onRequestDelete: vi.fn(),
	onOpenSimulation: vi.fn(),
	onSortChange: vi.fn(),
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
}));

vi.mock("@/components/common/AdminTableList", () => ({
	AdminTableList: ({
		emptyDescription,
		emptyTitle,
		headerRow,
		items,
		loading,
		renderRow,
	}: {
		emptyDescription?: string;
		emptyTitle: string;
		headerRow: React.ReactNode;
		items: unknown[];
		loading: boolean;
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
		variant,
		...props
	}: {
		children?: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		type?: "button" | "submit";
		variant?: string;
		[key: string]: unknown;
	}) => (
		<button
			type={type ?? "button"}
			data-variant={variant}
			disabled={disabled}
			onClick={onClick}
			{...props}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/select", () => {
	type SelectOption = {
		label: string;
		value: string;
	};

	const SelectContext = createContext<{
		items?: SelectOption[];
		onValueChange?: (value: string | null) => void;
		value?: string;
	}>({});

	return {
		Select: ({
			children,
			items,
			onValueChange,
			value,
		}: {
			children: React.ReactNode;
			items?: SelectOption[];
			onValueChange?: (value: string | null) => void;
			value?: string;
		}) => (
			<SelectContext.Provider value={{ items, onValueChange, value }}>
				<div>{children}</div>
			</SelectContext.Provider>
		),
		SelectContent: () => null,
		SelectItem: () => null,
		SelectTrigger: ({
			"aria-label": ariaLabel,
		}: {
			"aria-label"?: string;
			[key: string]: unknown;
		}) => {
			const context = use(SelectContext);

			return (
				<select
					aria-label={ariaLabel ?? "page-size"}
					value={context.value}
					onChange={(event) => context.onValueChange?.(event.target.value)}
				>
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

vi.mock("@/components/ui/table", () => ({
	TableCell: ({
		children,
		className,
		onClick,
		onKeyDown,
	}: {
		children: React.ReactNode;
		className?: string;
		onClick?: (event: React.MouseEvent<HTMLTableCellElement>) => void;
		onKeyDown?: (event: React.KeyboardEvent<HTMLTableCellElement>) => void;
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
		onKeyDown?: (event: React.KeyboardEvent<HTMLTableRowElement>) => void;
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

vi.mock("@/lib/format", () => ({
	formatBytes: (value: number) => `${value} B`,
	formatDateAbsolute: (value: string) => `formatted:${value}`,
}));

function createGroup(overrides: Record<string, unknown> = {}) {
	return {
		created_at: "2026-04-01T00:00:00Z",
		description: "Main traffic",
		id: 1,
		is_default: false,
		is_enabled: true,
		rules: [
			{
				id: 11,
				name: "Rule 1",
				description: "",
				priority: 1,
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
						id: 21,
						policy_id: 7,
						weight: 100,
						is_enabled: true,
						accepting_new_writes: true,
						stable_order: 1,
						policy: { id: 7, name: "Alpha Policy", connector_id: "local" },
					},
				],
			},
		],
		name: "Engineering",
		updated_at: "2026-04-02T00:00:00Z",
		...overrides,
	};
}

function createProps(
	overrides: Partial<React.ComponentProps<typeof PolicyGroupsTable>> = {},
): React.ComponentProps<typeof PolicyGroupsTable> {
	return {
		currentPage: 2,
		groups: [createGroup()],
		loading: false,
		nextPageDisabled: false,
		pageSize: 20,
		pageSizeOptions: [
			{ label: "20", value: "20" },
			{ label: "50", value: "50" },
		],
		prevPageDisabled: false,
		total: 2,
		totalPages: 4,
		onNextPage: mockState.onNextPage,
		onOpenEdit: mockState.onOpenEdit,
		onOpenMigration: mockState.onOpenMigration,
		onOpenSimulation: mockState.onOpenSimulation,
		onPageSizeChange: mockState.onPageSizeChange,
		onPreviousPage: mockState.onPreviousPage,
		onRequestDelete: mockState.onRequestDelete,
		onSortChange: mockState.onSortChange,
		...overrides,
	};
}

describe("PolicyGroupsTable", () => {
	beforeEach(() => {
		mockState.onNextPage.mockReset();
		mockState.onOpenEdit.mockReset();
		mockState.onOpenMigration.mockReset();
		mockState.onPageSizeChange.mockReset();
		mockState.onPreviousPage.mockReset();
		mockState.onRequestDelete.mockReset();
		mockState.onOpenSimulation.mockReset();
		mockState.onSortChange.mockReset();
	});

	it("opens edit from rows and action buttons while keeping destructive actions scoped", () => {
		const primaryGroup = createGroup();
		const defaultGroup = createGroup({
			description: "",
			id: 2,
			is_default: true,
			rules: [
				{
					id: 21,
					name: "Rule 2",
					description: "",
					priority: 2,
					is_enabled: true,
					matcher: {
						min_file_size: 100,
						max_file_size: 200,
						extensions: [],
						compound_extensions: [],
						extensionless: null,
						categories: [],
					},
					selection_mode: "first_available",
					unavailable_behavior: "next_rule",
					targets: [
						{
							id: 22,
							policy_id: 9,
							weight: 100,
							is_enabled: true,
							accepting_new_writes: true,
							stable_order: 1,
							policy: { id: 9, name: "Beta Policy", connector_id: "local" },
						},
					],
				},
			],
			name: "Default Group",
		});

		render(
			<PolicyGroupsTable
				{...createProps({
					groups: [primaryGroup, defaultGroup],
				})}
			/>,
		);

		fireEvent.click(screen.getByText("Engineering"));
		const defaultGroupRow = screen.getByText("Default Group").closest("tr");
		if (!defaultGroupRow) {
			throw new Error("Default group row not found");
		}
		fireEvent.keyDown(defaultGroupRow, {
			key: "Enter",
		});
		fireEvent.click(
			screen.getAllByRole("button", {
				name: "migrate_policy_group_assignments",
			})[0],
		);
		fireEvent.click(
			screen.getAllByRole("button", {
				name: "delete_policy_group",
			})[0],
		);

		const deleteButtons = screen.getAllByRole("button", {
			name: "delete_policy_group",
		});

		expect(mockState.onOpenEdit).toHaveBeenNthCalledWith(1, primaryGroup);
		expect(mockState.onOpenEdit).toHaveBeenNthCalledWith(2, defaultGroup);
		expect(mockState.onOpenMigration).toHaveBeenCalledWith(primaryGroup);
		expect(mockState.onRequestDelete).toHaveBeenCalledWith(1);
		expect(deleteButtons[1]).toBeEnabled();
	});

	it("keeps disabled action tooltips on fixed-size triggers", () => {
		render(
			<PolicyGroupsTable
				{...createProps({
					groups: [
						createGroup({
							id: 2,
							is_default: true,
							name: "Default Group",
						}),
					],
					total: 1,
				})}
			/>,
		);

		const migrationButton = screen.getByRole("button", {
			name: "migrate_policy_group_assignments",
		});
		const deleteButton = screen.getByRole("button", {
			name: "delete_policy_group",
		});

		expect(migrationButton).toBeDisabled();
		expect(deleteButton).toBeEnabled();
		expect(migrationButton.parentElement).toHaveClass(
			"inline-flex",
			"size-8",
			"shrink-0",
		);
		expect(deleteButton.parentElement).toHaveClass(
			"inline-flex",
			"size-8",
			"shrink-0",
		);
		expect(
			screen.getByText("policy_group_migration_unavailable"),
		).toBeInTheDocument();
		expect(
			screen.queryByText("policy_group_delete_default_blocked"),
		).toBeNull();
	});

	it("updates pagination state through the footer controls", () => {
		render(<PolicyGroupsTable {...createProps()} />);

		fireEvent.change(screen.getByRole("combobox"), {
			target: { value: "50" },
		});
		fireEvent.click(screen.getByRole("button", { name: "CaretLeft" }));
		fireEvent.click(screen.getByRole("button", { name: "CaretRight" }));

		expect(mockState.onPageSizeChange).toHaveBeenCalledWith("50");
		expect(mockState.onPreviousPage).toHaveBeenCalledTimes(1);
		expect(mockState.onNextPage).toHaveBeenCalledTimes(1);
		expect(screen.getByText("entries_page")).toBeInTheDocument();
	});

	it("formats all rule ranges and exposes simulation and sort actions", () => {
		const base = createGroup();
		const rules = [
			base.rules[0],
			{
				...base.rules[0],
				id: 12,
				priority: 2,
				matcher: {
					...base.rules[0].matcher,
					min_file_size: 1,
					max_file_size: 0,
				},
			},
			{
				...base.rules[0],
				id: 13,
				priority: 3,
				matcher: {
					...base.rules[0].matcher,
					min_file_size: 0,
					max_file_size: 2,
				},
			},
			{
				...base.rules[0],
				id: 14,
				priority: 4,
				matcher: {
					...base.rules[0].matcher,
					min_file_size: 1,
					max_file_size: 2,
				},
			},
		];
		const groups = rules.map((rule, index) =>
			createGroup({ id: index + 1, name: `Range ${index}`, rules: [rule] }),
		);
		groups.push(createGroup({ id: 9, rules }));
		render(<PolicyGroupsTable {...createProps({ groups })} />);
		expect(
			screen.getAllByText("policy_group_range_any").length,
		).toBeGreaterThan(0);
		expect(
			screen.getAllByText("policy_group_range_min").length,
		).toBeGreaterThan(0);
		expect(
			screen.getAllByText("policy_group_range_max").length,
		).toBeGreaterThan(0);
		expect(
			screen.getAllByText("policy_group_range_between").length,
		).toBeGreaterThan(0);
		expect(screen.getByText("+2 more")).toBeInTheDocument();
		const [simulate] = screen.getAllByRole("button", {
			name: "policy_group_simulator_open",
		});
		if (!simulate) throw new Error("simulation button missing");
		fireEvent.click(simulate);
		expect(mockState.onOpenSimulation).toHaveBeenCalledWith(
			expect.objectContaining({ id: 1 }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "core:nameSortDescending" }),
		);
		expect(mockState.onSortChange).toHaveBeenCalled();
	});
});
