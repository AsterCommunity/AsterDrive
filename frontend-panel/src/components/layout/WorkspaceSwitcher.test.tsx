import {
	act,
	cleanup,
	fireEvent,
	render,
	screen,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WorkspaceSwitcher } from "@/components/layout/WorkspaceSwitcher";

const teamServiceMocks = vi.hoisted(() => ({
	list: vi.fn(),
}));

const mockState = vi.hoisted(() => ({
	navigate: vi.fn(),
	workspace: {
		kind: "personal" as const,
	},
	teams: [] as { id: number; name: string }[],
	loading: false,
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, number | string>) => {
			if (key === "workspace_team_fallback") {
				return `translated:${key}:${options?.id}`;
			}
			if (key === "workspace_switcher_label") {
				return `translated:${key}:${options?.name}`;
			}
			return `translated:${key}`;
		},
	}),
}));

vi.mock("react-router-dom", () => ({
	useNavigate: () => mockState.navigate,
}));

vi.mock("@/stores/workspaceStore", () => ({
	useWorkspaceStore: (
		selector: (state: { workspace: typeof mockState.workspace }) => unknown,
	) => selector({ workspace: mockState.workspace }),
}));

vi.mock("@/stores/teamStore", () => ({
	useTeamStore: (
		selector: (state: {
			teams: typeof mockState.teams;
			loading: boolean;
		}) => unknown,
	) =>
		selector({
			teams: mockState.teams,
			loading: mockState.loading,
		}),
}));

vi.mock("@/services/teamService", () => ({
	teamService: teamServiceMocks,
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		"aria-label": ariaLabel,
		children,
		className,
		onClick,
	}: {
		"aria-label"?: string;
		children?: React.ReactNode;
		className?: string;
		onClick?: () => void;
	}) => (
		<button
			type="button"
			aria-label={ariaLabel}
			className={className}
			onClick={onClick}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name, className }: { name: string; className?: string }) => (
		<span data-testid="icon" data-name={name} className={className} />
	),
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({
		"aria-label": ariaLabel,
		className,
		onChange,
		onKeyDown,
		placeholder,
		value,
	}: {
		"aria-label"?: string;
		className?: string;
		onChange?: React.ChangeEventHandler<HTMLInputElement>;
		onKeyDown?: React.KeyboardEventHandler<HTMLInputElement>;
		placeholder?: string;
		value?: string;
	}) => (
		<input
			aria-label={ariaLabel}
			className={className}
			onChange={onChange}
			onKeyDown={onKeyDown}
			placeholder={placeholder}
			value={value}
		/>
	),
}));

vi.mock("@/components/ui/dropdown-menu", () => {
	const mockRadioState = {
		onValueChange: undefined as undefined | ((value: string) => void),
	};

	return {
		DropdownMenu: ({
			children,
			onOpenChange,
			open,
		}: {
			children: React.ReactNode;
			onOpenChange?: (open: boolean) => void;
			open?: boolean;
		}) => (
			<div data-open={String(open ?? false)} data-testid="workspace-menu-root">
				<button type="button" onClick={() => onOpenChange?.(!(open ?? false))}>
					toggle-workspace-menu
				</button>
				<button type="button" onClick={() => onOpenChange?.(false)}>
					close-workspace-menu
				</button>
				{children}
			</div>
		),
		DropdownMenuTrigger: ({
			render,
			children,
		}: {
			render: React.ReactNode;
			children: React.ReactNode;
		}) => (
			<div>
				{render}
				{children}
			</div>
		),
		DropdownMenuContent: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
		DropdownMenuGroup: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
		DropdownMenuItem: ({
			children,
			disabled,
			onClick,
		}: {
			children: React.ReactNode;
			disabled?: boolean;
			onClick?: () => void;
		}) => (
			<button type="button" disabled={disabled} onClick={onClick}>
				{children}
			</button>
		),
		DropdownMenuLabel: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
		DropdownMenuRadioGroup: ({
			children,
			onValueChange,
			value,
		}: {
			children: React.ReactNode;
			onValueChange?: (value: string) => void;
			value?: string;
		}) => {
			mockRadioState.onValueChange = onValueChange;
			return (
				<div data-value={value} data-testid="radio-group">
					{children}
				</div>
			);
		},
		DropdownMenuRadioItem: ({
			children,
			closeOnClick,
			value,
		}: {
			children: React.ReactNode;
			closeOnClick?: boolean;
			value: string;
		}) => (
			<button
				type="button"
				data-close-on-click={String(closeOnClick ?? true)}
				data-value={value}
				onClick={() => mockRadioState.onValueChange?.(value)}
			>
				{children}
			</button>
		),
		DropdownMenuSeparator: () => <hr />,
	};
});

