import { act, render, screen, waitFor } from "@testing-library/react";
import { useEffect } from "react";
import { createMemoryRouter, RouterProvider } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Workspace } from "@/lib/workspace";
import { WorkspaceRoute } from "./WorkspaceRoute";

const mockState = vi.hoisted(() => ({
	pageMounts: 0,
	pageUnmounts: 0,
	resetWorkspaceState: vi.fn(),
	setWorkspace: vi.fn(),
	workspace: { kind: "personal" } as Workspace,
}));

vi.mock("@/components/files/UploadAreaHost", () => ({
	UploadAreaHost: ({ workspace }: { workspace: Workspace }) => (
		<div data-testid="workspace-marker">
			{workspace.kind === "team" ? `team:${workspace.teamId}` : "personal"}
		</div>
	),
}));

vi.mock("./Loading", () => ({
	Loading: () => <div data-testid="workspace-route-loading" />,
}));

vi.mock("@/stores/fileStore", () => ({
	useFileStore: {
		getState: () => ({
			resetWorkspaceState: mockState.resetWorkspaceState,
		}),
	},
}));

vi.mock("@/stores/workspaceStore", () => ({
	useWorkspaceStore: {
		getState: () => ({
			setWorkspace: (workspace: Workspace) => {
				mockState.workspace = workspace;
				mockState.setWorkspace(workspace);
			},
			workspace: mockState.workspace,
		}),
	},
}));

function WorkspacePageProbe() {
	useEffect(() => {
		mockState.pageMounts += 1;
		return () => {
			mockState.pageUnmounts += 1;
		};
	}, []);

	return <div>workspace page</div>;
}

function createWorkspaceRouter(initialEntry: string) {
	const workspaceRouteElement = <WorkspaceRoute />;
	const workspacePageElement = <WorkspacePageProbe />;
	return createMemoryRouter(
		[
			{
				element: workspaceRouteElement,
				children: [{ path: "/", element: workspacePageElement }],
			},
			{
				path: "/teams/:teamId",
				element: workspaceRouteElement,
				children: [{ index: true, element: workspacePageElement }],
			},
		],
		{ initialEntries: [initialEntry] },
	);
}

describe("WorkspaceRoute", () => {
	beforeEach(() => {
		mockState.pageMounts = 0;
		mockState.pageUnmounts = 0;
		mockState.resetWorkspaceState.mockReset();
		mockState.setWorkspace.mockReset();
		mockState.workspace = { kind: "personal" };
	});

	it("keeps one mounted page and does not show the route fallback across workspace switches", async () => {
		const router = createWorkspaceRouter("/");
		const view = render(<RouterProvider router={router} />);

		expect(await screen.findByTestId("workspace-marker")).toHaveTextContent(
			"personal",
		);
		expect(mockState.pageMounts).toBe(1);
		expect(mockState.setWorkspace).not.toHaveBeenCalled();
		expect(mockState.resetWorkspaceState).not.toHaveBeenCalled();

		let routeFallbackSeen = false;
		const observer = new MutationObserver((mutations) => {
			for (const mutation of mutations) {
				for (const node of mutation.addedNodes) {
					if (
						node instanceof Element &&
						(node.matches('[data-testid="workspace-route-loading"]') ||
							node.querySelector('[data-testid="workspace-route-loading"]'))
					) {
						routeFallbackSeen = true;
					}
				}
			}
		});
		observer.observe(view.container, { childList: true, subtree: true });

		await act(async () => {
			await router.navigate("/teams/9");
		});

		expect(screen.getByTestId("workspace-marker")).toHaveTextContent("team:9");

		await act(async () => {
			await router.navigate("/teams/12");
		});

		expect(screen.getByTestId("workspace-marker")).toHaveTextContent("team:12");

		await act(async () => {
			await router.navigate("/");
		});

		expect(screen.getByTestId("workspace-marker")).toHaveTextContent(
			"personal",
		);
		await act(async () => Promise.resolve());
		observer.disconnect();

		expect(mockState.pageMounts).toBe(1);
		expect(mockState.pageUnmounts).toBe(0);
		expect(routeFallbackSeen).toBe(false);
		expect(
			mockState.setWorkspace.mock.calls.map(([workspace]) => workspace),
		).toEqual([
			{ kind: "team", teamId: 9 },
			{ kind: "team", teamId: 12 },
			{ kind: "personal" },
		]);
		expect(mockState.resetWorkspaceState).toHaveBeenCalledTimes(3);
	});

	it("does not reset workspace state when the route already matches the store", async () => {
		mockState.workspace = { kind: "team", teamId: 9 };
		const router = createWorkspaceRouter("/teams/9");

		render(<RouterProvider router={router} />);

		expect(await screen.findByTestId("workspace-marker")).toHaveTextContent(
			"team:9",
		);
		expect(mockState.setWorkspace).not.toHaveBeenCalled();
		expect(mockState.resetWorkspaceState).not.toHaveBeenCalled();
	});

	it("initializes a team workspace when opening a team route directly", async () => {
		const router = createWorkspaceRouter("/teams/9");

		render(<RouterProvider router={router} />);

		expect(await screen.findByTestId("workspace-marker")).toHaveTextContent(
			"team:9",
		);
		expect(mockState.setWorkspace).toHaveBeenCalledWith({
			kind: "team",
			teamId: 9,
		});
		expect(mockState.resetWorkspaceState).toHaveBeenCalledTimes(1);
	});

	it.each([1, Number.MAX_SAFE_INTEGER])(
		"accepts the valid team id boundary %s",
		async (teamId) => {
			const router = createWorkspaceRouter(`/teams/${teamId}`);

			render(<RouterProvider router={router} />);

			expect(await screen.findByTestId("workspace-marker")).toHaveTextContent(
				`team:${teamId}`,
			);
			expect(mockState.setWorkspace).toHaveBeenCalledWith({
				kind: "team",
				teamId,
			});
			expect(mockState.resetWorkspaceState).toHaveBeenCalledTimes(1);
		},
	);

	it.each([
		"0",
		"-1",
		"1.5",
		"01",
		"+1",
		"1e3",
		"0x10",
		"not-a-number",
		String(Number.MAX_SAFE_INTEGER + 1),
	])(
		"redirects the invalid team id %s without resetting workspace state",
		async (teamId) => {
			const router = createWorkspaceRouter(`/teams/${teamId}`);

			render(<RouterProvider router={router} />);

			await waitFor(() => {
				expect(router.state.location.pathname).toBe("/");
			});
			expect(await screen.findByTestId("workspace-marker")).toHaveTextContent(
				"personal",
			);
			expect(mockState.setWorkspace).not.toHaveBeenCalled();
			expect(mockState.resetWorkspaceState).not.toHaveBeenCalled();
		},
	);
});
