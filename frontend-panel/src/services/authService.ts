import type {
	AcceptUserInvitationRequest,
	ActionMessageResp,
	ApiResponse,
	AuthSessionInfo,
	AuthTokenResp,
	AvatarSource,
	ChangePasswordRequest,
	CheckResp,
	ExternalAuthEmailVerificationStartRequest,
	ExternalAuthEmailVerificationStartResponse,
	ExternalAuthLinkInfo,
	ExternalAuthPasswordLinkRequest,
	ExternalAuthPublicProvider,
	ExternalAuthStartLoginRequest,
	ExternalAuthStartLoginResponse,
	MeField,
	MePartialResponse,
	MeQuery,
	MeResponse,
	PasskeyInfo,
	PasskeyLoginStartRequest,
	PasskeyLoginStartResponse,
	PasskeyRegisterStartRequest,
	PasskeyRegisterStartResponse,
	PasswordResetConfirmRequest,
	PasswordResetRequestRequest,
	PatchPasskeyRequest,
	PublicUserInvitationInfo,
	UpdatePreferencesRequest,
	UpdateProfileRequest,
	UserInfo,
	UserPreferences,
	UserProfileInfo,
} from "@/types/api";
import { ApiErrorCode } from "@/types/api-helpers";
import { ApiError, api } from "./http";

export interface AuthSessionState {
	expiresIn: number;
}

export type MfaMethod = "totp" | "recovery_code" | "email_code";

export type LoginResult =
	| { status: "authenticated"; expiresIn: number }
	| { status: "password_change_required"; expiresIn: number }
	| {
			status: "mfa_required";
			flowToken: string;
			expiresIn: number;
			methods: MfaMethod[];
	  };

export interface MfaFactorInfo {
	id: number;
	method: "totp";
	name: string;
	enabled_at: string;
	last_used_at: string | null;
}

export interface MfaStatus {
	enabled: boolean;
	factors: MfaFactorInfo[];
	recovery_codes_remaining: number;
}

export interface TotpSetupStartResponse {
	flow_token: string;
	expires_in: number;
	secret: string;
	otpauth_uri: string;
}

export interface TotpSetupFinishResponse {
	factor: MfaFactorInfo;
	recovery_codes: string[];
}

export interface MfaSensitiveActionRequest {
	code?: string;
}

export interface MfaEmailCodeSendResponse {
	expires_in: number;
	resend_after: number;
}

type RawLoginResponse = {
	status?: string;
	expires_in?: number;
	flow_token?: string;
	methods?: string[];
};

interface ListPasskeysOptions {
	force?: boolean;
}

interface ListExternalAuthLinksOptions {
	force?: boolean;
}

interface GetMfaStatusOptions {
	force?: boolean;
}

let cachedPasskeys: PasskeyInfo[] | null = null;
let pendingPasskeysRequest: Promise<PasskeyInfo[]> | null = null;
let passkeysCacheSerial = 0;

let cachedMfaStatus: MfaStatus | null = null;
let pendingMfaStatusRequest: Promise<MfaStatus> | null = null;
let mfaStatusCacheSerial = 0;

let cachedExternalAuthLinks: ExternalAuthLinkInfo[] | null = null;
let pendingExternalAuthLinksRequest: Promise<ExternalAuthLinkInfo[]> | null =
	null;
let externalAuthLinksCacheSerial = 0;

function clonePasskeys(passkeys: PasskeyInfo[]) {
	return passkeys.map((passkey) => ({ ...passkey }));
}

function cloneExternalAuthLinks(links: ExternalAuthLinkInfo[]) {
	return links.map((link) => ({ ...link }));
}

function cloneMfaStatus(status: MfaStatus) {
	return {
		...status,
		factors: status.factors.map((factor) => ({ ...factor })),
	};
}

function primePasskeysCache(passkeys: PasskeyInfo[]) {
	cachedPasskeys = clonePasskeys(passkeys);
}

function primeMfaStatusCache(status: MfaStatus) {
	cachedMfaStatus = cloneMfaStatus(status);
}

function primeExternalAuthLinksCache(links: ExternalAuthLinkInfo[]) {
	cachedExternalAuthLinks = cloneExternalAuthLinks(links);
}

export function invalidatePasskeysCache() {
	cachedPasskeys = null;
	pendingPasskeysRequest = null;
	passkeysCacheSerial += 1;
}

export function invalidateMfaStatusCache() {
	cachedMfaStatus = null;
	pendingMfaStatusRequest = null;
	mfaStatusCacheSerial += 1;
}

