import { fireEvent, render, screen } from "@testing-library/react";
import type { ComponentProps, ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AdminTeamsPage from "@/pages/admin/AdminTeamsPage";
import type { UserSummary } from "@/types/api";

const mockState = vi.hoisted(() => ({
	createTeam: vi.fn(),
	handleApiError: vi.fn(),
	listPolicyGroups: vi.fn(),
	navigate: vi.fn(),
	policyGroupsCache: [] as unknown[],
	reload: vi.fn(),
	searchParams: "",
	setSearchParams: vi.fn(),
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
}));

function createUserSummary(): UserSummary {
	return {
		id: 9,
		username: "root",
		profile: {
			display_name: "Root",
			avatar: {
				source: "none",
				url_1024: null,
				url_512: null,
				version: 0,
			},
		},
	};
}

const TEAMS = [
	{
		archived_at: null,
		created_at: "2026-04-01T00:00:00Z",
		created_by: createUserSummary(),
		description: "Product and design",
		id: 14,
		member_count: 8,
		name: "Product",
		policy_group_id: 5,
		storage_quota: 0,
		storage_used: 2048,
		updated_at: "2026-04-02T00:00:00Z",
	},
] as const;

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		error: (...args: unknown[]) => mockState.toastError(...args),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/lib/adminPolicyGroupLookup", () => ({
	readAdminPolicyGroupLookup: () => mockState.policyGroupsCache,
	loadAdminPolicyGroupLookup: () => mockState.listPolicyGroups(100),
}));

vi.mock("react-router-dom", () => ({
	useNavigate: () => mockState.navigate,
	useSearchParams: () => [
		new URLSearchParams(mockState.searchParams),
		mockState.setSearchParams,
	],
}));

vi.mock("@/components/common/AdminTableList", () => ({
	AdminTableList: ({
		headerRow,
		items,
		renderRow,
	}: {
		headerRow: ReactNode;
		items: typeof TEAMS;
		renderRow: (item: (typeof TEAMS)[number]) => ReactNode;
	}) => (
		<table>
			{headerRow}
			<tbody>{items.map(renderRow)}</tbody>
		</table>
	),
}));

vi.mock("@/components/layout/AdminLayout", () => ({
	AdminLayout: ({ children }: { children: ReactNode }) => <div>{children}</div>,
}));

vi.mock("@/components/layout/AdminPageHeader", () => ({
	AdminPageHeader: ({
		actions,
		toolbar,
		title,
	}: {
		actions?: ReactNode;
		toolbar?: ReactNode;
		title: string;
	}) => (
		<div>
			<h1>{title}</h1>
			{actions}
			{toolbar}
		</div>
	),
}));

vi.mock("@/components/layout/AdminPageShell", () => ({
	AdminPageShell: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({ children }: { children: ReactNode }) => <span>{children}</span>,
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		onClick,
		type,
	}: {
		children: ReactNode;
		onClick?: () => void;
		type?: "button" | "submit";
	}) => (
		<button type={type ?? "button"} onClick={onClick}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({ children, open }: { children: ReactNode; open: boolean }) =>
		open ? <div>{children}</div> : null,
	DialogContent: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	DialogDescription: ({ children }: { children: ReactNode }) => (
		<p>{children}</p>
	),
	DialogFooter: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	DialogHeader: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({ children }: { children: ReactNode }) => <h2>{children}</h2>,
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({
		id,
		onChange,
		placeholder,
		type,
		value,
	}: {
		id?: string;
		onChange?: (event: { target: { value: string } }) => void;
		placeholder?: string;
		type?: string;
		value?: string;
	}) => (
		<input
			id={id}
			placeholder={placeholder}
			type={type}
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
	Select: ({ children }: { children: ReactNode }) => <div>{children}</div>,
	SelectContent: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	SelectItem: ({ children }: { children: ReactNode }) => <div>{children}</div>,
	SelectTrigger: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	SelectValue: ({ children }: { children?: ReactNode }) => <>{children}</>,
}));

