import { render } from "@testing-library/react";
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
	setAuthState: vi.fn(),
	themeInit: vi.fn(),
	thumbnailSupportLoad: vi.fn(),
}));

vi.mock("react-router-dom", () => ({
	RouterProvider: () => <div data-testid="router-provider" />,
}));

vi.mock("sonner", () => ({
	Toaster: () => <div data-testid="toaster" />,
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

vi.mock("@/components/layout/OfflineBootFallback", () => ({
	OfflineBootFallback: () => <div data-testid="offline-fallback" />,
}));

vi.mock("@/stores/brandingStore", () => ({
	useBrandingStore: {
		getState: () => ({
			load: mockState.brandingLoad,
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
		mockState.setAuthState.mockReset();
		mockState.themeInit.mockReset();
		mockState.thumbnailSupportLoad.mockReset();
		vi.useRealTimers();
	});

	afterEach(() => {
		window.history.replaceState({}, "", "/");
	});

	it("skips the bootstrap auth check on login", () => {
		window.history.replaceState({}, "", "/login");

		render(<App />);

		expect(mockState.previewAppsLoad).toHaveBeenCalledTimes(1);
		expect(mockState.thumbnailSupportLoad).toHaveBeenCalledTimes(1);
		expect(mockState.authStore.checkAuth).not.toHaveBeenCalled();
		expect(mockState.setAuthState).toHaveBeenCalledWith({ isChecking: false });
	});

	it("runs the bootstrap auth check on protected routes", () => {
		window.history.replaceState({}, "", "/");

		render(<App />);

		expect(mockState.previewAppsLoad).toHaveBeenCalledTimes(1);
		expect(mockState.thumbnailSupportLoad).toHaveBeenCalledTimes(1);
		expect(mockState.authStore.checkAuth).toHaveBeenCalledTimes(1);
		expect(mockState.setAuthState).not.toHaveBeenCalled();
	});

	it("revalidates public config when the tab becomes visible again", () => {
		render(<App />);

		expect(mockState.brandingLoad).toHaveBeenCalledTimes(1);
		expect(mockState.previewAppsLoad).toHaveBeenCalledTimes(1);
		expect(mockState.thumbnailSupportLoad).toHaveBeenCalledTimes(1);

		Object.defineProperty(document, "visibilityState", {
			configurable: true,
			value: "visible",
		});
		document.dispatchEvent(new Event("visibilitychange"));

		expect(mockState.brandingLoad).toHaveBeenCalledTimes(2);
		expect(mockState.previewAppsLoad).toHaveBeenCalledTimes(2);
		expect(mockState.thumbnailSupportLoad).toHaveBeenCalledTimes(2);
	});

	it("revalidates public config on the interval while visible", () => {
		vi.useFakeTimers();
		Object.defineProperty(document, "visibilityState", {
			configurable: true,
			value: "visible",
		});

		render(<App />);

		vi.advanceTimersByTime(60_000);

		expect(mockState.brandingLoad).toHaveBeenCalledTimes(2);
		expect(mockState.previewAppsLoad).toHaveBeenCalledTimes(2);
		expect(mockState.thumbnailSupportLoad).toHaveBeenCalledTimes(2);
	});
});
