import { act, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AdminWorkspacePage from "./AdminWorkspacePage";

const mocks = vi.hoisted(() => ({
	getUser: vi.fn(),
	getTeam: vi.fn(),
	error: vi.fn(),
	userId: "7" as string | undefined,
	teamId: undefined as string | undefined,
}));
vi.mock("react-router-dom", () => ({
	useParams: () => ({ userId: mocks.userId, teamId: mocks.teamId }),
	Navigate: ({ to }: { to: string }) => <span>redirect:{to}</span>,
}));
vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("@/services/adminService", () => ({
	adminUserService: { get: mocks.getUser },
	adminTeamService: { get: mocks.getTeam },
}));
vi.mock("@/hooks/usePageTitle", () => ({ usePageTitle: () => {} }));
vi.mock("@/hooks/useApiError", () => ({ handleApiError: mocks.error }));
vi.mock("@/components/layout/AppLayout", () => ({
	AppLayout: ({ children }: { children: ReactNode }) => <main>{children}</main>,
}));
vi.mock("@/components/admin/content-access/AdminWorkspaceBrowser", () => ({
	AdminWorkspaceBrowser: ({
		scopeKind,
		targetId,
		user,
		team,
	}: {
		scopeKind: string;
		targetId: number;
		user?: { username: string };
		team?: { name: string };
	}) => (
		<div>
			browser:{scopeKind}:{targetId}:{user?.username ?? team?.name}
		</div>
	),
}));
describe("standalone administrator workspace page", () => {
	beforeEach(() => {
		vi.resetAllMocks();
		mocks.userId = "7";
		mocks.teamId = undefined;
		mocks.getUser.mockResolvedValue({
			username: "target",
			profile: { display_name: "Target" },
		});
		mocks.getTeam.mockResolvedValue({ name: "team-target" });
	});
	it("loads the target personal identity and never redirects through global file management", async () => {
		render(<AdminWorkspacePage />);
		expect(
			await screen.findByText("browser:personal:7:target"),
		).toBeInTheDocument();
		expect(mocks.getTeam).not.toHaveBeenCalled();
	});
	it("loads the target team independently of ordinary membership", async () => {
		mocks.userId = undefined;
		mocks.teamId = "9";
		render(<AdminWorkspacePage />);
		expect(
			await screen.findByText("browser:team:9:team-target"),
		).toBeInTheDocument();
		expect(mocks.getUser).not.toHaveBeenCalled();
	});
	it("rejects invalid targets without any resource request", () => {
		mocks.userId = "-1";
		render(<AdminWorkspacePage />);
		expect(screen.getByText("redirect:/admin/users")).toBeInTheDocument();
		expect(mocks.getUser).not.toHaveBeenCalled();
	});
	it("withholds the browser when target lookup fails", async () => {
		mocks.getUser.mockRejectedValue(new Error("missing"));
		render(<AdminWorkspacePage />);
		await waitFor(() =>
			expect(screen.getByRole("alert")).toHaveTextContent(
				"content_workspace_unavailable",
			),
		);
		expect(screen.queryByText(/browser:/)).not.toBeInTheDocument();
	});
});

it("redirects invalid team IDs without looking up either identity", () => {
	mocks.userId = undefined;
	mocks.teamId = "NaN";
	mocks.getUser.mockClear();
	mocks.getTeam.mockClear();
	render(<AdminWorkspacePage />);
	expect(screen.getByText("redirect:/admin/teams")).toBeInTheDocument();
	expect(mocks.getUser).not.toHaveBeenCalled();
	expect(mocks.getTeam).not.toHaveBeenCalled();
});

it.each(["resolve", "reject"] as const)(
	"ignores stale personal identity %s after switching to a team",
	async (settlement) => {
		let resolve!: (value: unknown) => void;
		let reject!: (error: Error) => void;
		mocks.userId = "7";
		mocks.teamId = undefined;
		mocks.error.mockClear();
		mocks.getUser.mockReturnValueOnce(
			new Promise((yes, no) => {
				resolve = yes;
				reject = no;
			}),
		);
		mocks.getTeam.mockResolvedValueOnce({ name: "new-team" });
		const view = render(<AdminWorkspacePage />);
		mocks.userId = undefined;
		mocks.teamId = "9";
		view.rerender(<AdminWorkspacePage />);
		expect(
			await screen.findByText("browser:team:9:new-team"),
		).toBeInTheDocument();
		await act(async () =>
			settlement === "resolve"
				? resolve({ username: "stale-user" })
				: reject(new Error("stale")),
		);
		expect(screen.queryByText(/stale-user/)).not.toBeInTheDocument();
		expect(mocks.error).not.toHaveBeenCalled();
	},
);
