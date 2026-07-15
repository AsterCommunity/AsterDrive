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
import { useTeamStore } from "@/stores/teamStore";
import { useThemeStore } from "@/stores/themeStore";
import type {
	MeField,
	MePartialResponse,
	MeResponse,
	UserPreferences,
	UserProfileInfo,
} from "@/types/api";

const CACHED_USER_KEY = "aster-cached-user";
const EXPIRES_AT_KEY = "aster-auth-expires-at";
const THUMBNAIL_CACHE_NAMESPACE_KEY = "aster-thumbnail-cache-namespace";
const REFRESH_BUFFER_MS = 120_000;
const REFRESH_RETRY_MS = 60_000;
export const SESSION_REFRESH_THRESHOLD_MS = 30_000;

let refreshTimer: ReturnType<typeof setTimeout> | null = null;
let inFlightRefresh: Promise<void> | null = null;
let inFlightFullRefreshUser: Promise<void> | null = null;

interface RefreshUserOptions {
	fields?: MeField[];
}

type CachedUser = Partial<MeResponse> & {
	access_token_expires_at?: number | null;
	preferences?: UserPreferences | null;
	profile?: UserProfileInfo | null;
};

function sanitizeCachedUser(value: unknown): CachedUser | null {
	if (!value || typeof value !== "object") return null;
	const source = value as Partial<MeResponse>;
	const cached: CachedUser = {};
	if (source.profile) cached.profile = source.profile;
	if (source.preferences !== undefined) cached.preferences = source.preferences;
	if (source.access_token_expires_at !== undefined) {
		cached.access_token_expires_at = source.access_token_expires_at;
	}
	return Object.keys(cached).length > 0 ? cached : null;
}

function getCachedUser(): CachedUser | null {
	try {
		const raw = localStorage.getItem(CACHED_USER_KEY);
		if (!raw) return null;
		const cached = sanitizeCachedUser(JSON.parse(raw));
		if (cached) {
			localStorage.setItem(CACHED_USER_KEY, JSON.stringify(cached));
		} else {
			localStorage.removeItem(CACHED_USER_KEY);
		}
		return cached;
	} catch {
		return null;
	}
}

function setCachedUser(user: MeResponse | CachedUser | null) {
	const cached = sanitizeCachedUser(user);
	if (cached) {
		localStorage.setItem(CACHED_USER_KEY, JSON.stringify(cached));
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
			sessionStorage.removeItem(THUMBNAIL_CACHE_NAMESPACE_KEY);
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
			sessionStorage.removeItem(THUMBNAIL_CACHE_NAMESPACE_KEY);
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
	const displayTimeZoneStore = useDisplayTimeZoneStore.getState();

	themeStore._applyFromServer({
		mode: prefs.theme_mode ?? themeStore.mode,
		colorPreset: prefs.color_preset ?? themeStore.colorPreset,
	});
	void import("@/stores/fileStore").then(
		({ useFileStore }) => {
			const fileStore = useFileStore.getState();
			fileStore._applyFromServer({
				viewMode: prefs.view_mode ?? fileStore.viewMode,
				browserOpenMode: prefs.browser_open_mode ?? fileStore.browserOpenMode,
				sortBy: prefs.sort_by ?? fileStore.sortBy,
				sortOrder: prefs.sort_order ?? fileStore.sortOrder,
			});
		},
		(error) => {
			logger.warn("file browser preference bootstrap failed", error);
		},
	);
	displayTimeZoneStore._applyFromServer(prefs.display_time_zone);
	if (prefs.language) void i18n.changeLanguage(prefs.language);
}

function cachedUserToOfflineUser(cached: CachedUser | null): MeResponse | null {
	if (!cached) return null;
	return {
		id: 0,
		username: "",
		email: "",
		email_verified: false,
		pending_email: null,
		role: "user",
		status: "active",
		must_change_password: false,
		policy_group_id: null,
		storage_used: 0,
		storage_quota: 0,
		access_token_expires_at: cached.access_token_expires_at ?? 0,
		created_at: "",
		updated_at: "",
		profile: cached.profile ?? {
			avatar: {
				source: "none",
				url_512: null,
				url_1024: null,
				version: 0,
			},
			display_name: null,
		},
		preferences: cached.preferences ?? null,
	};
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
	probePublicSession: () => Promise<void>;
	ensureFreshSession: () => Promise<void>;
	refreshToken: () => Promise<void>;
	refreshUser: (options?: RefreshUserOptions) => Promise<void>;
	setStorageEventStreamEnabled: (enabled: boolean) => void;
	syncSession: (expiresIn: number) => void;
	startAutoRefresh: (delayMs?: number) => void;
	stopAutoRefresh: () => void;
}

const initialCachedUser = getCachedUser();
const initialOfflineUser = cachedUserToOfflineUser(initialCachedUser);
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

function commitAuthenticatedUser(
	user: MeResponse,
	getState: () => AuthState,
	setAuthState: (state: Partial<AuthState>) => void,
) {
	const expiresAt =
		getExpiresAtFromUser(user) ?? getState().expiresAt ?? getStoredExpiresAt();
	setCachedUser(user);
	if (expiresAt !== null) setStoredExpiresAt(expiresAt);
	setAuthState({
		isAuthenticated: true,
		isChecking: false,
		isAuthStale: false,
		bootOffline: false,
		user,
		expiresAt,
	});
	return expiresAt;
}

async function bootstrapCurrentSession(
	getState: () => AuthState,
	setAuthState: (state: Partial<AuthState>) => void,
	publicProbe: boolean,
) {
	setAuthState({ isChecking: true, bootOffline: false });
	try {
		const user = publicProbe
			? await authService.probeCurrentSession()
			: await authService.me();
		const expiresAt = commitAuthenticatedUser(user, getState, setAuthState);

		if (publicProbe) return;
		if (!expiresAt || expiresAt - Date.now() <= REFRESH_BUFFER_MS) {
			try {
				await getState().refreshToken();
			} catch (error) {
				logger.warn("checkAuth bootstrap refresh failed", error);
			}
		} else {
			getState().startAutoRefresh();
		}
	} catch (error) {
		if (isSessionAuthFailure(error)) {
			applyLoggedOutState(setAuthState);
			return;
		}
		if (publicProbe) {
			setAuthState(LOGGED_OUT_STATE);
			return;
		}

		const cached = getCachedUser();
		const offlineUser = cachedUserToOfflineUser(cached);
		const expiresAt =
			getExpiresAtFromUser(cached) ??
			getState().expiresAt ??
			getStoredExpiresAt();
		if (offlineUser) {
			setAuthState({
				isAuthenticated: true,
				isChecking: false,
				isAuthStale: true,
				bootOffline: false,
				user: offlineUser,
				expiresAt,
			});
			if (expiresAt) getState().startAutoRefresh();
			return;
		}

		setAuthState({
			...LOGGED_OUT_STATE,
			bootOffline: true,
		});
	}
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
	user: initialOfflineUser,
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
		await bootstrapCurrentSession(get, set, false);
	},

	probePublicSession: async () => {
		await bootstrapCurrentSession(get, set, true);
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