export function invalidateExternalAuthLinksCache() {
	cachedExternalAuthLinks = null;
	pendingExternalAuthLinksRequest = null;
	externalAuthLinksCacheSerial += 1;
}

function invalidateAuthIdentityCaches() {
	invalidatePasskeysCache();
	invalidateMfaStatusCache();
	invalidateExternalAuthLinksCache();
}

function normalizeLoginResult(data: RawLoginResponse): LoginResult {
	const expiresIn = Number(data.expires_in) || 900;
	if (data.status === "mfa_required") {
		if (!data.flow_token) {
			throw new Error("MFA challenge response is missing flow token");
		}
		return {
			status: "mfa_required",
			flowToken: data.flow_token,
			expiresIn,
			methods: (data.methods || []).filter(
				(method): method is MfaMethod =>
					method === "totp" ||
					method === "recovery_code" ||
					method === "email_code",
			),
		};
	}
	if (data.status === "password_change_required") {
		return {
			status: "password_change_required",
			expiresIn,
		};
	}
	return {
		status: "authenticated",
		expiresIn,
	};
}

function upsertCachedPasskey(passkey: PasskeyInfo) {
	pendingPasskeysRequest = null;
	passkeysCacheSerial += 1;
	if (cachedPasskeys === null) {
		return;
	}
	cachedPasskeys = [
		{ ...passkey },
		...cachedPasskeys.filter((item) => item.id !== passkey.id),
	];
}

function replaceCachedPasskey(passkey: PasskeyInfo) {
	pendingPasskeysRequest = null;
	passkeysCacheSerial += 1;
	if (cachedPasskeys === null) {
		return;
	}
	cachedPasskeys = cachedPasskeys.map((item) =>
		item.id === passkey.id ? { ...passkey } : item,
	);
}

function removeCachedPasskey(id: number) {
	pendingPasskeysRequest = null;
	passkeysCacheSerial += 1;
	if (cachedPasskeys === null) {
		return;
	}
	cachedPasskeys = cachedPasskeys.filter((item) => item.id !== id);
}

function removeCachedExternalAuthLink(id: number) {
	pendingExternalAuthLinksRequest = null;
	externalAuthLinksCacheSerial += 1;
	if (cachedExternalAuthLinks === null) {
		return;
	}
	cachedExternalAuthLinks = cachedExternalAuthLinks.filter(
		(item) => item.id !== id,
	);
}

function listPasskeys(options?: ListPasskeysOptions) {
	const force = options?.force ?? false;
	if (!force && cachedPasskeys !== null) {
		return Promise.resolve(clonePasskeys(cachedPasskeys));
	}
	if (!force && pendingPasskeysRequest !== null) {
		return pendingPasskeysRequest.then(clonePasskeys);
	}

	const requestSerial = ++passkeysCacheSerial;
	const request = api
		.get<PasskeyInfo[]>("/auth/passkeys")
		.then((passkeys) => {
			if (requestSerial === passkeysCacheSerial) {
				primePasskeysCache(passkeys);
			}
			return clonePasskeys(passkeys);
		})
		.finally(() => {
			if (pendingPasskeysRequest === request) {
				pendingPasskeysRequest = null;
			}
		});
	pendingPasskeysRequest = request;
	return request.then(clonePasskeys);
}

function listExternalAuthLinks(options?: ListExternalAuthLinksOptions) {
	const force = options?.force ?? false;
	if (!force && cachedExternalAuthLinks !== null) {
		return Promise.resolve(cloneExternalAuthLinks(cachedExternalAuthLinks));
	}
	if (!force && pendingExternalAuthLinksRequest !== null) {
		return pendingExternalAuthLinksRequest.then(cloneExternalAuthLinks);
	}

	const requestSerial = ++externalAuthLinksCacheSerial;
	const request = api
		.get<ExternalAuthLinkInfo[]>("/auth/external-auth/links")
		.then((links) => {
			if (requestSerial === externalAuthLinksCacheSerial) {
				primeExternalAuthLinksCache(links);
			}
			return cloneExternalAuthLinks(links);
		})
		.finally(() => {
			if (pendingExternalAuthLinksRequest === request) {
				pendingExternalAuthLinksRequest = null;
			}
		});
	pendingExternalAuthLinksRequest = request;
	return request.then(cloneExternalAuthLinks);
}

