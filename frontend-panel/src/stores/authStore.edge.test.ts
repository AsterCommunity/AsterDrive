import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiErrorCode } from "@/types/api-helpers";

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
		access_token_expires_at: Math.floor(Date.now() / 1000) + 60,
	};
}

function tokenApiError(code = ApiErrorCode.TokenInvalid) {
	return {
		code,
		message:
			code === ApiErrorCode.TokenInvalid
				? "session revoked"
				: code === ApiErrorCode.TokenMissing
					? "missing token"
					: "token expired",
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

	it("keeps cached auth state stale on transient auth check responses", async () => {
		const cachedUser = createCachedUser();
		localStorage.setItem("aster-cached-user", JSON.stringify(cachedUser));
		sessionStorage.setItem(
			"aster-auth-expires-at",
			String(Date.now() + 60_000),
		);
		mockState.me.mockRejectedValue({
			response: { status: 502 },
		});
		mockState.isAxiosError.mockReturnValue(true);
		const { useAuthStore } = await loadStore();

		await useAuthStore.getState().checkAuth();

		expect(useAuthStore.getState()).toMatchObject({
			isAuthenticated: true,
			isChecking: false,
			isAuthStale: true,
			bootOffline: false,
			user: cachedUser,
		});
		expect(localStorage.getItem("aster-cached-user")).toBe(
			JSON.stringify(cachedUser),
		);
		useAuthStore.getState().stopAutoRefresh();
	});

	it("clears cached auth state on token ApiError auth checks", async () => {
		localStorage.setItem(
			"aster-cached-user",
			JSON.stringify(createCachedUser()),
		);
		sessionStorage.setItem(
			"aster-auth-expires-at",
			String(Date.now() + 60_000),
		);
		mockState.me.mockRejectedValue(tokenApiError(ApiErrorCode.TokenMissing));
		mockState.isAxiosError.mockReturnValue(false);
		const { useAuthStore } = await loadStore();

		await useAuthStore.getState().checkAuth();

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

	it("does not refresh an already fresh session", async () => {
		const { SESSION_REFRESH_THRESHOLD_MS, useAuthStore } = await loadStore();
		useAuthStore.setState({
			expiresAt: Date.now() + SESSION_REFRESH_THRESHOLD_MS * 2,
			isAuthenticated: true,
			isAuthStale: false,
		});

		await useAuthStore.getState().ensureFreshSession();

		expect(mockState.refreshToken).not.toHaveBeenCalled();
		useAuthStore.getState().stopAutoRefresh();
	});

	it("refreshes stale or nearly expired sessions on demand", async () => {
		mockState.refreshToken.mockResolvedValue({ expiresIn: 900 });
		vi.useFakeTimers();
		vi.setSystemTime(new Date("2026-01-01T00:00:00.000Z"));
		const { SESSION_REFRESH_THRESHOLD_MS, useAuthStore } = await loadStore();
		useAuthStore.setState({
			expiresAt: Date.now() + 15_000,
			isAuthenticated: true,
			isAuthStale: false,
		});

		try {
			await useAuthStore.getState().ensureFreshSession();

			expect(mockState.refreshToken).toHaveBeenCalledTimes(1);
			expect(useAuthStore.getState().expiresAt).toBe(Date.now() + 900_000);
			expect(useAuthStore.getState().isAuthStale).toBe(false);

			vi.advanceTimersByTime(900_000 - SESSION_REFRESH_THRESHOLD_MS + 1);
			await useAuthStore.getState().ensureFreshSession();

			expect(mockState.refreshToken).toHaveBeenCalledTimes(2);
			useAuthStore.getState().stopAutoRefresh();
		} finally {
			vi.useRealTimers();
		}
	});

	it("rejects MFA-required login responses before syncing a session", async () => {
		mockState.login.mockResolvedValue({
			status: "mfa_required",
			expiresIn: 300,
			flowToken: "mfa-flow",
			methods: ["totp"],
		});
		const { useAuthStore } = await loadStore();

		await expect(
			useAuthStore.getState().login("alice@example.com", "secret"),
		).rejects.toThrow("MFA verification is required before session sync");
		expect(mockState.me).not.toHaveBeenCalled();
		expect(useAuthStore.getState()).toMatchObject({
			isAuthenticated: false,
			user: null,
		});
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
		useAuthStore.setState({ isAuthStale: false });

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

	it("keeps local session state when refresh fails with a transient gateway response", async () => {
		const cachedUser = createCachedUser();
		localStorage.setItem("aster-cached-user", JSON.stringify(cachedUser));
		sessionStorage.setItem(
			"aster-auth-expires-at",
			String(Date.now() + 60_000),
		);
		mockState.refreshToken.mockRejectedValue({
			response: { status: 502 },
		});
		mockState.isAxiosError.mockReturnValue(true);
		const { useAuthStore } = await loadStore();

		await expect(useAuthStore.getState().refreshToken()).rejects.toEqual(
			expect.objectContaining({
				response: { status: 502 },
			}),
		);
		expect(useAuthStore.getState()).toMatchObject({
			isAuthenticated: true,
			isAuthStale: true,
			bootOffline: false,
			user: cachedUser,
		});
		expect(localStorage.getItem("aster-cached-user")).toBe(
			JSON.stringify(cachedUser),
		);
		expect(sessionStorage.getItem("aster-auth-expires-at")).not.toBeNull();
		useAuthStore.getState().stopAutoRefresh();
	});

	it("clears local session state when refresh fails with a token ApiError", async () => {
		localStorage.setItem(
			"aster-cached-user",
			JSON.stringify(createCachedUser()),
		);
		sessionStorage.setItem(
			"aster-auth-expires-at",
			String(Date.now() + 60_000),
		);
		mockState.refreshToken.mockRejectedValue(tokenApiError());
		mockState.isAxiosError.mockReturnValue(false);
		const { useAuthStore } = await loadStore();

		await expect(useAuthStore.getState().refreshToken()).rejects.toEqual(
			expect.objectContaining({
				code: ApiErrorCode.TokenInvalid,
				message: "session revoked",
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

	it("recovers a stale refresh by syncing the current session", async () => {
		const cachedUser = createCachedUser();
		localStorage.setItem("aster-cached-user", JSON.stringify(cachedUser));
		sessionStorage.setItem(
			"aster-auth-expires-at",
			String(Date.now() + 60_000),
		);
		const nextExpiresAt = Math.floor(Date.now() / 1000) + 900;
		mockState.refreshToken.mockRejectedValue({
			code: ApiErrorCode.RefreshTokenStale,
			message: "stale refresh token",
		});
		mockState.me.mockResolvedValue({
			...cachedUser,
			access_token_expires_at: nextExpiresAt,
		});
		mockState.isAxiosError.mockReturnValue(false);
		const { useAuthStore } = await loadStore();

		await expect(
			useAuthStore.getState().refreshToken(),
		).resolves.toBeUndefined();

		expect(mockState.me).toHaveBeenCalledWith(["session"]);
		expect(useAuthStore.getState()).toMatchObject({
			isAuthenticated: true,
			isAuthStale: false,
			bootOffline: false,
			expiresAt: nextExpiresAt * 1000,
		});
		expect(sessionStorage.getItem("aster-auth-expires-at")).toBe(
			String(nextExpiresAt * 1000),
		);
		expect(
			JSON.parse(localStorage.getItem("aster-cached-user") ?? "{}"),
		).toMatchObject({
			access_token_expires_at: nextExpiresAt,
		});
		useAuthStore.getState().stopAutoRefresh();
	});

	it("clears local session state when a peer reports refresh auth failure", async () => {
		localStorage.setItem(
			"aster-cached-user",
			JSON.stringify(createCachedUser()),
		);
		sessionStorage.setItem(
			"aster-auth-expires-at",
			String(Date.now() + 60_000),
		);
		localStorage.setItem(
			"aster-auth-refresh-lock",
			JSON.stringify({
				ownerId: "peer-tab",
				lockId: "peer-lock",
				expiresAt: Date.now() + 15_000,
				updatedAt: Date.now(),
			}),
		);
		const { useAuthStore } = await loadStore();

		const refresh = useAuthStore.getState().refreshToken();
		window.dispatchEvent(
			new StorageEvent("storage", {
				key: "aster-auth-refresh-event",
				newValue: JSON.stringify({
					ownerId: "peer-tab",
					lockId: "peer-lock",
					status: "failure",
					failureKind: "auth",
					createdAt: Date.now(),
				}),
			}),
		);

		await expect(refresh).rejects.toThrow("peer auth refresh failed");
		expect(mockState.refreshToken).not.toHaveBeenCalled();
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

	it("logs auto refresh failures for immediate and scheduled retries", async () => {
		vi.useFakeTimers();
		const immediateFailure = new Error("immediate refresh failed");
		const scheduledFailure = new Error("scheduled refresh failed");
		mockState.refreshToken
			.mockRejectedValueOnce(immediateFailure)
			.mockRejectedValueOnce(scheduledFailure);
		mockState.isAxiosError.mockReturnValue(false);
		const { useAuthStore } = await loadStore();

		useAuthStore.getState().startAutoRefresh(-1);
		await vi.waitFor(() => {
			expect(mockState.warn).toHaveBeenCalledWith(
				"auto refresh failed",
				immediateFailure,
			);
		});

		expect(vi.getTimerCount()).toBe(1);

		useAuthStore.getState().startAutoRefresh(1);
		await vi.advanceTimersByTimeAsync(1);

		expect(mockState.warn).toHaveBeenCalledWith(
			"auto refresh failed",
			scheduledFailure,
		);
		expect(mockState.refreshToken).toHaveBeenCalledTimes(2);
		useAuthStore.getState().stopAutoRefresh();
		vi.useRealTimers();
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
