import { describe, expect, it, vi } from "vitest";

const createBrowserRouterMock = vi.fn((routes: unknown) => ({ routes }));

vi.mock("@/components/layout/AdminSiteUrlMismatchPrompt", () => ({
	AdminSiteUrlMismatchPrompt: () => null,
}));

vi.mock("@/components/files/UploadAreaHost", () => ({
	UploadAreaHost: () => null,
}));

vi.mock("@/pages/ErrorPage", () => ({
	default: () => null,
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: () => undefined,
}));

vi.mock("@/stores/fileStore", () => ({
	useFileStore: {
		getState: () => ({
			resetWorkspaceState: vi.fn(),
		}),
	},
}));

vi.mock("@/stores/workspaceStore", () => ({
	useWorkspaceStore: {
		getState: () => ({
			workspace: { kind: "personal" },
		}),
		setState: vi.fn(),
	},
}));

vi.mock("react-router-dom", async () => {
	const actual =
		await vi.importActual<typeof import("react-router-dom")>(
			"react-router-dom",
		);

	return {
		...actual,
		createBrowserRouter: createBrowserRouterMock,
	};
});

async function loadRoutes() {
	createBrowserRouterMock.mockClear();
	vi.resetModules();
	await import("./index");
	return createBrowserRouterMock.mock.calls[0]?.[0] as Array<{
		children?: Array<unknown>;
		element?: {
			props?: {
				replace?: boolean;
				to?: string;
			};
		};
		path?: string;
	}>;
}

type TestRoute = {
	children?: TestRoute[];
	element?: {
		props?: {
			to?: string;
		};
	};
	path?: string;
};

function flattenRoutes(items: TestRoute[]): TestRoute[] {
	return items.flatMap((route) => [
		route,
		...flattenRoutes(route.children ?? []),
	]);
}

describe("router", () => {
	it("redirects unmatched routes to the home route", async () => {
		const routes = await loadRoutes();
		const fallbackRoute = routes.at(-1);

		expect(fallbackRoute?.path).toBe("*");
		expect(fallbackRoute?.element?.props?.to).toBe("/");
		expect(fallbackRoute?.element?.props?.replace).toBe(true);
	});

	it("registers admin mail settings routes without the removed verify-contact page", async () => {
		const routes = await loadRoutes();
		const allRoutes = flattenRoutes(routes as TestRoute[]);

		expect(
			allRoutes.some((route) => route.path === "/admin/settings/user"),
		).toBe(true);
		expect(
			allRoutes.some((route) => route.path === "/admin/settings/mail"),
		).toBe(true);
		expect(allRoutes.some((route) => route.path === "/admin/tasks")).toBe(true);
		expect(
			allRoutes.some((route) => route.path === "/admin/users/invitations"),
		).toBe(true);
		expect(allRoutes.some((route) => route.path === "/tasks")).toBe(true);
		expect(allRoutes.some((route) => route.path === "tasks")).toBe(true);
		expect(allRoutes.some((route) => route.path === "/tags")).toBe(false);
		expect(allRoutes.some((route) => route.path === "tags")).toBe(false);
		expect(allRoutes.some((route) => route.path === "/settings/:section")).toBe(
			true,
		);
		expect(allRoutes.some((route) => route.path === "/verify-contact")).toBe(
			false,
		);
		expect(
			allRoutes.find((route) => route.path === "/admin/settings")?.element
				?.props?.to,
		).toBe("/admin/settings/site");
		expect(
			allRoutes.find((route) => route.path === "/admin/settings/:section")
				?.element?.props?.to,
		).toBe("/admin/settings/site");
		expect(
			allRoutes.find((route) => route.path === "/admin/settings/general")
				?.element?.props?.to,
		).toBe("/admin/settings/site");
		expect(
			allRoutes.find((route) => route.path === "/admin/settings/operations")
				?.element?.props?.to,
		).toBe("/admin/settings/runtime");
	});

	it("keeps settings routes outside workspace routes so they preserve the active workspace", async () => {
		const routes = (await loadRoutes()) as TestRoute[];
		const protectedRoute = routes.find((route) =>
			(route.children ?? []).some(
				(child) =>
					child.path === "/settings/webdav" || child.path === "/teams/:teamId",
			),
		);
		const protectedChildren = protectedRoute?.children ?? [];
		const personalWorkspaceRoute = protectedChildren.find(
			(route) => route.path == null && route.children?.length,
		);
		const personalPaths = flattenRoutes(personalWorkspaceRoute?.children ?? [])
			.map((route) => route.path)
			.filter(Boolean);

		expect(personalPaths).not.toContain("/settings/webdav");
		expect(personalPaths).not.toContain("/settings/:section");
		expect(personalPaths).not.toContain("/settings/teams/:teamId/:section");
		expect(
			protectedChildren.some((route) => route.path === "/settings/webdav"),
		).toBe(true);
		expect(
			protectedChildren.some((route) => route.path === "/settings/:section"),
		).toBe(true);
		expect(
			protectedChildren.some(
				(route) => route.path === "/settings/teams/:teamId/:section",
			),
		).toBe(true);
	});
});