function getMfaStatus(options?: GetMfaStatusOptions) {
	const force = options?.force ?? false;
	if (!force && cachedMfaStatus !== null) {
		return Promise.resolve(cloneMfaStatus(cachedMfaStatus));
	}
	if (!force && pendingMfaStatusRequest !== null) {
		return pendingMfaStatusRequest.then(cloneMfaStatus);
	}

	const requestSerial = ++mfaStatusCacheSerial;
	const request = (async () => {
		const status = await api.get<MfaStatus>("/auth/mfa");
		if (status) {
			if (requestSerial === mfaStatusCacheSerial) {
				primeMfaStatusCache(status);
			}
			return cloneMfaStatus(status);
		}
		return status;
	})().finally(() => {
		if (pendingMfaStatusRequest === request) {
			pendingMfaStatusRequest = null;
		}
	});
	pendingMfaStatusRequest = request;
	return request.then((status) => (status ? cloneMfaStatus(status) : status));
}

function me(): Promise<MeResponse>;
function me(fields: MeField[]): Promise<MePartialResponse>;
function me(fields?: MeField[]) {
	if (!fields || fields.length === 0) {
		return api.get<MeResponse>("/auth/me");
	}
	const params: MeQuery = { fields: fields.join(",") };
	return api.get<MePartialResponse>("/auth/me", {
		params,
	});
}

