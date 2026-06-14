import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiErrorCode } from "@/types/api-helpers";

type MockAxiosError = {
	config?: { _retry?: boolean; responseType?: string; url?: string };
	isAxiosError?: boolean;
	response?: { data?: unknown; status: number };
};

type MockRequestConfig = {
	headers?: Record<string, string>;
	method?: string;
	url?: string;
};

function setTestCookie(cookie: string) {
	// biome-ignore lint/suspicious/noDocumentCookie: jsdom tests need direct cookie mutation.
	document.cookie = cookie;
}

const mockState = vi.hoisted(() => {
	let requestHandler:
		| ((
				config: MockRequestConfig,
		  ) => MockRequestConfig | Promise<MockRequestConfig>)
		| undefined;
	let errorHandler: ((error: MockAxiosError) => Promise<unknown>) | undefined;
	const axiosHeaders = {
		from: vi.fn((headers: Record<string, string> = {}) => {
			const nextHeaders: Record<string, string> & {
				get: (name: string) => string | undefined;
				set: (name: string, value: string) => void;
			} = {
				...headers,
				get(name) {
					const key = Object.keys(this).find(
						(headerName) => headerName.toLowerCase() === name.toLowerCase(),
					);
					return key ? this[key] : undefined;
				},
				set(name, value) {
					this[name] = value;
				},
			};
			return nextHeaders;
		}),
	};

	const client = vi.fn();
	client.get = vi.fn();
	client.post = vi.fn();
	client.put = vi.fn();
	client.patch = vi.fn();
	client.delete = vi.fn();
	client.interceptors = {
		request: {
			use: vi.fn(
				(
					success: (
						config: MockRequestConfig,
					) => MockRequestConfig | Promise<MockRequestConfig>,
				) => {
					requestHandler = success;
					return 0;
				},
			),
		},
		response: {
			use: vi.fn(
				(
					_success: unknown,
					failure: (error: MockAxiosError) => Promise<unknown>,
				) => {
					errorHandler = failure;
					return 0;
				},
			),
		},
	};

	const axiosModule = {
		create: vi.fn(() => client),
		isCancel: vi.fn(() => false),
		post: vi.fn(),
		isAxiosError: vi.fn(
			(error: unknown) => !!(error as MockAxiosError | undefined)?.isAxiosError,
		),
	};

	const logout = vi.fn(async () => undefined);

	return {
		axiosHeaders,
		axiosModule,
		client,
		forceLogout: vi.fn(),
		getRequestHandler: () => {
			if (!requestHandler) throw new Error("request handler not registered");
			return requestHandler;
		},
		getErrorHandler: () => {
			if (!errorHandler)
				throw new Error("response error handler not registered");
			return errorHandler;
		},
		logout,
		refreshToken: vi.fn(async () => undefined),
	};
});

vi.mock("axios", () => ({
	AxiosHeaders: mockState.axiosHeaders,
	default: mockState.axiosModule,
}));

vi.mock("@/stores/authStore", () => ({
	forceLogout: mockState.forceLogout,
	useAuthStore: {
		getState: () => ({
			logout: mockState.logout,
			refreshToken: mockState.refreshToken,
		}),
	},
}));

async function loadHttpModule() {
	vi.resetModules();
	return await import("@/services/http");
}

