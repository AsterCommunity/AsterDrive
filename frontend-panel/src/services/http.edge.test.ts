import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiErrorCode } from "@/types/api-helpers";

type MockAxiosError = {
	config?: { _retry?: boolean; optionalAuth?: boolean; url?: string };
	isAxiosError?: boolean;
	response?: { data?: unknown; status: number };
};

type MockRequestConfig = {
	headers?: Record<string, string>;
	method?: string;
	url?: string;
};

function tokenErrorResponse(code = ApiErrorCode.TokenExpired) {
	return {
		status: 401,
		data: {
			code,
			msg:
				code === ApiErrorCode.TokenInvalid
					? "session revoked"
					: "token expired",
		},
	};
}

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
		post: vi.fn(),
		isAxiosError: vi.fn(
			(error: unknown) => !!(error as MockAxiosError | undefined)?.isAxiosError,
		),
	};

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
		logout: vi.fn(async () => undefined),
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

vi.mock("@/lib/crossTabRefresh", () => ({
	isCrossTabRefreshAuthFailure: (error: unknown) =>
		typeof error === "object" &&
		error !== null &&
		"crossTabRefreshAuthFailure" in error &&
		error.crossTabRefreshAuthFailure === true,
}));

async function loadHttpModule() {
	vi.resetModules();
	return await import("@/services/http");
}

