import { create } from "zustand";
import i18n from "@/i18n";
import {
	isSessionAuthFailure,
	isStaleRefreshTokenError,
} from "@/lib/authErrors";
import {
	isCrossTabRefreshAuthFailure,
	runWithCrossTabRefreshLock,
} from "@/lib/crossTabRefresh";
import { logger } from "@/lib/logger";
import { cancelPreferenceSync } from "@/lib/preferenceSync";
import { authService } from "@/services/authService";
import { useDisplayTimeZoneStore } from "@/stores/displayTimeZoneStore";
import { useFileStore } from "@/stores/fileStore";
import { useTeamStore } from "@/stores/teamStore";
import { useThemeStore } from "@/stores/themeStore";
import type {
	MeField,
	MePartialResponse,
	MeResponse,
	UserPreferences,
} from "@/types/api";

const CACHED_USER_KEY = "aster-cached-user";
const EXPIRES_AT_KEY = "aster-auth-expires-at";
const REFRESH_BUFFER_MS = 120_000;
const REFRESH_RETRY_MS = 60_000;
export const SESSION_REFRESH_THRESHOLD_MS = 30_000;

let refreshTimer: ReturnType<typeof setTimeout> | null = null;
let inFlightRefresh: Promise<void> | null = null;
let inFlightFullRefreshUser: Promise<void> | null = null;

interface RefreshUserOptions {
	fields?: MeField[];
}

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
	user: { access_token_expires_at?: number | null } | null,
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
	ensureFreshSession: () => Promise<void>;
	refreshToken: () => Promise<void>;
	refreshUser: (options?: RefreshUserOptions) => Promise<void>;
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
	void import("@/hooks/useBlobUrl")
		.then(({ clearBlobUrlCache, clearPersistedBlobUrlCache }) => {
			clearBlobUrlCache();
			void clearPersistedBlobUrlCache();
		})
		.catch(() => {
			// 登出清理不能被可选的缩略图缓存清理阻塞。
		});
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

function mergePartialUser(
	current: MeResponse | null,
	partial: MePartialResponse,
	fields: MeField[],
): MeResponse | null {
	if (!current) return null;

	const fieldSet = new Set(fields);
	return {
		...current,
		id: partial.id,
		username: partial.username,
		email: partial.email,
		email_verified: partial.email_verified,
		pending_email: partial.pending_email,
		role: partial.role,
		status: partial.status,
		policy_group_id: partial.policy_group_id,
		created_at: partial.created_at,
		updated_at: partial.updated_at,
		storage_used: fieldSet.has("quota")
			? (partial.storage_used ?? current.storage_used)
			: current.storage_used,
		storage_quota: fieldSet.has("quota")
			? (partial.storage_quota ?? current.storage_quota)
			: current.storage_quota,
		access_token_expires_at: fieldSet.has("session")
			? (partial.access_token_expires_at ?? current.access_token_expires_at)
			: current.access_token_expires_at,
		preferences: fieldSet.has("preferences")
			? (partial.preferences ?? null)
			: current.preferences,
		profile: fieldSet.has("profile")
			? (partial.profile ?? current.profile)
			: current.profile,
	};
}

function updateCachedSessionExpiry(expiresAt: number) {
	const cached = getCachedUser();
	const expiresAtSeconds = Math.floor(expiresAt / 1000);
	if (cached) {
		setCachedUser({
			...cached,
			access_token_expires_at: expiresAtSeconds,
		});
	}
	return expiresAtSeconds;
}

async function syncSessionFromMe(
	getState: () => AuthState,
	setAuthState: (state: Partial<AuthState>) => void,
) {
	const user = await authService.me(["session"]);
	const expiresAt = getExpiresAtFromUser(user) ?? Date.now() + REFRESH_RETRY_MS;
	const expiresAtSeconds = updateCachedSessionExpiry(expiresAt);
	const currentUser = getState().user;
	setStoredExpiresAt(expiresAt);
	setAuthState({
		expiresAt,
		isAuthenticated: true,
		isAuthStale: false,
		bootOffline: false,
		user: currentUser
			? {
					...currentUser,
					access_token_expires_at: expiresAtSeconds,
				}
			: null,
	});
	getState().startAutoRefresh();
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
		if (
			session.status !== "authenticated" &&
			session.status !== "password_change_required"
		) {
			throw new Error("MFA verification is required before session sync");
		}
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
			if (!isSessionAuthFailure(error)) {
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

	ensureFreshSession: async () => {
		const { expiresAt, isAuthenticated, isAuthStale } = get();
		if (!isAuthenticated) return;
		if (
			isAuthStale ||
			!expiresAt ||
			expiresAt - Date.now() <= SESSION_REFRESH_THRESHOLD_MS
		) {
			await get().refreshToken();
		}
	},

	refreshToken: async () => {
		if (inFlightRefresh) return inFlightRefresh;

		inFlightRefresh = (async () => {
			try {
				const refreshedLocally = await runWithCrossTabRefreshLock(
					async () => {
						try {
							const session = await authService.refreshToken();
							get().syncSession(session.expiresIn);
						} catch (error) {
							if (!isStaleRefreshTokenError(error)) {
								throw error;
							}
							await syncSessionFromMe(get, set);
						}
					},
					{
						classifyError: (error) =>
							isStaleRefreshTokenError(error) || isSessionAuthFailure(error)
								? "auth"
								: "transient",
					},
				);
				if (!refreshedLocally) {
					await syncSessionFromMe(get, set);
				}
			} catch (error) {
				if (
					isCrossTabRefreshAuthFailure(error) ||
					isSessionAuthFailure(error)
				) {
					applyLoggedOutState(set);
				} else {
					set({ isAuthStale: true });
					get().startAutoRefresh(REFRESH_RETRY_MS);
				}
				throw error;
			} finally {
				inFlightRefresh = null;
			}
		})();

		return inFlightRefresh;
	},

	refreshUser: async (options) => {
		const selectedFields =
			options?.fields && options.fields.length > 0 ? options.fields : null;
		const isPartialRefresh = selectedFields !== null;
		if (!isPartialRefresh && inFlightFullRefreshUser) {
			return inFlightFullRefreshUser;
		}

		const refresh = (async () => {
			try {
				const response = isPartialRefresh
					? await authService.me(selectedFields)
					: await authService.me();
				const user = isPartialRefresh
					? mergePartialUser(
							get().user,
							response as MePartialResponse,
							selectedFields,
						)
					: (response as MeResponse);
				if (!user) return;

				const expiresAt = selectedFields?.includes("session")
					? (getExpiresAtFromUser(user) ??
						get().expiresAt ??
						getStoredExpiresAt())
					: (getExpiresAtFromUser(user) ??
						get().expiresAt ??
						getStoredExpiresAt());
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
				if (!isPartialRefresh) {
					inFlightFullRefreshUser = null;
				}
			}
		})();

		if (!isPartialRefresh) {
			inFlightFullRefreshUser = refresh;
		}
		return refresh;
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
