import axios from "axios";
import { create } from "zustand";
import i18n from "@/i18n";
import { logger } from "@/lib/logger";
import { cancelPreferenceSync } from "@/lib/preferenceSync";
import { authService } from "@/services/authService";
import { useDisplayTimeZoneStore } from "@/stores/displayTimeZoneStore";
import { useFileStore } from "@/stores/fileStore";
import { useTeamStore } from "@/stores/teamStore";
import { useThemeStore } from "@/stores/themeStore";
import type { MeResponse, UserPreferences } from "@/types/api";

const CACHED_USER_KEY = "aster-cached-user";
const EXPIRES_AT_KEY = "aster-auth-expires-at";
const REFRESH_BUFFER_MS = 120_000;
const REFRESH_RETRY_MS = 60_000;

let refreshTimer: ReturnType<typeof setTimeout> | null = null;
let inFlightRefresh: Promise<void> | null = null;
let inFlightRefreshUser: Promise<void> | null = null;

function getCachedUser(): MeResponse | null {
	try {
		const raw = localStorage.getItem(CACHED_USER_KEY);
		return raw ? JSON.parse(raw) : null;
	} catch {
		return null;
	}
}

function setCachedUser(user: MeResponse | null) {
	if (user) {
		localStorage.setItem(CACHED_USER_KEY, JSON.stringify(user));
	} else {
		localStorage.removeItem(CACHED_USER_KEY);
	}
}

function getExpiresAtFromUser(
	user: Pick<MeResponse, "access_token_expires_at"> | null,
) {
	const expiresAtSeconds = Number(user?.access_token_expires_at);
	if (!Number.isFinite(expiresAtSeconds) || expiresAtSeconds <= 0) {
		return null;
	}
	return expiresAtSeconds * 1000;
}

function getStoredExpiresAt(): number | null {
	try {
		const raw = sessionStorage.getItem(EXPIRES_AT_KEY);
		if (!raw) return null;

		const expiresAt = Number(raw);
		if (Number.isNaN(expiresAt) || expiresAt <= Date.now()) {
			sessionStorage.removeItem(EXPIRES_AT_KEY);
			return null;
		}

		return expiresAt;
	} catch {
		return null;
	}
}

function setStoredExpiresAt(expiresAt: number | null) {
	try {
		if (expiresAt === null) {
			sessionStorage.removeItem(EXPIRES_AT_KEY);
			return;
		}
		sessionStorage.setItem(EXPIRES_AT_KEY, String(expiresAt));
	} catch {
		// ignore storage failures
	}
}

function clearRefreshTimer() {
	if (refreshTimer !== null) {
		clearTimeout(refreshTimer);
		refreshTimer = null;
	}
}

function applyServerPreferences(prefs: UserPreferences): void {
	const themeStore = useThemeStore.getState();
	const fileStore = useFileStore.getState();
	const displayTimeZoneStore = useDisplayTimeZoneStore.getState();

	themeStore._applyFromServer({
		mode: prefs.theme_mode ?? themeStore.mode,
		colorPreset: prefs.color_preset ?? themeStore.colorPreset,
	});
	fileStore._applyFromServer({
		viewMode: prefs.view_mode ?? fileStore.viewMode,
		browserOpenMode: prefs.browser_open_mode ?? fileStore.browserOpenMode,
		sortBy: prefs.sort_by ?? fileStore.sortBy,
		sortOrder: prefs.sort_order ?? fileStore.sortOrder,
	});
	displayTimeZoneStore._applyFromServer(prefs.display_time_zone);
	if (prefs.language) void i18n.changeLanguage(prefs.language);
}

interface AuthState {
	isAuthenticated: boolean;
	isChecking: boolean;
	isAuthStale: boolean;
	bootOffline: boolean;
	user: MeResponse | null;
	expiresAt: number | null;
	login: (identifier: string, password: string) => Promise<void>;
	logout: () => Promise<void>;
	checkAuth: () => Promise<void>;
	refreshToken: () => Promise<void>;
	refreshUser: () => Promise<void>;
	setStorageEventStreamEnabled: (enabled: boolean) => void;
	syncSession: (expiresIn: number) => void;
	startAutoRefresh: (delayMs?: number) => void;
	stopAutoRefresh: () => void;
}

const initialCachedUser = getCachedUser();
const initialExpiresAt = getStoredExpiresAt();
const LOGGED_OUT_STATE = {
	isAuthenticated: false,
	isChecking: false,
	isAuthStale: false,
	bootOffline: false,
	user: null,
	expiresAt: null,
} satisfies Pick<
	AuthState,
	| "isAuthenticated"
	| "isChecking"
	| "isAuthStale"
	| "bootOffline"
	| "user"
	| "expiresAt"
>;

function applyLoggedOutState(
	setAuthState: (state: Partial<AuthState>) => void,
) {
	cancelPreferenceSync();
	clearRefreshTimer();
	// teamStore 是独立的子状态，登出时直接清空
	useTeamStore.getState().clear();
	setStoredExpiresAt(null);
	setCachedUser(null);
	setAuthState(LOGGED_OUT_STATE);
}

function mergeUserPreferences(
	user: MeResponse,
	patch: Partial<UserPreferences>,
): MeResponse {
	return {
		...user,
		preferences: {
			...(user.preferences ?? {}),
			...patch,
		},
	};
}