describe("http refresh edge cases", () => {
	beforeEach(() => {
		mockState.axiosHeaders.from.mockClear();
		mockState.axiosModule.create.mockClear();
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
		Object.defineProperty(window, "location", {
			configurable: true,
			value: {
				href: "http://localhost/",
			},
		});
	});

	it("keeps the original error when refresh fails due to a network problem", async () => {
		const refreshError = new Error("offline");
		mockState.refreshToken.mockRejectedValue(refreshError);
		mockState.axiosModule.isAxiosError.mockReturnValue(false);
		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();
		const originalError = {
			config: { url: "/files", _retry: false },
			response: tokenErrorResponse(),
		} satisfies MockAxiosError;

		await expect(errorHandler(originalError)).rejects.toBe(originalError);
		expect(mockState.forceLogout).not.toHaveBeenCalled();
		expect(window.location.href).toBe("http://localhost/");
	});

	it("forces logout when refresh fails with an auth response", async () => {
		mockState.axiosModule.isAxiosError.mockReturnValue(true);
		mockState.refreshToken.mockRejectedValue({
			isAxiosError: true,
			response: { status: 401 },
		});
		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();
		const originalError = {
			config: { url: "/files", _retry: false },
			response: tokenErrorResponse(),
		} satisfies MockAxiosError;

		await expect(errorHandler(originalError)).rejects.toBe(originalError);
		expect(mockState.forceLogout).toHaveBeenCalledTimes(1);
		expect(window.location.href).toBe("/login");
	});

	it("keeps a public page in guest mode when optional auth refresh fails", async () => {
		mockState.axiosModule.isAxiosError.mockReturnValue(true);
		mockState.refreshToken.mockRejectedValue({
			isAxiosError: true,
			response: { status: 401 },
		});
		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();
		const originalError = {
			config: { optionalAuth: true, url: "/auth/me", _retry: false },
			response: tokenErrorResponse(ApiErrorCode.TokenMissing),
		} satisfies MockAxiosError;

		await expect(errorHandler(originalError)).rejects.toBe(originalError);
		expect(mockState.refreshToken).toHaveBeenCalledTimes(1);
		expect(mockState.forceLogout).not.toHaveBeenCalled();
		expect(window.location.href).toBe("http://localhost/");
	});

	it("does not force logout when refresh fails with a transient gateway response", async () => {
		mockState.axiosModule.isAxiosError.mockReturnValue(true);
		mockState.refreshToken.mockRejectedValue({
			isAxiosError: true,
			response: { status: 502 },
		});
		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();
		const originalError = {
			config: { url: "/files", _retry: false },
			response: tokenErrorResponse(),
		} satisfies MockAxiosError;

		await expect(errorHandler(originalError)).rejects.toBe(originalError);
		expect(mockState.forceLogout).not.toHaveBeenCalled();
		expect(window.location.href).toBe("http://localhost/");
	});

	it("forces logout when refresh fails with a token ApiError", async () => {
		mockState.axiosModule.isAxiosError.mockReturnValue(false);
		mockState.refreshToken.mockRejectedValue({
			code: ApiErrorCode.TokenInvalid,
			message: "session revoked",
		});
		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();
		const originalError = {
			config: { url: "/files", _retry: false },
			response: tokenErrorResponse(ApiErrorCode.TokenInvalid),
		} satisfies MockAxiosError;

		await expect(errorHandler(originalError)).rejects.toBe(originalError);
		expect(mockState.forceLogout).toHaveBeenCalledTimes(1);
		expect(window.location.href).toBe("/login");
	});

	it("queues concurrent 401 retries behind a single refresh call", async () => {
		let resolveRefresh: (() => void) | undefined;
		mockState.refreshToken.mockReturnValue(
			new Promise((resolve) => {
				resolveRefresh = () => resolve({});
			}),
		);
		mockState.client.mockResolvedValue({
			data: {
				code: ApiErrorCode.Success,
				msg: "ok",
				data: { retried: true },
			},
		});
		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();

		const first = errorHandler({
			config: { url: "/files/1", _retry: false },
			response: tokenErrorResponse(),
		});
		const second = errorHandler({
			config: { url: "/files/2", _retry: false },
			response: tokenErrorResponse(),
		});

		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(mockState.refreshToken).toHaveBeenCalledTimes(1);

		resolveRefresh?.();

		await expect(Promise.all([first, second])).resolves.toEqual([
			expect.objectContaining({
				data: expect.objectContaining({
					data: { retried: true },
				}),
			}),
			expect.objectContaining({
				data: expect.objectContaining({
					data: { retried: true },
				}),
			}),
		]);
		expect(mockState.client).toHaveBeenCalledTimes(2);
	});

	it("returns the original 401 when a shared refresh promise rejects", async () => {
		let rejectRefresh: ((error: Error) => void) | undefined;
		mockState.refreshToken.mockReturnValue(
			new Promise((_, reject) => {
				rejectRefresh = reject;
			}),
		);
		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();
		const firstError = {
			config: { url: "/files/1", _retry: false },
			response: tokenErrorResponse(),
		} satisfies MockAxiosError;
		const secondError = {
			config: { url: "/files/2", _retry: false },
			response: tokenErrorResponse(),
		} satisfies MockAxiosError;

		const first = errorHandler(firstError);
		const second = errorHandler(secondError);
		rejectRefresh?.(new Error("peer auth refresh timed out"));

		await expect(first).rejects.toBe(firstError);
		await expect(second).rejects.toBe(secondError);
		expect(mockState.forceLogout).not.toHaveBeenCalled();
	});

	it("forces logout when a shared refresh fails because another tab saw auth failure", async () => {
		const peerAuthError = Object.assign(new Error("peer auth refresh failed"), {
			crossTabRefreshAuthFailure: true,
		});
		mockState.refreshToken.mockRejectedValue(peerAuthError);
		mockState.axiosModule.isAxiosError.mockReturnValue(false);
		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();
		const originalError = {
			config: { url: "/files", _retry: false },
			response: tokenErrorResponse(),
		} satisfies MockAxiosError;

		await expect(errorHandler(originalError)).rejects.toBe(originalError);
		expect(mockState.forceLogout).toHaveBeenCalledTimes(1);
		expect(window.location.href).toBe("/login");
	});

	it("does not crash or refresh when a 401 has no request config", async () => {
		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();
		const originalError = {
			response: { status: 401 },
		} satisfies MockAxiosError;

		await expect(errorHandler(originalError)).rejects.toBe(originalError);
		expect(mockState.refreshToken).not.toHaveBeenCalled();
		expect(mockState.client).not.toHaveBeenCalled();
	});

	it("does not recursively refresh the refresh endpoint", async () => {
		await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();
		const originalError = {
			config: { url: "/auth/refresh", _retry: false },
			response: { status: 401 },
		} satisfies MockAxiosError;

		await expect(errorHandler(originalError)).rejects.toBe(originalError);
		expect(mockState.refreshToken).not.toHaveBeenCalled();
		expect(mockState.client).not.toHaveBeenCalled();
	});

	it("converts non-2xx API payloads into ApiError instances", async () => {
		const { ApiError } = await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();

		await expect(
			errorHandler({
				config: { url: "/auth/login" },
				response: {
					status: 403,
					data: {
						code: ApiErrorCode.PendingActivation,
						msg: "pending activation",
					},
				},
			}),
		).rejects.toEqual(
			expect.objectContaining({
				code: ApiErrorCode.PendingActivation,
				message: "pending activation",
			}),
		);
		await expect(
			errorHandler({
				config: { url: "/auth/login" },
				response: {
					status: 403,
					data: {
						code: ApiErrorCode.PendingActivation,
						msg: "pending activation",
					},
				},
			}),
		).rejects.toBeInstanceOf(ApiError);
	});

	it("preserves HTTP status when converting API payloads into ApiError", async () => {
		const { ApiError } = await loadHttpModule();
		const errorHandler = mockState.getErrorHandler();

		const converted = await errorHandler({
			config: { url: "/auth/login" },
			response: {
				status: 403,
				data: {
					code: ApiErrorCode.PendingActivation,
					msg: "pending activation",
				},
			},
		}).catch((error: unknown) => error);

		expect(converted).toBeInstanceOf(ApiError);
		expect(converted).toEqual(
			expect.objectContaining({
				code: ApiErrorCode.PendingActivation,
				message: "pending activation",
				status: 403,
			}),
		);
	});
});
