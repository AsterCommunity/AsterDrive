import type {
	ActionMessageResp,
	AuthSessionInfo,
	AuthTokenResp,
	AvatarSource,
	ChangePasswordRequest,
	CheckResp,
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
	UpdatePreferencesRequest,
	UpdateProfileRequest,
	UserInfo,
	UserPreferences,
	UserProfileInfo,
} from "@/types/api";
import { type ApiResponse, ErrorCode } from "@/types/api-helpers";
import { ApiError, api } from "./http";

export interface AuthSessionState {
	expiresIn: number;
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

	login: async (
		identifier: string,
		password: string,
	): Promise<AuthSessionState> => {
		const data = await api.post<AuthTokenResp>("/auth/login", {
			identifier,
			password,
		});
		return {
			expiresIn: Number(data.expires_in) || 900,
		};
	},

	startPasskeyLogin: (payload: PasskeyLoginStartRequest = {}) =>
		api.post<PasskeyLoginStartResponse>("/auth/passkeys/login/start", payload),

	finishPasskeyLogin: async (
		flowId: string,
		credential: unknown,
	): Promise<AuthSessionState> => {
		const data = await api.post<AuthTokenResp>("/auth/passkeys/login/finish", {
			flow_id: flowId,
			credential,
		});
		return {
			expiresIn: Number(data.expires_in) || 900,
		};
	},

	register: (username: string, email: string, password: string) =>
		api.post<UserInfo>("/auth/register", { username, email, password }),

	resendRegisterActivation: (identifier: string) =>
		api.post<ActionMessageResp>("/auth/register/resend", { identifier }),

	requestPasswordReset: (payload: PasswordResetRequestRequest) =>
		api.post<ActionMessageResp>("/auth/password/reset/request", payload),

	confirmPasswordReset: (payload: PasswordResetConfirmRequest) =>
		api.post<ActionMessageResp>("/auth/password/reset/confirm", payload),

	setup: (username: string, email: string, password: string) =>
		api.post<UserInfo>("/auth/setup", { username, email, password }),

	logout: () => api.post<void>("/auth/logout"),

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

	listPasskeys: () => api.get<PasskeyInfo[]>("/auth/passkeys"),

	startPasskeyRegistration: (payload: PasskeyRegisterStartRequest) =>
		api.post<PasskeyRegisterStartResponse>(
			"/auth/passkeys/register/start",
			payload,
		),

	finishPasskeyRegistration: (
		flowId: string,
		credential: unknown,
		name?: string,
	) =>
		api.post<PasskeyInfo>("/auth/passkeys/register/finish", {
			flow_id: flowId,
			credential,
			name,
		}),

	renamePasskey: (id: number, payload: PatchPasskeyRequest) =>
		api.patch<PasskeyInfo>(`/auth/passkeys/${id}`, payload),

	deletePasskey: (id: number) => api.delete<void>(`/auth/passkeys/${id}`),

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
		if (resp.code !== ErrorCode.Success) {
			throw new ApiError(resp.code, resp.msg, {
				internalCode: resp.error?.internal_code ?? undefined,
				subcode: resp.error?.subcode ?? undefined,
				retryable: resp.error?.retryable ?? undefined,
			});
		}
		return resp.data as UserProfileInfo;
	},
};