// ── Subscription: 用户偏好同步 ────────────────────────────────────────────────
//
// 当 user 对象变化且处于已认证状态时，将服务端偏好同步到 themeStore / fileStore。
// authStore 只管自身状态，跨 store 写入统一通过此 subscription 完成，
// 而不是在每个 login / checkAuth / refreshUser 中重复调用。
function handleAuthStateChange(state: AuthState, prevState: AuthState) {
	if (state.user !== prevState.user && state.isAuthenticated) {
		if (state.user?.preferences) {
			applyServerPreferences(state.user.preferences);
			return;
		}

		useDisplayTimeZoneStore.getState()._applyFromServer(undefined);
	}
}

export const useAuthStore = create<AuthState>((set, get) => ({
	isAuthenticated: initialCachedUser !== null,
	isChecking: true,
	isAuthStale: initialCachedUser !== null,
	bootOffline: false,
	user: initialCachedUser,
	expiresAt: initialExpiresAt,

	login: async (identifier, password) => {
		const session = await authService.login(identifier, password);
		const user = await authService.me();
		setCachedUser(user);
		set({
			isAuthenticated: true,
			isChecking: false,
			isAuthStale: false,
			bootOffline: false,
			user,
		});
		get().syncSession(session.expiresIn);
	},

	logout: async () => {
		get().stopAutoRefresh();
		try {
			await authService.logout();
		} catch {
			// logout 失败不阻塞
		}
		applyLoggedOutState(set);
	},

	checkAuth: async () => {
		set({ isChecking: true, bootOffline: false });
		try {
			const user = await authService.me();
			const expiresAt =
				getExpiresAtFromUser(user) ?? get().expiresAt ?? getStoredExpiresAt();
			setCachedUser(user);
			if (expiresAt !== null) setStoredExpiresAt(expiresAt);
			set({
				isAuthenticated: true,
				isChecking: false,
				isAuthStale: false,
				bootOffline: false,
				user,
				expiresAt,
			});

			if (!expiresAt || expiresAt - Date.now() <= REFRESH_BUFFER_MS) {
				try {
					await get().refreshToken();
				} catch (error) {
					logger.warn("checkAuth bootstrap refresh failed", error);
				}
			} else {
				get().startAutoRefresh();
			}
		} catch (error) {
			// 网络错误（离线）时用缓存的用户信息保持登录态
			if (!axios.isAxiosError(error) || !error.response) {
				const cached = getCachedUser();
				const expiresAt =
					getExpiresAtFromUser(cached) ??
					get().expiresAt ??
					getStoredExpiresAt();
				if (cached) {
					set({
						isAuthenticated: true,
						isChecking: false,
						isAuthStale: true,
						bootOffline: false,
						user: cached,
						expiresAt,
					});
					if (expiresAt) get().startAutoRefresh();
				} else {
					set({
						isAuthenticated: false,
						isChecking: false,
						isAuthStale: false,
						bootOffline: true,
						user: null,
						expiresAt: null,
					});
				}
				return;
			}
			applyLoggedOutState(set);
		}
	},

	refreshToken: async () => {
		if (inFlightRefresh) return inFlightRefresh;

		inFlightRefresh = (async () => {
			try {
				const session = await authService.refreshToken();
				get().syncSession(session.expiresIn);
			} catch (error) {
				if (axios.isAxiosError(error) && error.response) {
					applyLoggedOutState(set);
				} else {
					get().startAutoRefresh(REFRESH_RETRY_MS);
				}
				throw error;
			} finally {
				inFlightRefresh = null;
			}
		})();

		return inFlightRefresh;
	},

	refreshUser: async () => {
		if (inFlightRefreshUser) return inFlightRefreshUser;

		inFlightRefreshUser = (async () => {
			try {
				const user = await authService.me();
				const expiresAt =
					getExpiresAtFromUser(user) ?? get().expiresAt ?? getStoredExpiresAt();
				setCachedUser(user);
				if (expiresAt !== null) {
					setStoredExpiresAt(expiresAt);
					get().startAutoRefresh();
				}
				set({
					user,
					isAuthenticated: true,
					isAuthStale: false,
					bootOffline: false,
					expiresAt,
				});
			} catch (e) {
				logger.warn("refreshUser failed", e);
			} finally {
				inFlightRefreshUser = null;
			}
		})();

		return inFlightRefreshUser;
	},

	setStorageEventStreamEnabled: (enabled) => {
		const user = get().user;
		if (!user) return;

		const nextUser = mergeUserPreferences(user, {
			storage_event_stream_enabled: enabled,
		});
		setCachedUser(nextUser);
		set({ user: nextUser });
	},

	syncSession: (expiresIn) => {
		const expiresAt = Date.now() + expiresIn * 1000;
		setStoredExpiresAt(expiresAt);
		set({
			expiresAt,
			isAuthenticated: true,
			isAuthStale: false,
			bootOffline: false,
		});
		get().startAutoRefresh();
	},

	startAutoRefresh: (delayMs) => {
		clearRefreshTimer();

		const expiresAt = get().expiresAt;
		const refreshIn =
			delayMs ??
			(expiresAt ? expiresAt - Date.now() - REFRESH_BUFFER_MS : null);
		if (refreshIn === null) return;

		if (refreshIn <= 0) {
			void get()
				.refreshToken()
				.catch((error) => {
					logger.warn("auto refresh failed", error);
				});
			return;
		}

		refreshTimer = setTimeout(() => {
			void get()
				.refreshToken()
				.catch((error) => {
					logger.warn("auto refresh failed", error);
				});
		}, refreshIn);
	},

	stopAutoRefresh: () => {
		clearRefreshTimer();
	},
}));

// store 创建后注册订阅，避免 store 定义体内循环引用
useAuthStore.subscribe(handleAuthStateChange);

export function forceLogout() {
	applyLoggedOutState(useAuthStore.setState.bind(useAuthStore));
}