describe("http api helpers", () => {
	beforeEach(() => {
		mockState.axiosHeaders.from.mockClear();
		mockState.axiosModule.create.mockClear();
		mockState.axiosModule.isCancel.mockClear();
		mockState.axiosModule.isAxiosError.mockClear();
		mockState.axiosModule.post.mockReset();
		mockState.client.mockReset();
		mockState.client.delete.mockReset();
		mockState.client.get.mockReset();
		mockState.client.patch.mockReset();
		mockState.client.post.mockReset();
		mockState.client.put.mockReset();
		mockState.client.interceptors.request.use.mockClear();
		mockState.client.interceptors.response.use.mockClear();
		mockState.forceLogout.mockClear();
		mockState.logout.mockClear();
		mockState.refreshToken.mockReset();
		mockState.refreshToken.mockResolvedValue(undefined);
		setTestCookie("aster_csrf=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/");
	});

	it("unwraps successful responses from api.get", async () => {
		mockState.client.get.mockResolvedValue({
			data: {
				code: ApiErrorCode.Success,
				msg: "ok",
				data: { id: 7 },
			},
		});

		const { api } = await loadHttpModule();

		await expect(api.get("/files", { params: { limit: 10 } })).resolves.toEqual(
			{
				id: 7,
			},
		);
		expect(mockState.client.get).toHaveBeenCalledWith("/files", {
			params: { limit: 10 },
		});
	});

	it("forwards abort signals to axios requests", async () => {
		const controller = new AbortController();
		mockState.client.get.mockResolvedValue({
			data: {
				code: ApiErrorCode.Success,
				msg: "ok",
				data: { id: 8 },
			},
		});

		const { api } = await loadHttpModule();

		await expect(
			api.get("/files", { signal: controller.signal }),
		).resolves.toEqual({
			id: 8,
		});
		expect(mockState.client.get).toHaveBeenCalledWith("/files", {
			signal: controller.signal,
		});
	});

	it("unwraps successful responses from mutating api helpers", async () => {
		const successResponse = (data: unknown) => ({
			data: {
				code: ApiErrorCode.Success,
				msg: "ok",
				data,
			},
		});
		mockState.client.post.mockResolvedValue(successResponse({ created: true }));
		mockState.client.put.mockResolvedValue(successResponse({ replaced: true }));
		mockState.client.patch.mockResolvedValue(
			successResponse({ patched: true }),
		);
		mockState.client.delete.mockResolvedValue(
			successResponse({ deleted: true }),
		);

		const { api } = await loadHttpModule();

		await expect(api.post("/items", { name: "draft" })).resolves.toEqual({
			created: true,
		});
		await expect(api.put("/items/1", { name: "replacement" })).resolves.toEqual(
			{
				replaced: true,
			},
		);
		await expect(api.patch("/items/1", { name: "patch" })).resolves.toEqual({
			patched: true,
		});
		await expect(api.delete("/items/1")).resolves.toEqual({
			deleted: true,
		});
		expect(mockState.client.post).toHaveBeenCalledWith(
			"/items",
			{ name: "draft" },
			undefined,
		);
		expect(mockState.client.put).toHaveBeenCalledWith(
			"/items/1",
			{ name: "replacement" },
			undefined,
		);
		expect(mockState.client.patch).toHaveBeenCalledWith(
			"/items/1",
			{ name: "patch" },
			undefined,
		);
		expect(mockState.client.delete).toHaveBeenCalledWith("/items/1", undefined);
	});

	it("throws ApiError when the backend response code is not success", async () => {
		mockState.client.get.mockResolvedValue({
			data: {
				code: ApiErrorCode.Forbidden,
				msg: "forbidden",
				data: null,
			},
		});

		const { ApiError, api } = await loadHttpModule();

		await expect(api.get("/files")).rejects.toEqual(
			expect.objectContaining({
				code: ApiErrorCode.Forbidden,
				message: "forbidden",
			}),
		);
		await expect(api.get("/files")).rejects.toBeInstanceOf(ApiError);
	});

	it("throws ApiPendingError for accepted processing responses", async () => {
		mockState.client.get.mockResolvedValue({
			status: 202,
			headers: {
				"retry-after": "3",
			},
			data: {
				code: ApiErrorCode.Success,
				msg: "",
				data: null,
			},
		});

		const { ApiPendingError, api } = await loadHttpModule();

		await expect(api.get("/files/1/archive-preview")).rejects.toEqual(
			expect.objectContaining({
				message: "Request is still processing",
				retryAfterSeconds: 3,
			}),
		);
		await expect(api.get("/files/1/archive-preview")).rejects.toBeInstanceOf(
			ApiPendingError,
		);
	});

	it("preserves retryable backend error details on ApiError", async () => {
		mockState.client.get.mockResolvedValue({
			data: {
				code: ApiErrorCode.StorageTransient,
				msg: "remote timeout",
				error: {
					retryable: true,
				},
			},
		});

		const { api } = await loadHttpModule();

		await expect(api.get("/files")).rejects.toEqual(
			expect.objectContaining({
				code: ApiErrorCode.StorageTransient,
				message: "remote timeout",
				retryable: true,
			}),
		);
	});

	it("preserves specific auth error codes on ApiError", async () => {
		mockState.client.get.mockResolvedValue({
			data: {
				code: ApiErrorCode.AuthRegistrationDisabled,
				msg: "new user registration is disabled",
				error: { retryable: false },
			},
		});

		const { api } = await loadHttpModule();

		await expect(api.get("/auth/register")).rejects.toEqual(
			expect.objectContaining({
				code: ApiErrorCode.AuthRegistrationDisabled,
				message: "new user registration is disabled",
				retryable: false,
			}),
		);
	});

	it("ignores unknown top-level API codes", async () => {
		mockState.client.get.mockResolvedValue({
			data: {
				code: "remote.dynamic",
				msg: "denied",
			},
		});

		const { api } = await loadHttpModule();

		await expect(api.get("/files")).rejects.toEqual(
			expect.objectContaining({
				code: "remote.dynamic",
				message: "denied",
			}),
		);
	});

	it("keeps valid top-level ApiErrorCode when error info is malformed", async () => {
		mockState.client.get.mockResolvedValue({
			data: {
				code: ApiErrorCode.AuthRequestOriginUntrusted,
				msg: "denied",
				error: {
					retryable: "false",
				},
			},
		});

		const { api } = await loadHttpModule();

		await expect(api.get("/files")).rejects.toEqual(
			expect.objectContaining({
				code: ApiErrorCode.AuthRequestOriginUntrusted,
				message: "denied",
				retryable: undefined,
			}),
		);
	});

	it("ignores malformed ApiErrorInfo fields without failing extraction", async () => {
		mockState.client.get.mockResolvedValue({
			data: {
				code: ApiErrorCode.BadRequest,
				msg: "bad request",
				error: {
					retryable: "false",
				},
			},
		});

		const { api } = await loadHttpModule();

		await expect(api.get("/files")).rejects.toEqual(
			expect.objectContaining({
				code: ApiErrorCode.BadRequest,
				message: "bad request",
				retryable: undefined,
			}),
		);
	});

	it("adds the csrf header to unsafe requests when the cookie exists", async () => {
		setTestCookie("aster_csrf=csrf-token-1; path=/");

		await loadHttpModule();
		const requestHandler = mockState.getRequestHandler();

		expect(
			requestHandler({
				method: "post",
				headers: {},
				url: "/auth/profile",
			}),
		).toMatchObject({
			headers: {
				"X-CSRF-Token": "csrf-token-1",
			},
		});
	});

	it("does not add the csrf header to safe requests", async () => {
		setTestCookie("aster_csrf=csrf-token-1; path=/");

		await loadHttpModule();
		const requestHandler = mockState.getRequestHandler();

		expect(
			requestHandler({
				method: "get",
				headers: {},
				url: "/auth/me",
			}),
		).toMatchObject({
			headers: {},
		});
	});

	it("refreshes and retries a protected request after a 401", async () => {
		mockState.client.mockResolvedValue({
			data: {
				code: ApiErrorCode.Success,
				msg: "ok",
				data: { retried: true },
			},
		});

		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();
		const originalRequest = { url: "/files", _retry: false };

		await expect(
			errorHandler({
				config: originalRequest,
				response: {
					status: 401,
					data: {
						code: ApiErrorCode.TokenExpired,
						msg: "Token Expired",
					},
				},
			}),
		).resolves.toEqual({
			data: {
				code: ApiErrorCode.Success,
				msg: "ok",
				data: { retried: true },
			},
		});
		expect(mockState.refreshToken).toHaveBeenCalledTimes(1);
		expect(mockState.client).toHaveBeenCalledWith(
			expect.objectContaining({
				url: "/files",
				_retry: true,
			}),
		);
	});

	it("refreshes and retries a protected blob request when the 401 body is an API error blob", async () => {
		mockState.client.mockResolvedValue({
			data: {
				code: ApiErrorCode.Success,
				msg: "ok",
				data: new Blob(["retried"]),
			},
		});

		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();
		const originalRequest = {
			responseType: "blob",
			url: "/files/1/download",
			_retry: false,
		};

		await expect(
			errorHandler({
				config: originalRequest,
				response: {
					status: 401,
					data: new Blob([
						JSON.stringify({
							code: ApiErrorCode.TokenExpired,
							msg: "Token Expired",
						}),
					]),
				},
			}),
		).resolves.toEqual({
			data: {
				code: ApiErrorCode.Success,
				msg: "ok",
				data: expect.any(Blob),
			},
		});
		expect(mockState.refreshToken).toHaveBeenCalledTimes(1);
		expect(mockState.client).toHaveBeenCalledWith(
			expect.objectContaining({
				responseType: "blob",
				url: "/files/1/download",
				_retry: true,
			}),
		);
	});

	it("does not parse skipped endpoint error blobs before rejecting", async () => {
		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();
		const loginErrorBlob = new Blob([
			JSON.stringify({
				code: ApiErrorCode.TokenExpired,
				msg: "Token Expired",
			}),
		]);
		const textSpy = vi
			.spyOn(loginErrorBlob, "text")
			.mockResolvedValue("should not be read");
		const error = {
			config: {
				responseType: "blob",
				url: "/auth/login",
				_retry: false,
			},
			response: {
				status: 401,
				data: loginErrorBlob,
			},
		};

		await expect(errorHandler(error)).rejects.toBe(error);
		expect(textSpy).not.toHaveBeenCalled();
		expect(mockState.refreshToken).not.toHaveBeenCalled();
	});

	it("refreshes and retries a protected text request when the 401 body is an API error string", async () => {
		mockState.client.mockResolvedValue({
			data: "retried text",
			status: 200,
		});

		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();
		const originalRequest = {
			responseType: "text",
			url: "/files/1/download",
			_retry: false,
		};

		await expect(
			errorHandler({
				config: originalRequest,
				response: {
					status: 401,
					data: JSON.stringify({
						code: ApiErrorCode.TokenExpired,
						msg: "Token Expired",
					}),
				},
			}),
		).resolves.toEqual({
			data: "retried text",
			status: 200,
		});
		expect(mockState.refreshToken).toHaveBeenCalledTimes(1);
		expect(mockState.client).toHaveBeenCalledWith(
			expect.objectContaining({
				responseType: "text",
				url: "/files/1/download",
				_retry: true,
			}),
		);
	});

	it("refreshes and retries when the protected request has no access token", async () => {
		mockState.client.mockResolvedValue({
			data: {
				code: ApiErrorCode.Success,
				msg: "ok",
				data: { retried: true },
			},
		});

		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();
		const originalRequest = { url: "/auth/me", _retry: false };

		await expect(
			errorHandler({
				config: originalRequest,
				response: {
					status: 401,
					data: {
						code: ApiErrorCode.TokenMissing,
						msg: "missing token",
					},
				},
			}),
		).resolves.toEqual({
			data: {
				code: ApiErrorCode.Success,
				msg: "ok",
				data: { retried: true },
			},
		});
		expect(mockState.refreshToken).toHaveBeenCalledTimes(1);
		expect(mockState.client).toHaveBeenCalledWith(
			expect.objectContaining({
				url: "/auth/me",
				_retry: true,
			}),
		);
	});

	it("does not attempt refresh for non-token auth failures", async () => {
		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();
		const originalError = {
			config: { url: "/auth/mfa/factors/3", _retry: false },
			response: {
				status: 401,
				data: {
					code: ApiErrorCode.AuthMfaCodeInvalid,
					msg: "invalid MFA code",
				},
			},
		} satisfies MockAxiosError;

		await expect(errorHandler(originalError)).rejects.toEqual(
			expect.objectContaining({
				code: ApiErrorCode.AuthMfaCodeInvalid,
				message: "invalid MFA code",
			}),
		);
		expect(mockState.refreshToken).not.toHaveBeenCalled();
		expect(mockState.client).not.toHaveBeenCalled();
	});

	it("does not attempt refresh for public share endpoints", async () => {
		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();
		const originalError = {
			config: { url: "/s/token-1/content" },
			response: { status: 401 },
		} satisfies MockAxiosError;

		await expect(errorHandler(originalError)).rejects.toBe(originalError);
		expect(mockState.axiosModule.post).not.toHaveBeenCalled();
		expect(mockState.client).not.toHaveBeenCalled();
	});

	it("does not attempt refresh for public branding endpoints", async () => {
		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();
		const originalError = {
			config: { url: "/public/branding" },
			response: { status: 401 },
		} satisfies MockAxiosError;

		await expect(errorHandler(originalError)).rejects.toBe(originalError);
		expect(mockState.axiosModule.post).not.toHaveBeenCalled();
		expect(mockState.client).not.toHaveBeenCalled();
	});

	it("does not attempt refresh for passkey login endpoints", async () => {
		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();
		const originalError = {
			config: { url: "/auth/passkeys/login/start" },
			response: { status: 401 },
		} satisfies MockAxiosError;

		await expect(errorHandler(originalError)).rejects.toBe(originalError);
		expect(mockState.refreshToken).not.toHaveBeenCalled();
		expect(mockState.client).not.toHaveBeenCalled();

		const finishError = {
			config: { url: "/auth/passkeys/login/finish" },
			response: { status: 401 },
		} satisfies MockAxiosError;

		await expect(errorHandler(finishError)).rejects.toBe(finishError);
		expect(mockState.refreshToken).not.toHaveBeenCalled();
		expect(mockState.client).not.toHaveBeenCalled();
	});
});