describe("WorkspaceSwitcher", () => {
	let dateNowSpy: ReturnType<typeof vi.spyOn>;

	beforeEach(() => {
		vi.useRealTimers();
		dateNowSpy = vi.spyOn(Date, "now");
		teamServiceMocks.list.mockReset();
		mockState.navigate.mockReset();
		mockState.workspace = { kind: "personal" };
		mockState.teams = [];
		mockState.loading = false;
	});

	afterEach(() => {
		dateNowSpy.mockRestore();
		cleanup();
	});

	it("renders the personal workspace state", () => {
		render(<WorkspaceSwitcher />);

		const trigger = screen.getByRole("button", {
			name: "translated:workspace_switcher_label:translated:my_drive",
		});

		expect(trigger).toBeInTheDocument();
		expect(trigger.className).toContain("rounded-full");
		expect(screen.getAllByText("translated:my_drive")[0]).toBeInTheDocument();
		expect(
			screen.getAllByText("translated:workspace_personal_label")[0],
		).toBeInTheDocument();
		expect(screen.getByTestId("radio-group")).toHaveAttribute(
			"data-value",
			"personal",
		);
		expect(
			screen.getByRole("button", { name: /^translated:my_drive/ }),
		).toHaveAttribute("data-close-on-click", "false");
		expect(
			screen.getByRole("textbox", {
				name: "translated:workspace_search_placeholder",
			}),
		).toBeInTheDocument();
	});

	it("uses a full-width trigger in the sidebar", () => {
		render(<WorkspaceSwitcher variant="sidebar" />);

		const trigger = screen.getByRole("button", {
			name: "translated:workspace_switcher_label:translated:my_drive",
		});

		expect(trigger.className).toContain("h-10");
		expect(trigger.className).toContain("w-full");
		expect(trigger.className).toContain("rounded-lg");
		expect(trigger.className).not.toContain("rounded-full");
	});

	it("renders the current team and navigates to another workspace", () => {
		mockState.workspace = { kind: "team", teamId: 3 };
		mockState.teams = [
			{ id: 3, name: "Core" },
			{ id: 9, name: "Design" },
		];

		render(<WorkspaceSwitcher />);

		expect(screen.getAllByText("Core")[0]).toBeInTheDocument();
		expect(
			screen.getAllByText("translated:workspace_team_label")[0],
		).toBeInTheDocument();
		expect(screen.getByTestId("radio-group")).toHaveAttribute(
			"data-value",
			"team:3",
		);

		fireEvent.click(screen.getByRole("button", { name: /^Design/ }));

		expect(mockState.navigate).toHaveBeenCalledWith("/teams/9");
	});

	it("does not navigate when reselecting the active workspace", () => {
		mockState.workspace = { kind: "team", teamId: 3 };
		mockState.teams = [{ id: 3, name: "Core" }];

		render(<WorkspaceSwitcher />);

		fireEvent.click(screen.getByRole("button", { name: /^Core/ }));

		expect(mockState.navigate).not.toHaveBeenCalled();
	});

	it("navigates back to the personal workspace", () => {
		mockState.workspace = { kind: "team", teamId: 3 };
		mockState.teams = [{ id: 3, name: "Core" }];

		render(<WorkspaceSwitcher />);

		fireEvent.click(
			screen.getByRole("button", { name: /^translated:my_drive/ }),
		);

		expect(mockState.navigate).toHaveBeenCalledWith("/");
	});

	it("keeps search key presses inside the workspace menu", () => {
		const stopPropagation = vi.spyOn(Event.prototype, "stopPropagation");

		render(<WorkspaceSwitcher />);

		const searchInput = screen.getByRole("textbox", {
			name: "translated:workspace_search_placeholder",
		});

		fireEvent.keyDown(searchInput, { key: "ArrowDown" });

		expect(stopPropagation).toHaveBeenCalledTimes(1);
		stopPropagation.mockClear();

		fireEvent.keyDown(searchInput, { key: "Tab" });

		expect(stopPropagation).not.toHaveBeenCalled();
		stopPropagation.mockRestore();
	});

	it("restores the open menu after personal and team route branches remount", () => {
		mockState.teams = [{ id: 9, name: "Design" }];
		dateNowSpy.mockReturnValue(1_000);

		const { unmount } = render(<WorkspaceSwitcher variant="sidebar" />);

		expect(screen.getByTestId("workspace-menu-root")).toHaveAttribute(
			"data-open",
			"false",
		);

		fireEvent.click(
			screen.getByRole("button", { name: "toggle-workspace-menu" }),
		);
		expect(screen.getByTestId("workspace-menu-root")).toHaveAttribute(
			"data-open",
			"true",
		);

		fireEvent.click(screen.getByRole("button", { name: /^Design/ }));
		expect(mockState.navigate).toHaveBeenCalledWith("/teams/9");

		unmount();
		mockState.workspace = { kind: "team", teamId: 9 };
		dateNowSpy.mockReturnValue(2_000);
		render(<WorkspaceSwitcher variant="sidebar" />);

		expect(screen.getByTestId("workspace-menu-root")).toHaveAttribute(
			"data-open",
			"true",
		);

		fireEvent.click(
			screen.getByRole("button", { name: "close-workspace-menu" }),
		);
		expect(screen.getByTestId("workspace-menu-root")).toHaveAttribute(
			"data-open",
			"false",
		);
	});

	it("searches team options with the backend after debounce", async () => {
		vi.useFakeTimers();
		mockState.teams = [
			{ id: 3, name: "Core" },
			{ id: 9, name: "Design" },
		];
		teamServiceMocks.list.mockResolvedValue([{ id: 9, name: "Design" }]);

		render(<WorkspaceSwitcher />);

		fireEvent.change(
			screen.getByRole("textbox", {
				name: "translated:workspace_search_placeholder",
			}),
			{ target: { value: "des" } },
		);

		expect(
			screen.queryByRole("button", { name: /^Core/ }),
		).not.toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: /^Design/ }),
		).not.toBeInTheDocument();
		expect(screen.getByText("translated:loading")).toBeInTheDocument();

		await act(async () => {
			await vi.advanceTimersByTimeAsync(250);
		});

		expect(teamServiceMocks.list).toHaveBeenCalledWith({
			keyword: "des",
			limit: 50,
		});
		expect(screen.getByRole("button", { name: /^Design/ })).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: /^Core/ }),
		).not.toBeInTheDocument();
	});

	it("shows an empty search state when the backend returns no teams", async () => {
		vi.useFakeTimers();
		mockState.teams = [{ id: 3, name: "Core" }];
		teamServiceMocks.list.mockResolvedValue([]);

		render(<WorkspaceSwitcher />);

		fireEvent.change(
			screen.getByRole("textbox", {
				name: "translated:workspace_search_placeholder",
			}),
			{ target: { value: "missing" } },
		);

		await act(async () => {
			await vi.advanceTimersByTimeAsync(250);
		});

		expect(
			screen.getByText("translated:workspace_no_matching_teams"),
		).toBeInTheDocument();
	});

	it("shows an empty search state when team search fails", async () => {
		vi.useFakeTimers();
		mockState.teams = [{ id: 3, name: "Core" }];
		teamServiceMocks.list.mockRejectedValue(new Error("search failed"));

		render(<WorkspaceSwitcher />);

		fireEvent.change(
			screen.getByRole("textbox", {
				name: "translated:workspace_search_placeholder",
			}),
			{ target: { value: "broken" } },
		);

		await act(async () => {
			await vi.advanceTimersByTimeAsync(250);
		});

		expect(
			screen.getByText("translated:workspace_no_matching_teams"),
		).toBeInTheDocument();
	});

	it("falls back to the team id when the active team is not loaded yet", () => {
		mockState.workspace = { kind: "team", teamId: 12 };

		render(<WorkspaceSwitcher />);

		expect(
			screen.getAllByText("translated:workspace_team_fallback:12"),
		).not.toHaveLength(0);
	});

	it("shows a loading row while team options are being fetched", () => {
		mockState.loading = true;

		render(<WorkspaceSwitcher />);

		expect(screen.getByText("translated:loading")).toBeInTheDocument();
	});

	it("navigates to team settings from the footer action", () => {
		render(<WorkspaceSwitcher />);

		fireEvent.click(
			screen.getByRole("button", {
				name: /translated:workspace_manage_teams/i,
			}),
		);

		expect(mockState.navigate).toHaveBeenCalledWith("/settings/teams");
	});
});
