import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "@/App";

const mockState = vi.hoisted(() => ({
	authStore: {
		bootOffline: false,
		checkAuth: vi.fn(),
		isAuthenticated: false,
		isChecking: false,
		user: null as { role?: string } | null,
	},
	brandingLoad: vi.fn(),
	displayTimeZoneStore: {
		preference: "browser",
	},
	previewAppsLoad: vi.fn(),
	mediaDataSupportLoad: vi.fn(),
	warmupRouteChunks: vi.fn(),
	setAuthState: vi.fn(),
	themeInit: vi.fn(),
	thumbnailSupportLoad: vi.fn(),
	toastSuccess: vi.fn(),
}));

vi.mock("react-router-dom", () => ({
	RouterProvider: () => <div data-testid="router-provider" />,
}));

vi.mock("sonner", () => ({
	Toaster: () => <div data-testid="toaster" />,
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/i18n", () => ({
	default: {
		t: (key: string) => key,
	},
}));

vi.mock("@/router", () => ({
	router: {},
}));

vi.mock("@/hooks/usePwaUpdate", () => ({
	usePwaUpdate: vi.fn(),
}));

vi.mock("@/hooks/useStorageChangeEvents", () => ({
	useStorageChangeEvents: vi.fn(),
}));

vi.mock("@/lib/pwaWarmup", () => ({
	warmupRouteChunks: (...args: unknown[]) =>
		mockState.warmupRouteChunks(...args),
}));

vi.mock("@/lib/idleTask", () => ({
	runWhenIdle: (task: () => void) => {
		const timer = window.setTimeout(task, 1200);
		return () => window.clearTimeout(timer);
	},
}));

vi.mock("@/components/layout/OfflineBootFallback", () => ({
	OfflineBootFallback: () => <div data-testid="offline-fallback" />,
}));

vi.mock("@/components/music/MusicPlayerHost", () => ({
	MusicPlayerHost: () => <div data-testid="music-player-host" />,
}));

vi.mock("@/stores/brandingStore", () => ({
	useBrandingStore: {
		getState: () => ({
			load: mockState.brandingLoad,
		}),
	},
}));

vi.mock("@/stores/mediaDataSupportStore", () => ({
	useMediaDataSupportStore: {
		getState: () => ({
			load: mockState.mediaDataSupportLoad,
		}),
	},
}));

vi.mock("@/stores/previewAppStore", () => ({
	usePreviewAppStore: {
		getState: () => ({
			load: mockState.previewAppsLoad,
		}),
	},
}));

vi.mock("@/stores/thumbnailSupportStore", () => ({
	useThumbnailSupportStore: {
		getState: () => ({
			load: mockState.thumbnailSupportLoad,
		}),
	},
}));

vi.mock("@/stores/displayTimeZoneStore", () => ({
	resolveActiveDisplayTimeZone: (preference: string) =>
		preference === "browser" ? "UTC" : preference,
	useDisplayTimeZoneStore: (
		selector: (state: typeof mockState.displayTimeZoneStore) => unknown,
	) => selector(mockState.displayTimeZoneStore),
}));

vi.mock("@/stores/themeStore", () => ({
	useThemeStore: {
		getState: () => ({
			init: mockState.themeInit,
		}),
	},
}));

vi.mock("@/stores/authStore", () => {
	const useAuthStore = Object.assign(
		(selector: (state: typeof mockState.authStore) => unknown) =>
			selector(mockState.authStore),
		{
			setState: (...args: unknown[]) => mockState.setAuthState(...args),
		},
	);

	return {
		useAuthStore,
	};
});

