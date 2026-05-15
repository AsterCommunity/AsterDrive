import { beforeEach, describe, expect, it, vi } from "vitest";
import { authService } from "@/services/authService";
import { ErrorCode } from "@/types/api-helpers";

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
		mockState.clientPost.mockReset();
		mockState.delete.mockReset();
		mockState.get.mockReset();
		mockState.patch.mockReset();
		mockState.post.mockReset();
		mockState.put.mockReset();
	});

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
		mockState.get.mockImplementation((url: string) => {
			if (url === "/auth/sessions") {
				return [];
			}
			if (url === "/auth/passkeys") {
				return [];
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
		expect(authService.listPasskeys()).toEqual([]);
		authService.startPasskeyRegistration({ name: "Laptop" });
		authService.finishPasskeyRegistration(
			"register-flow",
			{ id: "cred" },
			"Laptop",
		);
		authService.renamePasskey(1, { name: "Phone" });
		authService.deletePasskey(1);
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
			"/auth/sessions/session-1",
		);
		expect(mockState.delete).toHaveBeenNthCalledWith(
			3,
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
					subcode: "avatar-too-large",
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
			subcode: "avatar-too-large",
		});
	});
});
