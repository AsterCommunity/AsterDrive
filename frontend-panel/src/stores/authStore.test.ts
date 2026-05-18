import { HttpResponse, http } from "msw";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { apiResponse, createMeResponse } from "@/test/fixtures";
import { server } from "@/test/server";
import { ErrorCode } from "@/types/api-helpers";

const changeLanguage = vi.fn(async () => undefined);

vi.mock("@/i18n", () => ({
	default: {
		changeLanguage,
		language: "en",
	},
}));

async function loadStores() {
	vi.resetModules();
	const [
		{ useAuthStore },
		{ useDisplayTimeZoneStore },
		{ useFileStore },
		{ useThemeStore },
	] = await Promise.all([
		import("@/stores/authStore"),
		import("@/stores/displayTimeZoneStore"),
		import("@/stores/fileStore"),
		import("@/stores/themeStore"),
	]);

	return { useAuthStore, useDisplayTimeZoneStore, useFileStore, useThemeStore };
}

describe("useAuthStore", () => {
	beforeEach(() => {
		localStorage.clear();
		sessionStorage.clear();
		vi.useRealTimers();
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("logs in, caches the user, and applies server preferences", async () => {
		changeLanguage.mockClear();

		let loginPayload: unknown;
		const user = createMeResponse();

		server.use(
			http.post("*/api/v1/auth/login", async ({ request }) => {
				loginPayload = await request.json();
				return HttpResponse.json(apiResponse({ expires_in: 900 }));
			}),
			http.get("*/api/v1/auth/me", () => HttpResponse.json(apiResponse(user))),
		);

		const {
			useAuthStore,
			useDisplayTimeZoneStore,
			useFileStore,
			useThemeStore,
		} = await loadStores();

		await useAuthStore.getState().login("alice@example.com", "secret");

		expect(loginPayload).toEqual({
			identifier: "alice@example.com",
			password: "secret",
		});
		expect(useAuthStore.getState()).toMatchObject({
			isAuthenticated: true,
			isChecking: false,
			isAuthStale: false,
			bootOffline: false,
			user,
		});
		expect(useAuthStore.getState().expiresAt).toEqual(expect.any(Number));
		expect(localStorage.getItem("aster-cached-user")).not.toBeNull();
		expect(sessionStorage.getItem("aster-auth-expires-at")).not.toBeNull();
		expect(useThemeStore.getState()).toMatchObject({
			mode: "dark",
			colorPreset: "#f97316",
		});
		expect(useFileStore.getState()).toMatchObject({
			viewMode: "grid",
			browserOpenMode: "double_click",
			sortBy: "updated_at",
			sortOrder: "desc",
		});
		expect(useDisplayTimeZoneStore.getState().preference).toBe("Asia/Shanghai");
		expect(localStorage.getItem("aster-display-time-zone")).toBe(
			"Asia/Shanghai",
		);
		expect(changeLanguage).toHaveBeenCalledWith("zh");
	});

	it("rethrows pending-activation login responses as ApiError", async () => {
		server.use(
			http.post("*/api/v1/auth/login", () =>
				HttpResponse.json(
					{
						code: ErrorCode.PendingActivation,
						msg: "account pending activation",
						data: null,
					},
					{ status: 403 },
				),
			),
		);

		const { useAuthStore } = await loadStores();

		await expect(
			useAuthStore.getState().login("alice@example.com", "secret"),
		).rejects.toEqual(
			expect.objectContaining({
				code: ErrorCode.PendingActivation,
				message: "account pending activation",
			}),
		);
		expect(useAuthStore.getState()).toMatchObject({
			isAuthenticated: false,
			isChecking: true,
			isAuthStale: false,
			bootOffline: false,
			user: null,
		});
	});

	it("keeps the cached user when auth check fails offline", async () => {
		const cachedUser = createMeResponse({
			username: "offline-user",
			email: "offline@example.com",
		});
		localStorage.setItem("aster-cached-user", JSON.stringify(cachedUser));

		server.use(http.get("*/api/v1/auth/me", () => HttpResponse.error()));

		const { useAuthStore } = await loadStores();

		await useAuthStore.getState().checkAuth();

		expect(useAuthStore.getState()).toMatchObject({
			isAuthenticated: true,
			isChecking: false,
			isAuthStale: true,
			bootOffline: false,
			user: cachedUser,
		});
	});

	it("hydrates expiresAt from auth me when bootstrapping auth state", async () => {
		const accessTokenExpiresAt = Math.floor(Date.now() / 1000) + 900;
		const user = createMeResponse({
			access_token_expires_at: accessTokenExpiresAt,
		});

		server.use(
			http.get("*/api/v1/auth/me", () => HttpResponse.json(apiResponse(user))),
		);

		const { useAuthStore } = await loadStores();

		await useAuthStore.getState().checkAuth();

		expect(useAuthStore.getState().expiresAt).toBe(accessTokenExpiresAt * 1000);
		expect(sessionStorage.getItem("aster-auth-expires-at")).toBe(
			String(accessTokenExpiresAt * 1000),
		);
	});

	it("refreshes access token before expiry", async () => {
		vi.useFakeTimers();

		let refreshCount = 0;
		const user = createMeResponse();

		server.use(
			http.post("*/api/v1/auth/login", () =>
				HttpResponse.json(apiResponse({ expires_in: 900 })),
			),
			http.post("*/api/v1/auth/refresh", () => {
				refreshCount += 1;
				return HttpResponse.json(apiResponse({ expires_in: 900 }));
			}),
			http.get("*/api/v1/auth/me", () => HttpResponse.json(apiResponse(user))),
		);

		const { useAuthStore } = await loadStores();

		await useAuthStore.getState().login("alice@example.com", "secret");
		await vi.advanceTimersByTimeAsync(780_000);

		expect(refreshCount).toBe(1);
		useAuthStore.getState().stopAutoRefresh();
	});

	it("waits for another tab refresh and syncs the local session expiry", async () => {
		vi.useFakeTimers();

		let refreshCount = 0;
		const accessTokenExpiresAt = Math.floor(Date.now() / 1000) + 900;
		const user = createMeResponse({
			access_token_expires_at: accessTokenExpiresAt,
		});

		localStorage.setItem(
			"aster-auth-refresh-lock",
			JSON.stringify({
				ownerId: "peer-tab",
				lockId: "peer-lock",
				expiresAt: Date.now() + 15_000,
			}),
		);

		server.use(
			http.post("*/api/v1/auth/refresh", () => {
				refreshCount += 1;
				return HttpResponse.json(apiResponse({ expires_in: 900 }));
			}),
			http.get("*/api/v1/auth/me", ({ request }) => {
				const url = new URL(request.url);
				expect(url.searchParams.get("fields")).toBe("session");
				return HttpResponse.json(apiResponse(user));
			}),
		);

		const { useAuthStore } = await loadStores();

		const refresh = useAuthStore.getState().refreshToken();
		window.dispatchEvent(
			new StorageEvent("storage", {
				key: "aster-auth-refresh-event",
				newValue: JSON.stringify({
					ownerId: "peer-tab",
					lockId: "peer-lock",
					status: "success",
					createdAt: Date.now(),
				}),
			}),
		);

		await refresh;

		expect(refreshCount).toBe(0);
		expect(useAuthStore.getState().expiresAt).toBe(accessTokenExpiresAt * 1000);
		useAuthStore.getState().stopAutoRefresh();
	});

	it("updates the cached user when toggling storage event stream locally", async () => {
		const cachedUser = createMeResponse();
		localStorage.setItem("aster-cached-user", JSON.stringify(cachedUser));

		const { useAuthStore } = await loadStores();

		useAuthStore.getState().setStorageEventStreamEnabled(false);

		expect(
			useAuthStore.getState().user?.preferences?.storage_event_stream_enabled,
		).toBe(false);
		expect(
			JSON.parse(localStorage.getItem("aster-cached-user") ?? "{}").preferences
				?.storage_event_stream_enabled,
		).toBe(false);
	});

	it("resets the display time zone to browser default when the server has no preferences", async () => {
		const user = createMeResponse({ preferences: null });
		server.use(
			http.get("*/api/v1/auth/me", () => HttpResponse.json(apiResponse(user))),
		);
		const { useAuthStore, useDisplayTimeZoneStore } = await loadStores();

		useDisplayTimeZoneStore.getState()._applyFromServer("Asia/Shanghai");
		await useAuthStore.getState().checkAuth();

		expect(useDisplayTimeZoneStore.getState().preference).toBe("browser");
		expect(localStorage.getItem("aster-display-time-zone")).toBe("browser");
	});

	it("merges partial quota refreshes without dropping profile or preferences", async () => {
		const user = createMeResponse({
			storage_used: 10,
			storage_quota: 100,
		});
		server.use(
			http.get("*/api/v1/auth/me", ({ request }) => {
				const url = new URL(request.url);
				if (url.searchParams.get("fields") === "quota") {
					return HttpResponse.json(
						apiResponse({
							id: user.id,
							username: user.username,
							email: user.email,
							email_verified: user.email_verified,
							pending_email: user.pending_email,
							role: user.role,
							status: user.status,
							policy_group_id: user.policy_group_id,
							created_at: user.created_at,
							updated_at: user.updated_at,
							storage_used: 42,
							storage_quota: 100,
						}),
					);
				}
				return HttpResponse.json(apiResponse(user));
			}),
		);
		const { useAuthStore } = await loadStores();

		await useAuthStore.getState().checkAuth();
		await useAuthStore.getState().refreshUser({ fields: ["quota"] });

		expect(useAuthStore.getState().user).toMatchObject({
			storage_used: 42,
			storage_quota: 100,
			profile: user.profile,
			preferences: user.preferences,
		});
	});
});