vi.mock("@/components/ui/table", () => ({
	Table: ({ children, ...props }: ComponentProps<"table">) => (
		<table {...props}>{children}</table>
	),
	TableBody: ({ children, ...props }: ComponentProps<"tbody">) => (
		<tbody {...props}>{children}</tbody>
	),
	TableCell: ({ children, ...props }: ComponentProps<"td">) => (
		<td {...props}>{children}</td>
	),
	TableHead: ({ children, ...props }: ComponentProps<"th">) => (
		<th {...props}>{children}</th>
	),
	TableHeader: ({ children }: { children: ReactNode }) => (
		<thead>{children}</thead>
	),
	TableRow: ({ children, ...props }: ComponentProps<"tr">) => (
		<tr {...props}>{children}</tr>
	),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/hooks/useApiList", () => ({
	useApiList: () => ({
		items: TEAMS,
		loading: false,
		reload: mockState.reload,
	}),
}));

vi.mock("@/services/adminService", () => ({
	adminTeamService: {
		create: (...args: unknown[]) => mockState.createTeam(...args),
		list: vi.fn(),
	},
}));

describe("AdminTeamsPage", () => {
	beforeEach(() => {
		mockState.createTeam.mockReset();
		mockState.handleApiError.mockReset();
		mockState.listPolicyGroups.mockReset();
		mockState.navigate.mockReset();
		mockState.policyGroupsCache = [
			{
				created_at: "2026-04-01T00:00:00Z",
				description: "",
				id: 5,
				is_default: true,
				is_enabled: true,
				items: [
					{
						id: 1,
						max_file_size: 0,
						min_file_size: 0,
						policy: {
							id: 7,
							name: "Default",
						},
						policy_id: 7,
						priority: 1,
					},
				],
				name: "Primary",
				updated_at: "2026-04-01T00:00:00Z",
			},
		];
		mockState.reload.mockReset();
		mockState.searchParams = "";
		mockState.setSearchParams.mockReset();
		mockState.toastError.mockReset();
		mockState.toastSuccess.mockReset();

		mockState.createTeam.mockResolvedValue({
			id: 15,
		});
		mockState.listPolicyGroups.mockResolvedValue(mockState.policyGroupsCache);
		mockState.reload.mockResolvedValue(undefined);
	});

	it("navigates to the team detail page when clicking a team row", async () => {
		render(<AdminTeamsPage />);

		fireEvent.click(screen.getByText("Product"));

		expect(mockState.navigate).toHaveBeenCalledWith(
			"/admin/teams/14/overview",
			{
				viewTransition: false,
			},
		);
	});

	it("converts the create dialog team quota from MB to bytes", async () => {
		render(<AdminTeamsPage />);

		fireEvent.click(screen.getByText("new_team"));
		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Design" },
		});
		fireEvent.change(screen.getByLabelText("team_admin_identifier"), {
			target: { value: "lead@example.com" },
		});
		fireEvent.change(screen.getByLabelText("team_quota_mb"), {
			target: { value: "20" },
		});

		await screen.findByText("Primary");
		fireEvent.click(screen.getByText("create_team"));

		expect(mockState.createTeam).toHaveBeenCalledWith({
			name: "Design",
			description: undefined,
			admin_identifier: "lead@example.com",
			storage_quota: 20 * 1024 * 1024,
			policy_group_id: 5,
		});
	});

	it("omits the create quota override when the quota field is blank", async () => {
		render(<AdminTeamsPage />);

		fireEvent.click(screen.getByText("new_team"));
		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Defaulted" },
		});
		fireEvent.change(screen.getByLabelText("team_admin_identifier"), {
			target: { value: "lead@example.com" },
		});

		fireEvent.click(screen.getByText("create_team"));

		expect(mockState.createTeam).toHaveBeenCalledTimes(1);
		const payload = mockState.createTeam.mock.calls[0][0];
		expect(payload).toEqual({
			name: "Defaulted",
			description: undefined,
			admin_identifier: "lead@example.com",
			policy_group_id: 5,
		});
		expect(payload).not.toHaveProperty("storage_quota");
	});

	it("rejects overflowing create quota values before submitting", async () => {
		render(<AdminTeamsPage />);

		fireEvent.click(screen.getByText("new_team"));
		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Huge" },
		});
		fireEvent.change(screen.getByLabelText("team_admin_identifier"), {
			target: { value: "lead@example.com" },
		});
		fireEvent.change(screen.getByLabelText("team_quota_mb"), {
			target: { value: "999999999999999999999999" },
		});

		fireEvent.click(screen.getByText("create_team"));

		expect(mockState.createTeam).not.toHaveBeenCalled();
		expect(mockState.toastError).toHaveBeenCalledWith("team_quota_invalid");
	});
});
