import { fireEvent, render, screen } from "@testing-library/react";
import type { ComponentProps, ReactNode, SetStateAction } from "react";
import { describe, expect, it, vi } from "vitest";
import { TeamManageMembersSection } from "@/components/settings/team-manage-detail/TeamManageMembersSection";
import type { TeamInfo, TeamMemberInfo, UserSummary } from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) =>
			options ? `${key}:${Object.values(options).join("/")}` : key,
	}),
}));

vi.mock("@/components/common/EmptyState", () => ({
	EmptyState: ({
		title,
		description,
	}: {
		title: string;
		description?: string;
	}) => (
		<div>
			<h3>{title}</h3>
			<p>{description}</p>
		</div>
	),
}));

vi.mock("@/components/common/SkeletonTable", () => ({
	SkeletonTable: ({ columns, rows }: { columns: number; rows: number }) => (
		<div>{`skeleton:${columns}:${rows}`}</div>
	),
}));

vi.mock("@/components/common/UserIdentity", () => ({
	UserIdentity: ({ user }: { user: UserSummary }) => (
		<span>{user.username}</span>
	),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({ children }: { children: ReactNode }) => <span>{children}</span>,
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
		type,
	}: {
		children: ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		type?: "button" | "submit";
	}) => (
		<button type={type ?? "button"} disabled={disabled} onClick={onClick}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({
		disabled,
		onChange,
		placeholder,
		value,
		...props
	}: {
		disabled?: boolean;
		onChange?: (event: { target: { value: string } }) => void;
		placeholder?: string;
		value?: string;
	}) => (
		<input
			{...props}
			disabled={disabled}
			placeholder={placeholder}
			value={value}
			onChange={(event) =>
				onChange?.({ target: { value: event.target.value } })
			}
		/>
	),
}));

vi.mock("@/components/ui/label", () => ({
	Label: ({ children, htmlFor }: { children: ReactNode; htmlFor?: string }) => (
		<label htmlFor={htmlFor}>{children}</label>
	),
}));

vi.mock("@/components/ui/select", () => ({
	Select: ({
		onValueChange,
		value,
	}: {
		onValueChange?: (value: string) => void;
		value: string;
	}) => (
		<select
			aria-label={`select:${value}`}
			value={value}
			onChange={(event) => onValueChange?.(event.target.value)}
		>
			<option value="__all__">__all__</option>
			<option value="owner">owner</option>
			<option value="admin">admin</option>
			<option value="member">member</option>
			<option value="active">active</option>
			<option value="disabled">disabled</option>
		</select>
	),
	SelectContent: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	SelectItem: ({ children }: { children: ReactNode }) => <div>{children}</div>,
	SelectTrigger: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	SelectValue: () => null,
}));

vi.mock("@/components/ui/table", () => ({
	Table: ({ children }: ComponentProps<"table">) => <table>{children}</table>,
	TableBody: ({ children }: ComponentProps<"tbody">) => (
		<tbody>{children}</tbody>
	),
	TableCell: ({ children }: ComponentProps<"td">) => <td>{children}</td>,
	TableHead: ({ children }: ComponentProps<"th">) => <th>{children}</th>,
	TableHeader: ({ children }: { children: ReactNode }) => (
		<thead>{children}</thead>
	),
	TableRow: ({ children }: ComponentProps<"tr">) => <tr>{children}</tr>,
}));

vi.mock("@/lib/format", () => ({
	formatDateShort: (value: string) => `date:${value}`,
}));

const user = (id: number, username: string): UserSummary => ({
	id,
	profile: {
		avatar: {
			source: "none",
			url_1024: null,
			url_512: null,
			version: 0,
		},
		display_name: "",
	},
	username,
});

const member = (overrides: Partial<TeamMemberInfo> = {}): TeamMemberInfo => ({
	created_at: "2026-05-01T00:00:00Z",
	email: "member@example.com",
	id: 1,
	role: "member",
	status: "active",
	team_id: 4,
	user: user(2, "member"),
	user_id: 2,
	...overrides,
});

const team = (): TeamInfo => ({
	archived_at: null,
	created_at: "2026-05-01T00:00:00Z",
	created_by: user(1, "owner"),
	description: "",
	id: 4,
	member_count: 12,
	my_role: "owner",
	name: "Product",
	policy_group_id: null,
	storage_quota: 0,
	storage_used: 0,
	updated_at: "2026-05-01T00:00:00Z",
});

function expectNumericStateUpdater(
	value: SetStateAction<number>,
	current: number,
	expected: number,
) {
	expect(typeof value).toBe("function");
	expect((value as (current: number) => number)(current)).toBe(expected);
}

