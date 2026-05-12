import { beforeEach, describe, expect, it, vi } from "vitest";

const mockState = vi.hoisted(() => ({
	applyFilePrefs: vi.fn(),
	applyThemePrefs: vi.fn(),
	cancelPreferenceSync: vi.fn(),
	changeLanguage: vi.fn(async () => undefined),
	isAxiosError: vi.fn(),
	login: vi.fn(),
	logout: vi.fn(),
	me: vi.fn(),
	refreshToken: vi.fn(),
	warn: vi.fn(),
}));

vi.mock("axios", () => ({
	default: {
		isAxiosError: mockState.isAxiosError,
	},
}));

vi.mock("@/i18n", () => ({
	default: {
		changeLanguage: mockState.changeLanguage,
	},
}));

vi.mock("@/lib/logger", () => ({
	logger: {
		warn: mockState.warn,
	},
}));

vi.mock("@/lib/preferenceSync", () => ({
	cancelPreferenceSync: mockState.cancelPreferenceSync,
}));

vi.mock("@/services/authService", () => ({
	authService: {
		login: mockState.login,
		logout: mockState.logout,
		me: mockState.me,
		refreshToken: mockState.refreshToken,
	},
}));

vi.mock("@/stores/fileStore", () => ({
	useFileStore: {
		getState: () => ({
			viewMode: "list",
			browserOpenMode: "single_click",
			sortBy: "name",
			sortOrder: "asc",
			_applyFromServer: mockState.applyFilePrefs,
		}),
	},
}));

vi.mock("@/stores/themeStore", () => ({
	useThemeStore: {
		getState: () => ({
			mode: "system",
			colorPreset: "#2563eb",
			_applyFromServer: mockState.applyThemePrefs,
		}),
	},
}));

function createCachedUser() {
	return {
		id: 1,
		username: "cached-user",
		email: "cached@example.com",
	};
}

async function loadStore() {
	vi.resetModules();
	return await import("@/stores/authStore");
}

describe("useAuthStore edge cases", () => {
	beforeEach(() => {
		localStorage.clear();
		sessionStorage.clear();
		mockState.applyFilePrefs.mockReset();
		mockState.applyThemePrefs.mockReset();
		mockState.cancelPreferenceSync.mockReset();
		mockState.changeLanguage.mockReset();
		mockState.isAxiosError.mockReset();
		mockState.login.mockReset();
		mockState.logout.mockReset();
		mockState.me.mockReset();
		mockState.refreshToken.mockReset();
		mockState.warn.mockReset();
	});

	it("clears local auth state even when logout fails", async () => {
		localStorage.setItem(
			"aster-cached-user",
			JSON.stringify(createCachedUser()),
		);
		mockState.logout.mockRejectedValue(new Error("logout failed"));
		const { useAuthStore } = await loadStore();

		await useAuthStore.getState().logout();

		expect(mockState.cancelPreferenceSync).toHaveBeenCalledTimes(1);
		expect(useAuthStore.getState()).toMatchObject({
			isAuthenticated: false,
			isChecking: false,
			isAuthStale: false,
			bootOffline: false,
			user: null,
		});
		expect(localStorage.getItem("aster-cached-user")).toBeNull();
		expect(sessionStorage.getItem("aster-auth-expires-at")).toBeNull();
	});

	it("boots into offline mode when auth check fails without a cached user", async () => {
		mockState.me.mockRejectedValue(new Error("offline"));
		mockState.isAxiosError.mockReturnValue(false);
		const { useAuthStore } = await loadStore();

		await useAuthStore.getState().checkAuth();

		expect(useAuthStore.getState()).toMatchObject({
			isAuthenticated: false,
			isChecking: false,
			isAuthStale: false,
			bootOffline: true,
			user: null,
		});
	});

	it("clears cached auth state on server-side auth failures", async () => {
		localStorage.setItem(
			"aster-cached-user",
			JSON.stringify(createCachedUser()),
		);
		mockState.me.mockRejectedValue({
			response: { status: 401 },
		});
		mockState.isAxiosError.mockReturnValue(true);
		const { useAuthStore } = await loadStore();

		await useAuthStore.getState().checkAuth();

		expect(useAuthStore.getState()).toMatchObject({
			isAuthenticated: false,
			isChecking: false,
			isAuthStale: false,
			bootOffline: false,
			user: null,
		});
		expect(localStorage.getItem("aster-cached-user")).toBeNull();
	});

	it("logs a warning when refreshUser fails", async () => {
		localStorage.setItem(
			"aster-cached-user",
			JSON.stringify(createCachedUser()),
		);
		const failure = new Error("refresh failed");
		mockState.me.mockRejectedValue(failure);
		const { useAuthStore } = await loadStore();

		await useAuthStore.getState().refreshUser();

		expect(mockState.warn).toHaveBeenCalledWith("refreshUser failed", failure);
	});

	it("clears local session state when refresh fails with an auth response", async () => {
		localStorage.setItem(
			"aster-cached-user",
			JSON.stringify(createCachedUser()),
		);
		sessionStorage.setItem(
			"aster-auth-expires-at",
			String(Date.now() + 60_000),
		);
		mockState.refreshToken.mockRejectedValue({
			response: { status: 401 },
		});
		mockState.isAxiosError.mockReturnValue(true);
		const { useAuthStore } = await loadStore();

		await expect(useAuthStore.getState().refreshToken()).rejects.toEqual(
			expect.objectContaining({
				response: { status: 401 },
			}),
		);
		expect(useAuthStore.getState()).toMatchObject({
			isAuthenticated: false,
			isChecking: false,
			isAuthStale: false,
			bootOffline: false,
			user: null,
			expiresAt: null,
		});
		expect(localStorage.getItem("aster-cached-user")).toBeNull();
		expect(sessionStorage.getItem("aster-auth-expires-at")).toBeNull();
	});

	it("forceLogout clears cached auth artifacts", async () => {
		localStorage.setItem(
			"aster-cached-user",
			JSON.stringify(createCachedUser()),
		);
		sessionStorage.setItem(
			"aster-auth-expires-at",
			String(Date.now() + 60_000),
		);
		const { forceLogout, useAuthStore } = await loadStore();

		forceLogout();

		expect(useAuthStore.getState()).toMatchObject({
			isAuthenticated: false,
			user: null,
			expiresAt: null,
		});
		expect(localStorage.getItem("aster-cached-user")).toBeNull();
		expect(sessionStorage.getItem("aster-auth-expires-at")).toBeNull();
	});
});
