import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "@/App";

const mockState = vi.hoisted(() => ({
	authStore: {
		bootOffline: false,
		checkAuth: vi.fn(),
		probePublicSession: vi.fn(),
		isAuthenticated: false,
		isChecking: false,
		user: null as {
			preferences?: { storage_event_stream_enabled?: boolean };
			role?: string;
		} | null,
	},
	frontendConfigLoad: vi.fn(),
	initFrontendConfigRuntime: vi.fn(),
	displayTimeZoneStore: {
		preference: "browser",
	},
	ensureAllI18nNamespaces: vi.fn(),
	ensureAuthenticatedShellI18nNamespaces: vi.fn(),
	loggerWarn: vi.fn(),
	previewAppsLoad: vi.fn(),
	mediaDataSupportLoad: vi.fn(),
	musicPlayerHostMountRequested: false,
	setAuthState: vi.fn(),
	themeInit: vi.fn(),
	thumbnailSupportLoad: vi.fn(),
	toastSuccess: vi.fn(),
	warmupRouteChunks: vi.fn(),
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
		dir: () => "ltr",
		t: (key: string) => key,
	},
	ensureAllI18nNamespaces: () => mockState.ensureAllI18nNamespaces(),
	ensureAuthenticatedShellI18nNamespaces: () =>
		mockState.ensureAuthenticatedShellI18nNamespaces(),
}));

vi.mock("@/router", () => ({
	router: {},
}));

vi.mock("@/hooks/usePwaUpdate", () => ({
	usePwaUpdate: vi.fn(),
}));

vi.mock("@/hooks/useStorageChangeEvents", () => ({
	StorageChangeEventsBridge: () => <div data-testid="storage-events-bridge" />,
	useStorageChangeEvents: vi.fn(),
}));

vi.mock("@/lib/idleTask", () => ({
	runWhenIdle: (task: () => void) => {
		const timer = window.setTimeout(task, 1200);
		return () => window.clearTimeout(timer);
	},
}));

vi.mock("@/lib/logger", () => ({
	logger: {
		warn: (...args: unknown[]) => mockState.loggerWarn(...args),
	},
}));

vi.mock("@/components/layout/OfflineBootFallback", () => ({
	OfflineBootFallback: () => <div data-testid="offline-fallback" />,
}));

vi.mock("@/components/music/MusicPlayerHost", () => ({
	MusicPlayerHost: () => <div data-testid="music-player-host" />,
}));

vi.mock("@/lib/musicPlayerMountSignal", () => ({
	useMusicPlayerHostMountRequested: () =>
		mockState.musicPlayerHostMountRequested,
}));

vi.mock("@/lib/pwaWarmup", () => ({
	warmupRouteChunks: (...args: unknown[]) =>
		mockState.warmupRouteChunks(...args),
}));

