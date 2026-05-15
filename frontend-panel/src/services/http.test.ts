import { beforeEach, describe, expect, it, vi } from "vitest";
import { ErrorCode } from "@/types/api-helpers";

type MockAxiosError = {
	config?: { _retry?: boolean; url?: string };
	isAxiosError?: boolean;
	response?: { status: number };
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
				code: ErrorCode.Success,
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
				code: ErrorCode.Success,
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

	it("throws ApiError when the backend response code is not success", async () => {
		mockState.client.get.mockResolvedValue({
			data: {
				code: ErrorCode.Forbidden,
				msg: "forbidden",
				data: null,
			},
		});

		const { ApiError, api } = await loadHttpModule();

		await expect(api.get("/files")).rejects.toEqual(
			expect.objectContaining({
				code: ErrorCode.Forbidden,
				message: "forbidden",
			}),
		);
		await expect(api.get("/files")).rejects.toBeInstanceOf(ApiError);
	});

	it("preserves backend error details on ApiError", async () => {
		mockState.client.get.mockResolvedValue({
			data: {
				code: ErrorCode.StorageTransientFailure,
				msg: "Storage Driver Error",
				error: {
					internal_code: "E031",
					subcode: "storage.transient",
					retryable: true,
				},
			},
		});

		const { api } = await loadHttpModule();

		await expect(api.get("/files")).rejects.toEqual(
			expect.objectContaining({
				code: ErrorCode.StorageTransientFailure,
				message: "Storage Driver Error",
				internalCode: "E031",
				subcode: "storage.transient",
				retryable: true,
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
				code: ErrorCode.Success,
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
				response: { status: 401 },
			}),
		).resolves.toEqual({
			data: {
				code: ErrorCode.Success,
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