export const authService = {
	check: () => api.post<CheckResp>("/auth/check"),

	login: async (identifier: string, password: string): Promise<LoginResult> => {
		invalidateAuthIdentityCaches();
		const data = await api.post<RawLoginResponse>("/auth/login", {
			identifier,
			password,
		});
		return normalizeLoginResult(data);
	},

	startPasskeyLogin: (payload: PasskeyLoginStartRequest = {}) =>
		api.post<PasskeyLoginStartResponse>("/auth/passkeys/login/start", payload),

	listExternalAuthProviders: () =>
		api.get<ExternalAuthPublicProvider[]>("/auth/external-auth/providers"),

	startExternalAuthLogin: (
		provider: ExternalAuthPublicProvider,
		payload: ExternalAuthStartLoginRequest = {},
	) =>
		api.post<ExternalAuthStartLoginResponse>(
			`/auth/external-auth/${encodeURIComponent(provider.kind)}/${encodeURIComponent(provider.key)}/start`,
			payload,
		),

	startExternalAuthEmailVerification: (
		payload: ExternalAuthEmailVerificationStartRequest,
	) =>
		api.post<ExternalAuthEmailVerificationStartResponse>(
			"/auth/external-auth/email-verification/start",
			payload,
		),

	linkExternalAuthWithPassword: async (
		payload: ExternalAuthPasswordLinkRequest,
	): Promise<LoginResult> => {
		invalidateAuthIdentityCaches();
		const data = await api.post<RawLoginResponse>(
			"/auth/external-auth/password-link",
			payload,
		);
		return normalizeLoginResult(data);
	},

	finishPasskeyLogin: async (
		flowId: string,
		credential: unknown,
	): Promise<LoginResult> => {
		invalidateAuthIdentityCaches();
		const data = await api.post<RawLoginResponse>(
			"/auth/passkeys/login/finish",
			{
				flow_id: flowId,
				credential,
			},
		);
		return normalizeLoginResult(data);
	},

	register: (username: string, email: string, password: string) => {
		invalidateAuthIdentityCaches();
		return api.post<UserInfo>("/auth/register", { username, email, password });
	},

	resendRegisterActivation: (identifier: string) =>
		api.post<ActionMessageResp>("/auth/register/resend", { identifier }),

	requestPasswordReset: (payload: PasswordResetRequestRequest) =>
		api.post<ActionMessageResp>("/auth/password/reset/request", payload),

	confirmPasswordReset: (payload: PasswordResetConfirmRequest) =>
		api.post<ActionMessageResp>("/auth/password/reset/confirm", payload),

	verifyInvitation: (token: string) =>
		api.get<PublicUserInvitationInfo>(
			`/auth/invitations/${encodeURIComponent(token)}`,
		),

	acceptInvitation: (token: string, payload: AcceptUserInvitationRequest) => {
		invalidateAuthIdentityCaches();
		return api.post<UserInfo>(
			`/auth/invitations/${encodeURIComponent(token)}/accept`,
			payload,
		);
	},

	setup: (username: string, email: string, password: string) => {
		invalidateAuthIdentityCaches();
		return api.post<UserInfo>("/auth/setup", { username, email, password });
	},

	logout: () => {
		invalidateAuthIdentityCaches();
		return api.post<void>("/auth/logout");
	},

	refreshToken: async (): Promise<AuthSessionState> => {
		const data = await api.post<AuthTokenResp>("/auth/refresh");
		return {
			expiresIn: Number(data.expires_in) || 900,
		};
	},

	me,

	updatePreferences: (prefs: UpdatePreferencesRequest) =>
		api.patch<UserPreferences>("/auth/preferences", prefs),

	changePassword: async (
		payload: ChangePasswordRequest,
	): Promise<AuthSessionState> => {
		const data = await api.put<AuthTokenResp>("/auth/password", payload);
		return {
			expiresIn: Number(data.expires_in) || 900,
		};
	},

	listSessions: () => api.get<AuthSessionInfo[]>("/auth/sessions"),

	listExternalAuthLinks,

	getMfaStatus,

	startTotpSetup: () =>
		api.post<TotpSetupStartResponse>("/auth/mfa/totp/setup/start"),

	finishTotpSetup: (payload: {
		flow_token: string;
		code: string;
		name?: string;
	}) => {
		invalidateMfaStatusCache();
		return api.post<TotpSetupFinishResponse>(
			"/auth/mfa/totp/setup/finish",
			payload,
		);
	},

	verifyMfaChallenge: async (payload: {
		flow_token: string;
		method: MfaMethod;
		code: string;
	}): Promise<LoginResult> => {
		const data = await api.post<RawLoginResponse>(
			"/auth/mfa/challenge/verify",
			payload,
		);
		return normalizeLoginResult(data);
	},

	sendMfaEmailCode: (payload: { flow_token: string }) =>
		api.post<MfaEmailCodeSendResponse>(
			"/auth/mfa/challenge/email-code/send",
			payload,
		),

	deleteMfaFactor: async (id: number, payload: MfaSensitiveActionRequest) => {
		const result = await api.delete<void>(`/auth/mfa/factors/${id}`, {
			data: payload,
		});
		invalidateMfaStatusCache();
		return result;
	},

	regenerateMfaRecoveryCodes: async (payload: MfaSensitiveActionRequest) => {
		const result = await api.post<string[]>(
			"/auth/mfa/recovery-codes/regenerate",
			payload,
		);
		invalidateMfaStatusCache();
		return result;
	},

	deleteExternalAuthLink: async (id: number) => {
		await api.delete<void>(`/auth/external-auth/links/${id}`);
		removeCachedExternalAuthLink(id);
	},

	listPasskeys,

	startPasskeyRegistration: (payload: PasskeyRegisterStartRequest) =>
		api.post<PasskeyRegisterStartResponse>(
			"/auth/passkeys/register/start",
			payload,
		),

	finishPasskeyRegistration: async (
		flowId: string,
		credential: unknown,
		name?: string,
	) => {
		const passkey = await api.post<PasskeyInfo>(
			"/auth/passkeys/register/finish",
			{
				flow_id: flowId,
				credential,
				name,
			},
		);
		upsertCachedPasskey(passkey);
		return passkey;
	},

	renamePasskey: async (id: number, payload: PatchPasskeyRequest) => {
		const passkey = await api.patch<PasskeyInfo>(
			`/auth/passkeys/${id}`,
			payload,
		);
		replaceCachedPasskey(passkey);
		return passkey;
	},

	deletePasskey: async (id: number) => {
		await api.delete<void>(`/auth/passkeys/${id}`);
		removeCachedPasskey(id);
	},

	revokeSession: (id: string) => api.delete<void>(`/auth/sessions/${id}`),

	revokeOtherSessions: async (): Promise<number> => {
		const data = await api.delete<{ removed: number }>("/auth/sessions/others");
		return Number(data.removed) || 0;
	},

	updateProfile: (profile: UpdateProfileRequest) =>
		api.patch<UserProfileInfo>("/auth/profile", profile),

	requestEmailChange: (newEmail: string) =>
		api.post<UserInfo>("/auth/email/change", { new_email: newEmail }),

	resendEmailChange: () =>
		api.post<ActionMessageResp>("/auth/email/change/resend"),

	setAvatarSource: (source: Extract<AvatarSource, "none" | "gravatar">) =>
		api.put<UserProfileInfo>("/auth/profile/avatar/source", { source }),

	uploadAvatar: async (file: File) => {
		const formData = new FormData();
		formData.set("file", file);
		const { data: resp } = await api.client.post<ApiResponse<UserProfileInfo>>(
			"/auth/profile/avatar/upload",
			formData,
			{
				headers: {
					"Content-Type": "multipart/form-data",
				},
			},
		);
		if (resp.code !== ApiErrorCode.Success) {
			throw new ApiError(resp.code, resp.msg, {
				retryable: resp.error?.retryable ?? undefined,
			});
		}
		return resp.data as UserProfileInfo;
	},
};