vi.mock("@/stores/frontendConfigStore", () => ({
	initFrontendConfigRuntime: () => mockState.initFrontendConfigRuntime(),
	useFrontendConfigStore: {
		getState: () => ({
			load: mockState.frontendConfigLoad,
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
		mockState.authStore.probePublicSession.mockReset();
		mockState.authStore.isAuthenticated = false;
		mockState.authStore.isChecking = false;
		mockState.authStore.user = null;
		mockState.displayTimeZoneStore.preference = "browser";
		mockState.ensureAllI18nNamespaces.mockReset();
		mockState.ensureAllI18nNamespaces.mockResolvedValue(undefined);
		mockState.ensureAuthenticatedShellI18nNamespaces.mockReset();
		mockState.ensureAuthenticatedShellI18nNamespaces.mockResolvedValue(
			undefined,
		);
		mockState.loggerWarn.mockReset();
		mockState.musicPlayerHostMountRequested = false;
		mockState.frontendConfigLoad.mockReset();
		mockState.initFrontendConfigRuntime.mockReset();
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

	it("skips the bootstrap auth check on login", async () => {
		window.history.replaceState({}, "", "/login");

		render(<App />);

		await waitFor(() => {
			expect(mockState.frontendConfigLoad).toHaveBeenCalledTimes(1);
		});
		expect(mockState.previewAppsLoad).not.toHaveBeenCalled();
		expect(mockState.thumbnailSupportLoad).not.toHaveBeenCalled();
		expect(mockState.mediaDataSupportLoad).not.toHaveBeenCalled();
		expect(mockState.ensureAllI18nNamespaces).not.toHaveBeenCalled();
		expect(
			mockState.ensureAuthenticatedShellI18nNamespaces,
		).not.toHaveBeenCalled();
		expect(mockState.authStore.checkAuth).not.toHaveBeenCalled();
		expect(mockState.setAuthState).toHaveBeenCalledWith({ isChecking: false });
	});

	it("uses an optional session probe on public share routes", () => {
		window.history.replaceState({}, "", "/s/share-token");

		render(<App />);

		expect(mockState.authStore.checkAuth).not.toHaveBeenCalled();
		expect(mockState.authStore.probePublicSession).toHaveBeenCalledTimes(1);
		expect(mockState.setAuthState).not.toHaveBeenCalled();
	});

	it("skips the bootstrap auth check on invitation routes", () => {
		window.history.replaceState({}, "", "/invite/invite-token");

		render(<App />);

		expect(mockState.authStore.checkAuth).not.toHaveBeenCalled();
		expect(mockState.setAuthState).toHaveBeenCalledWith({ isChecking: false });
	});

	it("skips the bootstrap auth check on password reset routes", () => {
		window.history.replaceState({}, "", "/reset-password?token=reset-token");

		render(<App />);

		expect(mockState.authStore.checkAuth).not.toHaveBeenCalled();
		expect(mockState.setAuthState).toHaveBeenCalledWith({ isChecking: false });
	});

	it("runs the bootstrap auth check on protected routes", async () => {
		window.history.replaceState({}, "", "/");

		render(<App />);

		await waitFor(() => {
			expect(mockState.frontendConfigLoad).toHaveBeenCalledTimes(1);
		});
		expect(mockState.previewAppsLoad).not.toHaveBeenCalled();
		expect(mockState.thumbnailSupportLoad).not.toHaveBeenCalled();
		expect(mockState.mediaDataSupportLoad).not.toHaveBeenCalled();
		expect(mockState.authStore.checkAuth).toHaveBeenCalledTimes(1);
		expect(mockState.setAuthState).not.toHaveBeenCalled();
	});

	it("does not load support config while unauthenticated", async () => {
		vi.useFakeTimers();
		render(<App />);

		await vi.waitFor(() => {
			expect(mockState.frontendConfigLoad).toHaveBeenCalledTimes(1);
		});
		expect(mockState.previewAppsLoad).not.toHaveBeenCalled();
		expect(mockState.thumbnailSupportLoad).not.toHaveBeenCalled();
		expect(mockState.mediaDataSupportLoad).not.toHaveBeenCalled();
		expect(
			mockState.ensureAuthenticatedShellI18nNamespaces,
		).not.toHaveBeenCalled();

		await vi.advanceTimersByTimeAsync(1200);

		expect(
			mockState.ensureAuthenticatedShellI18nNamespaces,
		).not.toHaveBeenCalled();
		expect(mockState.previewAppsLoad).not.toHaveBeenCalled();
		expect(mockState.thumbnailSupportLoad).not.toHaveBeenCalled();
		expect(mockState.mediaDataSupportLoad).not.toHaveBeenCalled();
		expect(mockState.warmupRouteChunks).not.toHaveBeenCalled();

		Object.defineProperty(document, "visibilityState", {
			configurable: true,
			value: "visible",
		});
		document.dispatchEvent(new Event("visibilitychange"));
		await vi.advanceTimersByTimeAsync(300_000);

		expect(mockState.frontendConfigLoad).toHaveBeenCalledTimes(1);
		expect(mockState.previewAppsLoad).not.toHaveBeenCalled();
		expect(mockState.thumbnailSupportLoad).not.toHaveBeenCalled();
		expect(mockState.mediaDataSupportLoad).not.toHaveBeenCalled();
	});

	it("loads support config after auth is ready and does not revalidate public config on an interval", async () => {
		vi.useFakeTimers();
		Object.defineProperty(document, "visibilityState", {
			configurable: true,
			value: "visible",
		});
		mockState.authStore.isAuthenticated = true;
		mockState.authStore.isChecking = false;

		render(<App />);

		await vi.waitFor(() => {
			expect(mockState.frontendConfigLoad).toHaveBeenCalledTimes(1);
		});
		await vi.advanceTimersByTimeAsync(300_000);

		expect(mockState.frontendConfigLoad).toHaveBeenCalledTimes(1);
		expect(mockState.warmupRouteChunks).toHaveBeenCalledWith("user");
		await vi.waitFor(() => {
			expect(
				mockState.ensureAuthenticatedShellI18nNamespaces,
			).toHaveBeenCalledTimes(1);
			expect(mockState.previewAppsLoad).toHaveBeenCalledTimes(1);
			expect(mockState.thumbnailSupportLoad).toHaveBeenCalledTimes(1);
			expect(mockState.mediaDataSupportLoad).toHaveBeenCalledTimes(1);
		});
	});

	it("holds authenticated routes until the authenticated shell locale bundle is ready", async () => {
		let resolveLocale!: () => void;
		mockState.ensureAuthenticatedShellI18nNamespaces.mockReturnValueOnce(
			new Promise<void>((resolve) => {
				resolveLocale = resolve;
			}),
		);
		mockState.authStore.isAuthenticated = true;
		mockState.authStore.isChecking = false;

		render(<App />);

		expect(screen.queryByTestId("router-provider")).not.toBeInTheDocument();
		expect(mockState.warmupRouteChunks).not.toHaveBeenCalled();

		resolveLocale();

		await waitFor(() => {
			expect(screen.getByTestId("router-provider")).toBeInTheDocument();
			expect(mockState.warmupRouteChunks).toHaveBeenCalledWith("user");
		});
	});

	it("holds cached authenticated routes while the bootstrap auth check is still running", async () => {
		mockState.authStore.isAuthenticated = true;
		mockState.authStore.isChecking = true;

		const { rerender } = render(<App />);

		expect(screen.queryByTestId("router-provider")).not.toBeInTheDocument();
		expect(
			mockState.ensureAuthenticatedShellI18nNamespaces,
		).not.toHaveBeenCalled();
		expect(mockState.warmupRouteChunks).not.toHaveBeenCalled();

		mockState.authStore.isChecking = false;
		rerender(<App />);

		await waitFor(() => {
			expect(
				mockState.ensureAuthenticatedShellI18nNamespaces,
			).toHaveBeenCalledTimes(1);
			expect(screen.getByTestId("router-provider")).toBeInTheDocument();
			expect(mockState.warmupRouteChunks).toHaveBeenCalledWith("user");
		});
	});

	it("mounts authenticated routes when authenticated shell locale loading fails", async () => {
		const error = new Error("locale load failed");
		mockState.ensureAuthenticatedShellI18nNamespaces.mockRejectedValueOnce(
			error,
		);
		mockState.authStore.isAuthenticated = true;
		mockState.authStore.isChecking = false;

		render(<App />);

		await waitFor(() => {
			expect(screen.getByTestId("router-provider")).toBeInTheDocument();
			expect(mockState.warmupRouteChunks).toHaveBeenCalledWith("user");
		});
		expect(mockState.loggerWarn).toHaveBeenCalledWith(
			"failed to load authenticated locale namespaces",
			error,
		);
	});

	it("renders the offline boot fallback instead of the router", async () => {
		mockState.authStore.bootOffline = true;

		render(<App />);

		expect(await screen.findByTestId("offline-fallback")).toBeInTheDocument();
		expect(screen.queryByTestId("router-provider")).not.toBeInTheDocument();
		expect(screen.getByTestId("toaster")).toBeInTheDocument();
		expect(screen.queryByTestId("music-player-host")).not.toBeInTheDocument();
	});

	it("defers redirect handling while auth is still checking", async () => {
		mockState.authStore.isAuthenticated = true;
		mockState.authStore.isChecking = true;
		window.history.replaceState({}, "", "/?external_auth=success");

		render(<App />);
		await Promise.resolve();

		expect(mockState.toastSuccess).not.toHaveBeenCalled();
		expect(window.location.search).toBe("?external_auth=success");
	});

	it("shows and consumes the external auth success redirect toast after auth is ready", async () => {
		mockState.authStore.isAuthenticated = true;
		mockState.authStore.isChecking = false;
		window.history.replaceState(
			{ preserved: true },
			"",
			"/?external_auth=success&view=grid#files",
		);

		render(<App />);

		await waitFor(() => {
			expect(mockState.toastSuccess).toHaveBeenCalledWith(
				"auth:login_success",
				{
					id: "external-auth-login-success",
				},
			);
		});
		expect(window.location.pathname).toBe("/");
		expect(window.location.search).toBe("?view=grid");
		expect(window.location.hash).toBe("#files");
	});

	it("removes the external auth success query when it is the only search parameter", async () => {
		mockState.authStore.isAuthenticated = true;
		mockState.authStore.isChecking = false;
		window.history.replaceState({}, "", "/tasks?external_auth=success");

		render(<App />);

		await waitFor(() => {
			expect(mockState.toastSuccess).toHaveBeenCalledWith(
				"auth:login_success",
				{
					id: "external-auth-login-success",
				},
			);
		});
		expect(window.location.pathname).toBe("/tasks");
		expect(window.location.search).toBe("");
	});

	it("warms route chunks after auth is ready", async () => {
		mockState.authStore.isAuthenticated = true;
		mockState.authStore.isChecking = false;

		render(<App />);

		await waitFor(() => {
			expect(mockState.warmupRouteChunks).toHaveBeenCalledWith("user");
		});

		expect(screen.queryByTestId("music-player-host")).not.toBeInTheDocument();
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

	it("loads the music player host only after it is requested", async () => {
		mockState.musicPlayerHostMountRequested = true;

		render(<App />);

		await waitFor(() => {
			expect(screen.getByTestId("music-player-host")).toBeInTheDocument();
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
