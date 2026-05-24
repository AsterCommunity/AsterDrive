import { fireEvent, render, screen } from "@testing-library/react";
import type { ComponentProps, ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { AdminTeamDetailMembersSection } from "@/components/admin/admin-team-detail/AdminTeamDetailMembersSection";
import type { AdminTeamInfo, TeamMemberInfo, UserSummary } from "@/types/api";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) =>
			options ? `${key}:${Object.values(options).join("/")}` : key,
	}),
}));

vi.mock("@/components/common/AdminTable", () => ({
	AdminSortableTableHead: ({
		children,
		onSortChange,
		sortKey,
	}: {
		children: ReactNode;
		onSortChange: (sortKey: string, sortOrder: "asc") => void;
		sortKey: string;
	}) => (
		<th>
			<button type="button" onClick={() => onSortChange(sortKey, "asc")}>
				{children}
			</button>
		</th>
	),
	AdminTable: ({ children }: { children: ReactNode }) => (
		<table>{children}</table>
	),
	AdminTableBody: ({ children }: { children: ReactNode }) => (
		<tbody>{children}</tbody>
	),
	AdminTableCell: ({ children }: { children: ReactNode }) => (
		<td>{children}</td>
	),
	AdminTableHead: ({ children }: { children: ReactNode }) => (
		<th>{children}</th>
	),
	AdminTableHeader: ({ children }: { children: ReactNode }) => (
		<thead>{children}</thead>
	),
	AdminTableRow: ({ children }: { children: ReactNode }) => <tr>{children}</tr>,
}));

vi.mock("@/components/common/EmptyState", () => ({
	EmptyState: ({ title }: { title: string }) => <div>{title}</div>,
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
		onChange,
		placeholder,
		value,
	}: {
		onChange?: (event: { target: { value: string } }) => void;
		placeholder?: string;
		value?: string;
	}) => (
		<input
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

function createProps(
	overrides: Partial<ComponentProps<typeof AdminTeamDetailMembersSection>> = {},
) {
	const team: AdminTeamInfo = {
		archived_at: null,
		created_at: "2026-05-01T00:00:00Z",
		created_by: user(1, "root"),
		description: "",
		id: 4,
		member_count: 12,
		name: "Product",
		policy_group_id: null,
		storage_quota: 0,
		storage_used: 0,
		updated_at: "2026-05-01T00:00:00Z",
	};
	return {
		canMutateTeam: true,
		hasMemberFilters: true,
		managerCount: 2,
		memberCurrentPage: 1,
		memberIdentifier: "new@example.com",
		memberLoading: false,
		memberMutating: false,
		memberOffset: 0,
		memberQuery: "ada",
		memberRole: "member",
		memberRoleFilter: "__all__",
		memberSortBy: "role",
		memberSortOrder: "asc",
		memberStatusFilter: "__all__",
		memberTotal: 12,
		memberTotalPages: 2,
		members: [member()],
		nextMemberPageDisabled: false,
		ownerCount: 1,
		prevMemberPageDisabled: true,
		requestRemoveConfirm: vi.fn(),
		roleFilterOptions: [{ label: "all", value: "__all__" }],
		roleLabel: (role) => `role:${role}`,
		roleOptions: ["owner", "admin", "member"],
		setMemberIdentifier: vi.fn(),
		setMemberOffset: vi.fn(),
		setMemberQuery: vi.fn(),
		setMemberRole: vi.fn(),
		setMemberRoleFilter: vi.fn(),
		setMemberStatusFilter: vi.fn(),
		statusFilterOptions: [{ label: "all", value: "__all__" }],
		team,
		onAddMember: vi.fn((event) => event.preventDefault()),
		onMemberSortChange: vi.fn(),
		onUpdateMemberRole: vi.fn(),
		...overrides,
	} satisfies ComponentProps<typeof AdminTeamDetailMembersSection>;
}

function expectNumericStateUpdater(
	value: number | ((current: number) => number) | undefined,
	current: number,
	expected: number,
) {
	expect(typeof value).toBe("function");
	expect((value as (current: number) => number)(current)).toBe(expected);
}

describe("AdminTeamDetailMembersSection", () => {
	it("wires filters, sorting, member mutation controls and pagination", () => {
		const props = createProps();
		render(<AdminTeamDetailMembersSection {...props} />);

		fireEvent.change(
			screen.getByPlaceholderText("team_member_search_placeholder"),
			{
				target: { value: "query" },
			},
		);
		expect(props.setMemberOffset).toHaveBeenCalledWith(0);
		expect(props.setMemberQuery).toHaveBeenCalledWith("query");

		fireEvent.click(screen.getByRole("button", { name: "clear_filters" }));
		expect(props.setMemberQuery).toHaveBeenCalledWith("");
		expect(props.setMemberRoleFilter).toHaveBeenCalledWith("__all__");
		expect(props.setMemberStatusFilter).toHaveBeenCalledWith("__all__");

		fireEvent.click(
			screen.getByRole("button", { name: "settings:settings_team_member" }),
		);
		expect(props.onMemberSortChange).toHaveBeenCalledWith("username", "asc");

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
			screen.getByRole("button", {
				name: "settings:settings_team_remove_member",
			}),
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

	it("renders readonly, loading and empty states", () => {
		const props = createProps({
			canMutateTeam: false,
			memberLoading: true,
			memberTotal: 0,
			members: [],
		});
		const { rerender } = render(<AdminTeamDetailMembersSection {...props} />);

		expect(
			screen.getByText("team_members_readonly_archived"),
		).toBeInTheDocument();
		expect(screen.getByText("skeleton:6:5")).toBeInTheDocument();

		rerender(
			<AdminTeamDetailMembersSection
				{...props}
				memberLoading={false}
				hasMemberFilters={false}
			/>,
		);

		expect(
			screen.getByText("settings:settings_team_no_members"),
		).toBeInTheDocument();
	});
});
