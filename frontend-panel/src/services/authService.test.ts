import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	authService,
	invalidateExternalAuthLinksCache,
	invalidateMfaStatusCache,
	invalidatePasskeysCache,
} from "@/services/authService";
import type {
	ExternalAuthLinkInfo,
	ExternalAuthPublicProvider,
	PasskeyInfo,
} from "@/types/api";
import { ApiSubcode, ErrorCode } from "@/types/api-helpers";

const mockState = vi.hoisted(() => ({
	clientPost: vi.fn(),
	delete: vi.fn(),
	get: vi.fn(),
	patch: vi.fn(),
	post: vi.fn(),
	put: vi.fn(),
}));

vi.mock("@/services/http", () => ({
	api: {
		client: {
			post: mockState.clientPost,
		},
		delete: mockState.delete,
		get: mockState.get,
		patch: mockState.patch,
		post: mockState.post,
		put: mockState.put,
	},
	ApiError: class ApiError extends Error {
		code: number;
		internalCode?: string;
		retryable?: boolean;
		subcode?: string;

		constructor(
			code: number,
			message: string,
			options?: {
				internalCode?: string;
				retryable?: boolean;
				subcode?: string;
			},
		) {
			super(message);
			this.code = code;
			this.internalCode = options?.internalCode;
			this.retryable = options?.retryable;
			this.subcode = options?.subcode;
		}
	},
}));