function createProps(
	overrides: Partial<ComponentProps<typeof TeamManageMembersSection>> = {},
) {
	const props: ComponentProps<typeof TeamManageMembersSection> = {
		canAssignOwner: true,
		canManageTeam: true,
		currentUserId: 2,
		hasMemberFilters: true,
		managerCount: 2,
		memberCurrentPage: 1,
		memberIdentifier: "new@example.com",
		memberLoading: false,
		memberOffset: 0,
		memberPageSize: 10,
		memberQuery: "ada",
		memberRole: "member",
		memberRoleFilter: "__all__",
		memberStatusFilter: "__all__",
		memberTotal: 12,
		memberTotalPages: 2,
		members: [
			member(),
			member({ id: 2, role: "owner", user_id: 3, user: user(3, "owner") }),
		],
		mutating: false,
		nextMemberPageDisabled: false,
		onAddMember: vi.fn((event) => event.preventDefault()),
		onUpdateMemberRole: vi.fn(),
		ownerCount: 1,
		prevMemberPageDisabled: true,
		requestRemoveConfirm: vi.fn(),
		roleFilterOptions: [
			{ label: "all", value: "__all__" },
			{ label: "member", value: "member" },
		],
		roleLabel: (role) => `role:${role}`,
		roleOptions: ["owner", "admin", "member"],
		setMemberIdentifier: vi.fn(),
		setMemberOffset: vi.fn(),
		setMemberQuery: vi.fn(),
		setMemberRole: vi.fn(),
		setMemberRoleFilter: vi.fn(),
		setMemberStatusFilter: vi.fn(),
		statusFilterOptions: [
			{ label: "all", value: "__all__" },
			{ label: "active", value: "active" },
		],
		team: team(),
		viewerRole: "admin",
		...overrides,
	};
	return props;
}

function renderSection(
	overrides: Partial<ComponentProps<typeof TeamManageMembersSection>> = {},
) {
	const props = createProps(overrides);
	render(<TeamManageMembersSection {...props} />);
	return props;
}

describe("TeamManageMembersSection", () => {
	it("renders members and wires filters, role updates, add and paging actions", () => {
		const props = renderSection();

		fireEvent.change(
			screen.getByPlaceholderText(
				"settings:settings_team_member_search_placeholder",
			),
			{ target: { value: "new query" } },
		);
		expect(props.setMemberOffset).toHaveBeenCalledWith(0);
		expect(props.setMemberQuery).toHaveBeenCalledWith("new query");

		fireEvent.click(
			screen.getByRole("button", {
				name: "settings:settings_team_clear_filters",
			}),
		);
		expect(props.setMemberQuery).toHaveBeenCalledWith("");
		expect(props.setMemberRoleFilter).toHaveBeenCalledWith("__all__");
		expect(props.setMemberStatusFilter).toHaveBeenCalledWith("__all__");

		const addMemberForm = screen
			.getByText("settings:settings_team_add_member")
			.closest("form");
		if (!addMemberForm) throw new Error("Expected add member form");
		fireEvent.submit(addMemberForm);
		expect(props.onAddMember).toHaveBeenCalled();

		fireEvent.change(screen.getAllByLabelText("select:member")[1], {
			target: { value: "admin" },
		});
		expect(props.onUpdateMemberRole).toHaveBeenCalledWith(2, "admin");

		fireEvent.click(
			screen.getByRole("button", { name: "settings:settings_team_leave" }),
		);
		expect(props.requestRemoveConfirm).toHaveBeenCalledWith(2);
		const nextPageButton = screen.getByText("CaretRight").closest("button");
		if (!nextPageButton) throw new Error("Expected next page button");
		fireEvent.click(nextPageButton);
		expectNumericStateUpdater(
			vi.mocked(props.setMemberOffset).mock.lastCall?.[0],
			props.memberOffset,
			10,
		);
	});

	it("shows loading and filtered empty states", () => {
		const props = createProps({
			memberLoading: true,
			memberTotal: 0,
			members: [],
		});
		const { rerender } = render(<TeamManageMembersSection {...props} />);

		expect(screen.getByText("skeleton:6:5")).toBeInTheDocument();

		rerender(
			<TeamManageMembersSection
				{...props}
				memberLoading={false}
				memberTotal={0}
				members={[]}
			/>,
		);

		expect(
			screen.getByText("settings:settings_team_member_filtered_empty"),
		).toBeInTheDocument();
	});
});
