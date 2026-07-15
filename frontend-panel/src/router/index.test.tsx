import { describe, expect, it, vi } from "vitest";

const createBrowserRouterMock = vi.fn((routes: unknown) => ({ routes }));
const ensureI18nNamespacesMock = vi.fn(async () => {});

vi.mock("@/components/layout/AdminSiteUrlMismatchPrompt", () => ({
	AdminSiteUrlMismatchPrompt: () => null,
}));

vi.mock("@/components/files/UploadAreaHost", () => ({
	UploadAreaHost: () => null,
}));

vi.mock("@/pages/ErrorPage", () => ({
	default: () => null,
}));

vi.mock("@/pages/ForcePasswordChangePage", () => ({
	default: () => null,
}));

vi.mock("@/pages/InviteRegisterPage", () => ({
	default: () => null,
}));

vi.mock("@/pages/ResetPasswordPage", () => ({
	default: () => null,
}));

vi.mock("@/pages/ShareViewPage", () => ({
	default: () => null,
}));

vi.mock("@/pages/TrashPage", () => ({
	default: () => null,
}));

vi.mock("@/pages/admin/AdminTeamDetailPage", () => ({
	default: () => null,
}));

vi.mock("@/pages/WebdavAccountsPage", () => ({
	default: () => null,
}));

vi.mock("@/pages/SettingsPage", () => ({
	default: () => null,
}));

vi.mock("@/pages/TeamManagePage", () => ({
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

vi.mock("@/i18n", () => ({
	ensureI18nNamespaces: (...args: unknown[]) =>
		ensureI18nNamespacesMock(...args),
}));

vi.mock("react", async () => {
	const actual = await vi.importActual<typeof import("react")>("react");

	return {
		...actual,
		lazy: (load: () => Promise<unknown>) => load,
	};
});

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
	element?: unknown;
	errorElement?: unknown;
	index?: boolean;
	path?: string;
};

function flattenRoutes(items: TestRoute[]): TestRoute[] {
	return items.flatMap((route) => [
		route,
		...flattenRoutes(route.children ?? []),
	]);
}

function isThenable(value: unknown): value is PromiseLike<unknown> {
	return (
		value != null && typeof (value as { then?: unknown }).then === "function"
	);
}

async function resolveLazyElement(element: unknown) {
	const targetElement =
		(element as { type?: unknown; props?: { children?: unknown } } | undefined)
			?.props?.children ?? element;
	const type = (targetElement as { type?: unknown } | undefined)?.type;

	if (typeof type !== "function") {
		throw new Error("expected lazy route element");
	}

	try {
		const result = type();
		return isThenable(result) ? await result : result;
	} catch (error) {
		if (isThenable(error)) {
			await error;
			const result = type();
			return isThenable(result) ? await result : result;
		}
		throw error;
	}
}

function routeTo(element: unknown) {
	return (element as { props?: { to?: string } } | undefined)?.props?.to;
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
			routeTo(
				allRoutes.find((route) => route.path === "/admin/settings")?.element,
			),
		).toBe("/admin/settings/site");
		expect(
			routeTo(
				allRoutes.find((route) => route.path === "/admin/settings/:section")
					?.element,
			),
		).toBe("/admin/settings/site");
		expect(
			routeTo(
				allRoutes.find((route) => route.path === "/admin/settings/general")
					?.element,
			),
		).toBe("/admin/settings/site");
		expect(
			routeTo(
				allRoutes.find((route) => route.path === "/admin/settings/operations")
					?.element,
			),
		).toBe("/admin/settings/runtime");
		expect(
			routeTo(
				allRoutes.find(
					(route) => route.path === "/admin/settings/file_processing",
				)?.element,
			),
		).toBe("/admin/settings/file-processing");
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

	it("registers root and canonical subfolder routes for public folder shares", async () => {
		const routes = (await loadRoutes()) as TestRoute[];
		const shareRoute = routes.find((route) => route.path === "/s/:token");

		expect(shareRoute).toBeDefined();
		expect(shareRoute?.children).toEqual([
			expect.objectContaining({ index: true }),
			expect.objectContaining({ path: "folder/:folderId" }),
		]);
		expect(
			routes.some((route) => route.path === "/s/:token/folder/:folderId"),
		).toBe(false);
	});

	it("loads deferred i18n namespaces before localized lazy route pages", async () => {
		const routes = (await loadRoutes()) as TestRoute[];
		const allRoutes = flattenRoutes(routes);
		const localizedRoutes = [
			{
				path: "/force-password-change",
				namespaces: ["auth", "core", "settings", "validation"],
			},
			{
				path: "/reset-password",
				namespaces: ["auth", "core"],
			},
			{
				path: "/invite/:token",
				namespaces: ["auth", "core"],
			},
			{
				path: "/s/:token",
				namespaces: ["core", "share", "files", "tasks", "errors"],
			},
			{
				path: "/trash",
				namespaces: ["core", "files", "admin", "tasks"],
			},
			{
				path: "/settings/webdav",
				namespaces: ["core", "admin", "auth", "webdav", "errors"],
			},
			{
				path: "/settings/:section",
				namespaces: ["core", "files", "settings", "auth", "admin"],
			},
			{
				path: "/settings/teams/:teamId",
				namespaces: ["core", "settings", "admin", "webdav", "errors"],
			},
			{
				path: "/settings/teams/:teamId/:section",
				namespaces: ["core", "settings", "admin", "webdav", "errors"],
			},
			{
				path: "/admin/teams/:teamId",
				namespaces: ["admin", "core", "settings"],
			},
			{
				path: "/admin/teams/:teamId/:section",
				namespaces: ["admin", "core", "settings"],
			},
		];

		for (const routeConfig of localizedRoutes) {
			const route = allRoutes.find((item) => item.path === routeConfig.path);

			await resolveLazyElement(route?.element);

			expect(ensureI18nNamespacesMock).toHaveBeenLastCalledWith(
				routeConfig.namespaces,
			);
		}

		await resolveLazyElement(routes[0]?.errorElement);
		expect(ensureI18nNamespacesMock).toHaveBeenLastCalledWith(["errors"]);
	});
});