describe("App", () => {
	beforeEach(() => {
		mockState.authStore.bootOffline = false;
		mockState.authStore.checkAuth.mockReset();
		mockState.authStore.isAuthenticated = false;
		mockState.authStore.isChecking = false;
		mockState.authStore.user = null;
		mockState.displayTimeZoneStore.preference = "browser";
		mockState.brandingLoad.mockReset();
		mockState.previewAppsLoad.mockReset();
		mockState.mediaDataSupportLoad.mockReset();
		mockState.setAuthState.mockReset();
		mockState.themeInit.mockReset();
		mockState.thumbnailSupportLoad.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.warmupRouteChunks.mockReset();
		vi.useRealTimers();
	});

	afterEach(() => {
		window.history.replaceState({}, "", "/");
	});

	it("skips the bootstrap auth check on login", () => {
		window.history.replaceState({}, "", "/login");

		render(<App />);

		expect(mockState.brandingLoad).toHaveBeenCalledTimes(1);
		expect(mockState.previewAppsLoad).not.toHaveBeenCalled();
		expect(mockState.thumbnailSupportLoad).not.toHaveBeenCalled();
		expect(mockState.mediaDataSupportLoad).not.toHaveBeenCalled();
		expect(mockState.authStore.checkAuth).not.toHaveBeenCalled();
		expect(mockState.setAuthState).toHaveBeenCalledWith({ isChecking: false });
	});

	it("skips the bootstrap auth check on public share routes", () => {
		window.history.replaceState({}, "", "/s/share-token");

		render(<App />);

		expect(mockState.authStore.checkAuth).not.toHaveBeenCalled();
		expect(mockState.setAuthState).toHaveBeenCalledWith({ isChecking: false });
	});

	it("runs the bootstrap auth check on protected routes", () => {
		window.history.replaceState({}, "", "/");

		render(<App />);

		expect(mockState.brandingLoad).toHaveBeenCalledTimes(1);
		expect(mockState.previewAppsLoad).not.toHaveBeenCalled();
		expect(mockState.thumbnailSupportLoad).not.toHaveBeenCalled();
		expect(mockState.mediaDataSupportLoad).not.toHaveBeenCalled();
		expect(mockState.authStore.checkAuth).toHaveBeenCalledTimes(1);
		expect(mockState.setAuthState).not.toHaveBeenCalled();
	});

	it("defers noncritical public config and does not revalidate on tab visibility changes", () => {
		vi.useFakeTimers();
		render(<App />);

		expect(mockState.brandingLoad).toHaveBeenCalledTimes(1);
		expect(mockState.previewAppsLoad).not.toHaveBeenCalled();
		expect(mockState.thumbnailSupportLoad).not.toHaveBeenCalled();
		expect(mockState.mediaDataSupportLoad).not.toHaveBeenCalled();

		vi.advanceTimersByTime(1200);

		expect(mockState.previewAppsLoad).toHaveBeenCalledTimes(1);
		expect(mockState.thumbnailSupportLoad).toHaveBeenCalledTimes(1);
		expect(mockState.mediaDataSupportLoad).toHaveBeenCalledTimes(1);

		Object.defineProperty(document, "visibilityState", {
			configurable: true,
			value: "visible",
		});
		document.dispatchEvent(new Event("visibilitychange"));
		vi.advanceTimersByTime(300_000);

		expect(mockState.brandingLoad).toHaveBeenCalledTimes(1);
		expect(mockState.previewAppsLoad).toHaveBeenCalledTimes(1);
		expect(mockState.thumbnailSupportLoad).toHaveBeenCalledTimes(1);
		expect(mockState.mediaDataSupportLoad).toHaveBeenCalledTimes(1);
	});

	it("does not revalidate public config on an interval", () => {
		vi.useFakeTimers();
		Object.defineProperty(document, "visibilityState", {
			configurable: true,
			value: "visible",
		});

		render(<App />);

		vi.advanceTimersByTime(300_000);

		expect(mockState.brandingLoad).toHaveBeenCalledTimes(1);
		expect(mockState.previewAppsLoad).toHaveBeenCalledTimes(1);
		expect(mockState.thumbnailSupportLoad).toHaveBeenCalledTimes(1);
		expect(mockState.mediaDataSupportLoad).toHaveBeenCalledTimes(1);
	});

	it("renders the offline boot fallback instead of the router", () => {
		mockState.authStore.bootOffline = true;

		render(<App />);

		expect(screen.getByTestId("offline-fallback")).toBeInTheDocument();
		expect(screen.queryByTestId("router-provider")).not.toBeInTheDocument();
		expect(screen.getByTestId("toaster")).toBeInTheDocument();
		expect(screen.getByTestId("music-player-host")).toBeInTheDocument();
	});

	it("defers redirect handling and warmup while auth is still checking", async () => {
		mockState.authStore.isAuthenticated = true;
		mockState.authStore.isChecking = true;
		window.history.replaceState({}, "", "/?external_auth=success");

		render(<App />);
		await Promise.resolve();

		expect(mockState.toastSuccess).not.toHaveBeenCalled();
		expect(mockState.warmupRouteChunks).not.toHaveBeenCalled();
		expect(window.location.search).toBe("?external_auth=success");
	});

	it("shows and consumes the external auth success redirect toast after auth is ready", () => {
		mockState.authStore.isAuthenticated = true;
		mockState.authStore.isChecking = false;
		window.history.replaceState(
			{ preserved: true },
			"",
			"/?external_auth=success&view=grid#files",
		);

		render(<App />);

		expect(mockState.toastSuccess).toHaveBeenCalledWith("auth:login_success", {
			id: "external-auth-login-success",
		});
		expect(window.location.pathname).toBe("/");
		expect(window.location.search).toBe("?view=grid");
		expect(window.location.hash).toBe("#files");
	});

	it("removes the external auth success query when it is the only search parameter", () => {
		mockState.authStore.isAuthenticated = true;
		mockState.authStore.isChecking = false;
		window.history.replaceState({}, "", "/tasks?external_auth=success");

		render(<App />);

		expect(mockState.toastSuccess).toHaveBeenCalledWith("auth:login_success", {
			id: "external-auth-login-success",
		});
		expect(window.location.pathname).toBe("/tasks");
		expect(window.location.search).toBe("");
	});

	it("warms user route chunks after auth is ready", async () => {
		mockState.authStore.isAuthenticated = true;
		mockState.authStore.isChecking = false;

		render(<App />);

		await waitFor(() => {
			expect(mockState.warmupRouteChunks).toHaveBeenCalledWith("user");
		});
	});

	it("warms admin route chunks for admin users", async () => {
		mockState.authStore.isAuthenticated = true;
		mockState.authStore.isChecking = false;
		mockState.authStore.user = { role: "admin" };

		render(<App />);

		await waitFor(() => {
			expect(mockState.warmupRouteChunks).toHaveBeenCalledWith("admin");
		});
	});

	it("removes the display time zone attribute on unmount", () => {
		mockState.displayTimeZoneStore.preference = "Asia/Shanghai";

		const { unmount } = render(<App />);

		expect(document.documentElement).toHaveAttribute(
			"data-display-time-zone",
			"Asia/Shanghai",
		);

		unmount();

		expect(document.documentElement).not.toHaveAttribute(
			"data-display-time-zone",
		);
	});
});