describe("authService", () => {
	beforeEach(() => {
		invalidateExternalAuthLinksCache();
		invalidateMfaStatusCache();
		invalidatePasskeysCache();
		mockState.clientPost.mockReset();
		mockState.delete.mockReset();
		mockState.get.mockReset();
		mockState.patch.mockReset();
		mockState.post.mockReset();
		mockState.put.mockReset();
	});

	function passkey(overrides: Partial<PasskeyInfo> = {}): PasskeyInfo {
		return {
			backed_up: false,
			backup_eligible: true,
			created_at: "2026-04-01T08:00:00Z",
			id: 1,
			last_used_at: null,
			name: "Phone",
			sign_count: 0,
			transports: null,
			updated_at: "2026-04-01T08:00:00Z",
			...overrides,
		};
	}

	function externalAuthLink(
		overrides: Partial<ExternalAuthLinkInfo> = {},
	): ExternalAuthLinkInfo {
		return {
			created_at: "2026-05-01T08:00:00Z",
			display_name_snapshot: "Alice",
			email_snapshot: "alice@example.com",
			id: 1,
			issuer: "https://idp.example.com",
			last_login_at: null,
			provider_display_name: "Example IDP",
			provider_icon_url: "/static/external-auth/example.svg",
			provider_id: 1,
			provider_kind: "oidc",
			provider_key: "example",
			subject: "subject-1",
			updated_at: "2026-05-01T08:00:00Z",
			...overrides,
		};
	}

	it("uses the expected auth endpoints and payloads", async () => {
		const prefs = {
			language: "zh",
			sort_by: "updated_at",
		};
		mockState.post.mockImplementation((url: string) => {
			if (
				url === "/auth/login" ||
				url === "/auth/refresh" ||
				url === "/auth/passkeys/login/finish"
			) {
				return { expires_in: 900 };
			}
			if (url === "/auth/passkeys/login/start") {
				return { flow_id: "login-flow", public_key: {} };
			}
			if (url === "/auth/passkeys/register/start") {
				return { flow_id: "register-flow", public_key: {} };
			}
			if (url === "/auth/passkeys/register/finish") {
				return { id: 1, name: "Laptop" };
			}
			return undefined;
		});
		mockState.put.mockImplementation((url: string) => {
			if (url === "/auth/password") {
				return { expires_in: 900 };
			}
			return undefined;
		});
		mockState.patch.mockImplementation((url: string) => {
			if (url === "/auth/passkeys/1") {
				return { id: 1, name: "Phone" };
			}
			return undefined;
		});
		mockState.get.mockImplementation((url: string) => {
			if (url === "/auth/sessions") {
				return [];
			}
			if (url === "/auth/passkeys") {
				return Promise.resolve([]);
			}
			if (url === "/auth/external-auth/links") {
				return Promise.resolve([]);
			}
			return undefined;
		});
		mockState.delete.mockImplementation((url: string) => {
			if (url === "/auth/sessions/others") {
				return { removed: 2 };
			}
			return undefined;
		});

		authService.check();
		await expect(
			authService.login("alice@example.com", "secret"),
		).resolves.toEqual({
			status: "authenticated",
			expiresIn: 900,
		});
		authService.register("alice", "alice@example.com", "secret");
		authService.resendRegisterActivation("alice@example.com");
		authService.requestPasswordReset({ email: "alice@example.com" });
		authService.confirmPasswordReset({
			new_password: "newsecret",
			token: "reset-token",
		});
		authService.setup("owner", "owner@example.com", "secret");
		authService.logout();
		await expect(authService.refreshToken()).resolves.toEqual({
			expiresIn: 900,
		});
		authService.startPasskeyLogin({ identifier: "alice@example.com" });
		await expect(
			authService.finishPasskeyLogin("login-flow", { id: "cred" }),
		).resolves.toEqual({ expiresIn: 900 });
		authService.me();
		authService.me(["quota"]);
		authService.updatePreferences(prefs);
		await expect(
			authService.changePassword({
				current_password: "secret",
				new_password: "newsecret",
			}),
		).resolves.toEqual({
			expiresIn: 900,
		});
		authService.updateProfile({ display_name: "Alice" });
		authService.requestEmailChange("alice+next@example.com");
		authService.resendEmailChange();
		authService.setAvatarSource("gravatar");
		expect(authService.listSessions()).toEqual([]);
		await expect(authService.listPasskeys()).resolves.toEqual([]);
		await expect(authService.listExternalAuthLinks()).resolves.toEqual([]);
		authService.startPasskeyRegistration({ name: "Laptop" });
		await authService.finishPasskeyRegistration(
			"register-flow",
			{ id: "cred" },
			"Laptop",
		);
		await authService.renamePasskey(1, { name: "Phone" });
		await authService.deletePasskey(1);
		await authService.deleteExternalAuthLink(1);
		authService.revokeSession("session-1");
		await expect(authService.revokeOtherSessions()).resolves.toBe(2);

		expect(mockState.post).toHaveBeenNthCalledWith(1, "/auth/check");
		expect(mockState.post).toHaveBeenNthCalledWith(2, "/auth/login", {
			identifier: "alice@example.com",
			password: "secret",
		});
		expect(mockState.post).toHaveBeenNthCalledWith(3, "/auth/register", {
			username: "alice",
			email: "alice@example.com",
			password: "secret",
		});
		expect(mockState.post).toHaveBeenNthCalledWith(4, "/auth/register/resend", {
			identifier: "alice@example.com",
		});
		expect(mockState.post).toHaveBeenNthCalledWith(
			5,
			"/auth/password/reset/request",
			{ email: "alice@example.com" },
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			6,
			"/auth/password/reset/confirm",
			{
				new_password: "newsecret",
				token: "reset-token",
			},
		);
		expect(mockState.post).toHaveBeenNthCalledWith(7, "/auth/setup", {
			username: "owner",
			email: "owner@example.com",
			password: "secret",
		});
		expect(mockState.post).toHaveBeenNthCalledWith(8, "/auth/logout");
		expect(mockState.post).toHaveBeenNthCalledWith(9, "/auth/refresh");
		expect(mockState.post).toHaveBeenNthCalledWith(
			10,
			"/auth/passkeys/login/start",
			{
				identifier: "alice@example.com",
			},
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			11,
			"/auth/passkeys/login/finish",
			{
				flow_id: "login-flow",
				credential: { id: "cred" },
			},
		);
		expect(mockState.get).toHaveBeenNthCalledWith(1, "/auth/me");
		expect(mockState.get).toHaveBeenNthCalledWith(2, "/auth/me", {
			params: { fields: "quota" },
		});
		expect(mockState.patch).toHaveBeenNthCalledWith(
			1,
			"/auth/preferences",
			prefs,
		);
		expect(mockState.put).toHaveBeenNthCalledWith(1, "/auth/password", {
			current_password: "secret",
			new_password: "newsecret",
		});
		expect(mockState.patch).toHaveBeenNthCalledWith(2, "/auth/profile", {
			display_name: "Alice",
		});
		expect(mockState.post).toHaveBeenNthCalledWith(12, "/auth/email/change", {
			new_email: "alice+next@example.com",
		});
		expect(mockState.post).toHaveBeenNthCalledWith(
			13,
			"/auth/email/change/resend",
		);
		expect(mockState.put).toHaveBeenNthCalledWith(
			2,
			"/auth/profile/avatar/source",
			{
				source: "gravatar",
			},
		);
		expect(mockState.get).toHaveBeenNthCalledWith(3, "/auth/sessions");
		expect(mockState.get).toHaveBeenNthCalledWith(4, "/auth/passkeys");
		expect(mockState.get).toHaveBeenNthCalledWith(
			5,
			"/auth/external-auth/links",
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			14,
			"/auth/passkeys/register/start",
			{ name: "Laptop" },
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			15,
			"/auth/passkeys/register/finish",
			{
				flow_id: "register-flow",
				credential: { id: "cred" },
				name: "Laptop",
			},
		);
		expect(mockState.patch).toHaveBeenNthCalledWith(3, "/auth/passkeys/1", {
			name: "Phone",
		});
		expect(mockState.delete).toHaveBeenNthCalledWith(1, "/auth/passkeys/1");
		expect(mockState.delete).toHaveBeenNthCalledWith(
			2,
			"/auth/external-auth/links/1",
		);
		expect(mockState.delete).toHaveBeenNthCalledWith(
			3,
			"/auth/sessions/session-1",
		);
		expect(mockState.delete).toHaveBeenNthCalledWith(
			4,
			"/auth/sessions/others",
		);
	});

	it("falls back invalid token lifetimes to the default session duration", async () => {
		mockState.post.mockImplementation((url: string) => {
			if (
				url === "/auth/login" ||
				url === "/auth/refresh" ||
				url === "/auth/passkeys/login/finish"
			) {
				return { expires_in: 0 };
			}
			return undefined;
		});
		mockState.put.mockReturnValue({ expires_in: Number.NaN });
		mockState.delete.mockReturnValue({ removed: 0 });

		await expect(authService.login("alice", "secret")).resolves.toEqual({
			status: "authenticated",
			expiresIn: 900,
		});
		await expect(
			authService.finishPasskeyLogin("flow", { id: "cred" }),
		).resolves.toEqual({
			expiresIn: 900,
		});
		await expect(authService.refreshToken()).resolves.toEqual({
			expiresIn: 900,
		});
		await expect(
			authService.changePassword({
				current_password: "oldsecret",
				new_password: "newsecret",
			}),
		).resolves.toEqual({
			expiresIn: 900,
		});
		await expect(authService.revokeOtherSessions()).resolves.toBe(0);
	});

	it("parses login and MFA challenge response variants", async () => {
		mockState.post.mockImplementation((url: string) => {
			if (url === "/auth/login") {
				return {
					status: "mfa_required",
					expires_in: 300,
					flow_token: "mfa-flow",
					methods: ["totp", "recovery_code", "email_code"],
				};
			}
			if (url === "/auth/mfa/challenge/verify") {
				return { status: "authenticated", expires_in: 900 };
			}
			if (url === "/auth/mfa/challenge/email-code/send") {
				return { expires_in: 600, resend_after: 60 };
			}
			return undefined;
		});

		await expect(authService.login("alice", "secret")).resolves.toEqual({
			status: "mfa_required",
			expiresIn: 300,
			flowToken: "mfa-flow",
			methods: ["totp", "recovery_code", "email_code"],
		});
		await expect(
			authService.verifyMfaChallenge({
				flow_token: "mfa-flow",
				method: "totp",
				code: "123456",
			}),
		).resolves.toEqual({ expiresIn: 900 });
		expect(authService.sendMfaEmailCode({ flow_token: "mfa-flow" })).toEqual({
			expires_in: 600,
			resend_after: 60,
		});

		expect(mockState.post).toHaveBeenNthCalledWith(1, "/auth/login", {
			identifier: "alice",
			password: "secret",
		});
		expect(mockState.post).toHaveBeenNthCalledWith(
			2,
			"/auth/mfa/challenge/verify",
			{
				flow_token: "mfa-flow",
				method: "totp",
				code: "123456",
			},
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			3,
			"/auth/mfa/challenge/email-code/send",
			{
				flow_token: "mfa-flow",
			},
		);
	});

	it("filters unsupported MFA methods from login responses", async () => {
		mockState.post.mockReturnValueOnce({
			status: "mfa_required",
			expires_in: 300,
			flow_token: "mfa-flow",
			methods: ["email_code", "sms", "", "totp"],
		});

		await expect(authService.login("alice", "secret")).resolves.toEqual({
			status: "mfa_required",
			expiresIn: 300,
			flowToken: "mfa-flow",
			methods: ["email_code", "totp"],
		});
	});

	it("rejects MFA login responses without a flow token", async () => {
		mockState.post.mockReturnValueOnce({
			status: "mfa_required",
			expires_in: 300,
			methods: ["totp"],
		});

		await expect(authService.login("alice", "secret")).rejects.toThrow(
			"MFA challenge response is missing flow token",
		);
	});

	it("uses MFA management endpoints without current password", async () => {
		mockState.post.mockImplementation((url: string) => {
			if (url === "/auth/mfa/totp/setup/start") {
				return {
					expires_in: 300,
					flow_token: "setup-flow",
					otpauth_uri: "otpauth://totp/AsterDrive:alice",
					secret: "SECRET",
				};
			}
			if (url === "/auth/mfa/totp/setup/finish") {
				return {
					factor: {
						enabled_at: "2026-05-23T00:00:00Z",
						id: 7,
						last_used_at: null,
						method: "totp",
						name: "Phone",
					},
					recovery_codes: ["ABCD-EFGH-IJKL"],
				};
			}
			if (url === "/auth/mfa/recovery-codes/regenerate") {
				return ["KLMN-OPQR-STUV"];
			}
			return undefined;
		});
		mockState.delete.mockResolvedValue(undefined);

		await expect(await authService.startTotpSetup()).toMatchObject({
			flow_token: "setup-flow",
			secret: "SECRET",
		});
		await expect(
			await authService.finishTotpSetup({
				flow_token: "setup-flow",
				code: "123456",
				name: "Phone",
			}),
		).toMatchObject({
			recovery_codes: ["ABCD-EFGH-IJKL"],
		});
		await authService.deleteMfaFactor(7, { code: "123456" });
		await expect(
			await authService.regenerateMfaRecoveryCodes({ code: "KLMN-OPQR-STUV" }),
		).toEqual(["KLMN-OPQR-STUV"]);
		await expect(authService.getMfaStatus()).resolves.toBeUndefined();

		expect(mockState.post).toHaveBeenNthCalledWith(
			1,
			"/auth/mfa/totp/setup/start",
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			2,
			"/auth/mfa/totp/setup/finish",
			{
				flow_token: "setup-flow",
				code: "123456",
				name: "Phone",
			},
		);
		expect(mockState.delete).toHaveBeenCalledWith("/auth/mfa/factors/7", {
			data: { code: "123456" },
		});
		expect(mockState.post).toHaveBeenNthCalledWith(
			3,
			"/auth/mfa/recovery-codes/regenerate",
			{ code: "KLMN-OPQR-STUV" },
		);
		expect(mockState.get).toHaveBeenCalledWith("/auth/mfa");
	});

	it("caches MFA status requests and invalidates them after MFA changes", async () => {
		const enabledStatus = {
			enabled: true,
			factors: [
				{
					enabled_at: "2026-05-23T00:00:00Z",
					id: 7,
					last_used_at: null,
					method: "totp" as const,
					name: "Phone",
				},
			],
			recovery_codes_remaining: 6,
		};
		mockState.get.mockResolvedValue(enabledStatus);
		mockState.post.mockResolvedValue({
			factor: enabledStatus.factors[0],
			recovery_codes: ["ABCD-EFGH-IJKL"],
		});

		const [first, second] = await Promise.all([
			authService.getMfaStatus(),
			authService.getMfaStatus(),
		]);
		first.factors[0].name = "mutated";
		await expect(authService.getMfaStatus()).resolves.toEqual(second);
		expect(mockState.get).toHaveBeenCalledTimes(1);

		await expect(authService.getMfaStatus({ force: true })).resolves.toEqual(
			second,
		);
		expect(mockState.get).toHaveBeenCalledTimes(2);

		await authService.finishTotpSetup({
			flow_token: "setup-flow",
			code: "123456",
		});
		await authService.getMfaStatus();
		expect(mockState.get).toHaveBeenCalledTimes(3);
	});

	it("uses the expected external auth endpoints and payloads", async () => {
		const provider: ExternalAuthPublicProvider = {
			display_name: "Example IDP",
			icon_url: null,
			key: "team/idp",
			kind: "oidc",
		};
		mockState.get.mockResolvedValueOnce([provider]);
		mockState.post.mockImplementation((url: string) => {
			if (url === "/auth/external-auth/email-verification/start") {
				return { message: "sent" };
			}
			if (url.endsWith("/start")) {
				return { authorization_url: "https://idp.example.com/authorize" };
			}
			if (url === "/auth/external-auth/password-link") {
				return { expires_in: 1200 };
			}
			return undefined;
		});

		await expect(authService.listExternalAuthProviders()).resolves.toEqual([
			provider,
		]);
		expect(
			authService.startExternalAuthLogin(provider, {
				return_path: "/files",
			}),
		).toEqual({
			authorization_url: "https://idp.example.com/authorize",
		});
		expect(
			authService.startExternalAuthEmailVerification({
				email: "alice@example.com",
				flow_token: "flow-token",
			}),
		).toEqual({ message: "sent" });
		await expect(
			authService.linkExternalAuthWithPassword({
				flow_token: "flow-token",
				identifier: "alice@example.com",
				password: "secret",
			}),
		).resolves.toEqual({ status: "authenticated", expiresIn: 1200 });

		expect(mockState.get).toHaveBeenCalledWith("/auth/external-auth/providers");
		expect(mockState.post).toHaveBeenNthCalledWith(
			1,
			"/auth/external-auth/oidc/team%2Fidp/start",
			{ return_path: "/files" },
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			2,
			"/auth/external-auth/email-verification/start",
			{
				email: "alice@example.com",
				flow_token: "flow-token",
			},
		);
		expect(mockState.post).toHaveBeenNthCalledWith(
			3,
			"/auth/external-auth/password-link",
			{
				flow_token: "flow-token",
				identifier: "alice@example.com",
				password: "secret",
			},
		);
	});

	it("caches passkey lists, clones cached results, and supports forced refresh", async () => {
		const phone = passkey({ id: 1, name: "Phone" });
		const laptop = passkey({ id: 2, name: "Laptop" });
		mockState.get
			.mockResolvedValueOnce([phone])
			.mockResolvedValueOnce([laptop]);

		const first = await authService.listPasskeys();
		first[0].name = "Mutated";

		await expect(authService.listPasskeys()).resolves.toEqual([phone]);
		expect(mockState.get).toHaveBeenCalledTimes(1);

		await expect(authService.listPasskeys({ force: true })).resolves.toEqual([
			laptop,
		]);
		expect(mockState.get).toHaveBeenCalledTimes(2);
	});

	it("deduplicates concurrent passkey list requests and retries after failures", async () => {
		const phone = passkey({ id: 1, name: "Phone" });
		const laptop = passkey({ id: 2, name: "Laptop" });
		let resolveFirst: ((value: PasskeyInfo[]) => void) | null = null;
		mockState.get.mockImplementationOnce(
			() =>
				new Promise<PasskeyInfo[]>((resolve) => {
					resolveFirst = resolve;
				}),
		);

		const first = authService.listPasskeys();
		const second = authService.listPasskeys();
		resolveFirst?.([phone]);

		await expect(Promise.all([first, second])).resolves.toEqual([
			[phone],
			[phone],
		]);
		expect(mockState.get).toHaveBeenCalledTimes(1);

		invalidatePasskeysCache();
		const error = new Error("load failed");
		mockState.get.mockRejectedValueOnce(error).mockResolvedValueOnce([laptop]);

		await expect(authService.listPasskeys()).rejects.toBe(error);
		await expect(authService.listPasskeys()).resolves.toEqual([laptop]);
		expect(mockState.get).toHaveBeenCalledTimes(3);
	});

	it("keeps the passkey cache in sync after create, rename, and delete", async () => {
		const phone = passkey({ id: 1, name: "Phone" });
		const laptop = passkey({ id: 2, name: "Laptop" });
		const renamed = passkey({ id: 2, name: "Work laptop" });
		mockState.get.mockResolvedValueOnce([phone]);
		mockState.post.mockResolvedValueOnce(laptop);
		mockState.patch.mockResolvedValueOnce(renamed);
		mockState.delete.mockResolvedValueOnce(undefined);

		await expect(authService.listPasskeys()).resolves.toEqual([phone]);
		await expect(
			authService.finishPasskeyRegistration(
				"register-flow",
				{ id: "cred" },
				"Laptop",
			),
		).resolves.toEqual(laptop);
		await expect(authService.listPasskeys()).resolves.toEqual([laptop, phone]);
		expect(mockState.get).toHaveBeenCalledTimes(1);

		await expect(
			authService.renamePasskey(2, { name: "Work laptop" }),
		).resolves.toEqual(renamed);
		await expect(authService.listPasskeys()).resolves.toEqual([renamed, phone]);
		expect(mockState.get).toHaveBeenCalledTimes(1);

		await authService.deletePasskey(2);
		await expect(authService.listPasskeys()).resolves.toEqual([phone]);
		expect(mockState.get).toHaveBeenCalledTimes(1);
	});

	it("caches external auth link lists, clones cached results, and supports forced refresh", async () => {
		const firstLink = externalAuthLink({ id: 1, subject: "subject-1" });
		const secondLink = externalAuthLink({ id: 2, subject: "subject-2" });
		mockState.get
			.mockResolvedValueOnce([firstLink])
			.mockResolvedValueOnce([secondLink]);

		const first = await authService.listExternalAuthLinks();
		first[0].subject = "mutated";

		await expect(authService.listExternalAuthLinks()).resolves.toEqual([
			firstLink,
		]);
		expect(mockState.get).toHaveBeenCalledTimes(1);

		await expect(
			authService.listExternalAuthLinks({ force: true }),
		).resolves.toEqual([secondLink]);
		expect(mockState.get).toHaveBeenCalledTimes(2);
	});

	it("deduplicates concurrent external auth link list requests and retries after failures", async () => {
		const firstLink = externalAuthLink({ id: 1, subject: "subject-1" });
		const secondLink = externalAuthLink({ id: 2, subject: "subject-2" });
		let resolveFirst: ((value: ExternalAuthLinkInfo[]) => void) | null = null;
		mockState.get.mockImplementationOnce(
			() =>
				new Promise<ExternalAuthLinkInfo[]>((resolve) => {
					resolveFirst = resolve;
				}),
		);

		const first = authService.listExternalAuthLinks();
		const second = authService.listExternalAuthLinks();
		resolveFirst?.([firstLink]);

		await expect(Promise.all([first, second])).resolves.toEqual([
			[firstLink],
			[firstLink],
		]);
		expect(mockState.get).toHaveBeenCalledTimes(1);

		invalidateExternalAuthLinksCache();
		const error = new Error("load failed");
		mockState.get
			.mockRejectedValueOnce(error)
			.mockResolvedValueOnce([secondLink]);

		await expect(authService.listExternalAuthLinks()).rejects.toBe(error);
		await expect(authService.listExternalAuthLinks()).resolves.toEqual([
			secondLink,
		]);
		expect(mockState.get).toHaveBeenCalledTimes(3);
	});

	it("keeps the external auth link cache in sync after delete", async () => {
		const firstLink = externalAuthLink({ id: 1, subject: "subject-1" });
		const secondLink = externalAuthLink({ id: 2, subject: "subject-2" });
		mockState.get.mockResolvedValueOnce([firstLink, secondLink]);
		mockState.delete.mockResolvedValueOnce(undefined);

		await expect(authService.listExternalAuthLinks()).resolves.toEqual([
			firstLink,
			secondLink,
		]);
		await authService.deleteExternalAuthLink(2);
		await expect(authService.listExternalAuthLinks()).resolves.toEqual([
			firstLink,
		]);
		expect(mockState.get).toHaveBeenCalledTimes(1);
	});

	it("invalidates passkey cache across auth identity changes", async () => {
		const phone = passkey({ id: 1, name: "Phone" });
		const laptop = passkey({ id: 2, name: "Laptop" });
		mockState.get
			.mockResolvedValueOnce([phone])
			.mockResolvedValueOnce([laptop]);
		mockState.post.mockResolvedValue({ expires_in: 900 });

		await expect(authService.listPasskeys()).resolves.toEqual([phone]);
		await authService.login("bob@example.com", "secret");
		await expect(authService.listPasskeys()).resolves.toEqual([laptop]);

		expect(mockState.get).toHaveBeenCalledTimes(2);
	});

	it("invalidates external auth link cache across auth identity changes", async () => {
		const firstLink = externalAuthLink({ id: 1, subject: "subject-1" });
		const secondLink = externalAuthLink({ id: 2, subject: "subject-2" });
		mockState.get
			.mockResolvedValueOnce([firstLink])
			.mockResolvedValueOnce([secondLink]);
		mockState.post.mockResolvedValue({ expires_in: 900 });

		await expect(authService.listExternalAuthLinks()).resolves.toEqual([
			firstLink,
		]);
		await authService.login("bob@example.com", "secret");
		await expect(authService.listExternalAuthLinks()).resolves.toEqual([
			secondLink,
		]);

		expect(mockState.get).toHaveBeenCalledTimes(2);
	});

	it("uploads avatars through multipart form data and unwraps API responses", async () => {
		const profile = {
			avatar: {
				source: "upload",
				url_512: "/avatars/1.webp",
				url_1024: "/avatars/1@2x.webp",
				version: 2,
			},
			display_name: "Alice",
		};
		mockState.clientPost.mockResolvedValue({
			data: {
				code: ErrorCode.Success,
				data: profile,
				msg: "",
			},
		});

		const file = new File(["avatar"], "avatar.png", { type: "image/png" });

		await expect(authService.uploadAvatar(file)).resolves.toBe(profile);

		expect(mockState.clientPost).toHaveBeenCalledWith(
			"/auth/profile/avatar/upload",
			expect.any(FormData),
			{
				headers: {
					"Content-Type": "multipart/form-data",
				},
			},
		);
		const formData = mockState.clientPost.mock.calls[0]?.[1] as FormData;
		expect(formData.get("file")).toBe(file);
	});

	it("throws ApiError details when avatar upload returns an error envelope", async () => {
		mockState.clientPost.mockResolvedValue({
			data: {
				code: 1000,
				error: {
					internal_code: "E001",
					retryable: true,
					subcode: ApiSubcode.AvatarRenderFailed,
				},
				msg: "upload failed",
			},
		});

		await expect(
			authService.uploadAvatar(
				new File(["avatar"], "avatar.png", { type: "image/png" }),
			),
		).rejects.toMatchObject({
			code: 1000,
			internalCode: "E001",
			message: "upload failed",
			retryable: true,
			subcode: ApiSubcode.AvatarRenderFailed,
		});
	});
});
