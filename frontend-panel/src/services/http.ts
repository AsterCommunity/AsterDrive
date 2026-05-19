import type {
	AxiosInstance,
	AxiosRequestConfig,
	InternalAxiosRequestConfig,
} from "axios";
import axios, { AxiosHeaders } from "axios";
import { config } from "@/config/app";
import { isCrossTabRefreshAuthFailure } from "@/lib/crossTabRefresh";
import {
	type ApiErrorInfo as ApiErrorInfoPayload,
	type ApiResponse,
	type ApiSubcode,
	ErrorCode,
	type ErrorCode as ErrorCodeType,
	isApiSubcode,
} from "@/types/api-helpers";
import { CSRF_HEADER_NAME, getCsrfToken } from "./csrf";

const client: AxiosInstance = axios.create({
	baseURL: config.apiBaseUrl,
	timeout: 30000,
	headers: { "Content-Type": "application/json" },
	withCredentials: true,
});

// 不需要自动 refresh 的路径
// FIXME: 如果登录流程新增/调整未认证端点，同步检查 passkey 登录路径是否仍应跳过 refresh。
const SKIP_REFRESH_PATHS = [
	"/auth/refresh",
	"/auth/login",
	"/auth/passkeys/login/start",
	"/auth/passkeys/login/finish",
	"/auth/register",
	"/auth/register/resend",
	"/auth/logout",
	"/auth/check",
	"/auth/contact-verification/confirm",
	"/auth/external-auth/providers",
	"/auth/external-auth/email-verification/start",
	"/auth/external-auth/password-link",
	"/auth/setup",
];

function shouldSkipRefresh(url: string) {
	if (SKIP_REFRESH_PATHS.some((path) => url.endsWith(path))) return true;
	if (url.includes("/auth/external-auth/") && url.endsWith("/start")) {
		return true;
	}
	return url.includes("/s/") || url.includes("/public/");
}

function isUnsafeMethod(method?: string) {
	return !["get", "head", "options", "trace"].includes(
		(method ?? "get").toLowerCase(),
	);
}

function hasHeader(
	headers: InternalAxiosRequestConfig["headers"],
	name: string,
): boolean {
	if (!headers) {
		return false;
	}

	if ("get" in headers && typeof headers.get === "function") {
		return headers.get(name) != null;
	}

	return Object.keys(headers).some(
		(key) => key.toLowerCase() === name.toLowerCase(),
	);
}

function setHeader(
	request: InternalAxiosRequestConfig,
	name: string,
	value: string,
) {
	if (
		request.headers &&
		"set" in request.headers &&
		typeof request.headers.set === "function"
	) {
		request.headers.set(name, value);
		return;
	}

	request.headers = AxiosHeaders.from(request.headers ?? {});
	request.headers.set(name, value);
}

let isRefreshing = false;
let refreshPromise: Promise<void> | null = null;

export type ApiRequestConfig = Pick<
	AxiosRequestConfig,
	"headers" | "params" | "signal"
>;

type ApiErrorDetails = {
	internalCode?: string;
	subcode?: ApiSubcode;
	retryable?: boolean;
};

export function isRequestCanceled(error: unknown): boolean {
	if (typeof axios.isCancel === "function" && axios.isCancel(error)) {
		return true;
	}

	if (typeof error !== "object" || error === null) {
		return false;
	}

	const code = "code" in error ? error.code : null;
	const name = "name" in error ? error.name : null;
	return code === "ERR_CANCELED" || name === "AbortError";
}

client.interceptors.request.use((request) => {
	const csrfToken = getCsrfToken();
	if (!csrfToken || !isUnsafeMethod(request.method)) {
		return request;
	}

	if (!hasHeader(request.headers, CSRF_HEADER_NAME)) {
		setHeader(request, CSRF_HEADER_NAME, csrfToken);
	}
	return request;
});

client.interceptors.response.use(
	(res) => res,
	async (error) => {
		if (isRequestCanceled(error)) {
			return Promise.reject(error);
		}

		const original = error.config;
		const url = original?.url || "";

		// 跳过公开端点的自动 refresh（避免把分享页误当成登录态接口）
		const shouldSkip = shouldSkipRefresh(url);
		if (
			error.response?.status === 401 &&
			original &&
			!original._retry &&
			!shouldSkip
		) {
			original._retry = true;

			if (!isRefreshing) {
				isRefreshing = true;
				refreshPromise = (async () => {
					const { useAuthStore } = await import("@/stores/authStore");
					await useAuthStore.getState().refreshToken();
				})().finally(() => {
					isRefreshing = false;
					refreshPromise = null;
				});
			}

			try {
				await refreshPromise;
				return client(original);
			} catch (refreshError) {
				// 网络错误（离线）时不强制登出
				if (
					!isCrossTabRefreshAuthFailure(refreshError) &&
					(!axios.isAxiosError(refreshError) || !refreshError.response)
				) {
					return Promise.reject(error);
				}
				const { forceLogout } = await import("@/stores/authStore");
				forceLogout();
				window.location.href = "/login";
				return Promise.reject(error);
			}
		}
		return Promise.reject(extractApiError(error) ?? error);
	},
);

export class ApiError extends Error {
	code: ErrorCodeType;
	internalCode?: string;
	subcode?: ApiSubcode;
	retryable?: boolean;

	constructor(
		code: ErrorCodeType,
		message: string,
		details: ApiErrorDetails = {},
	) {
		super(message);
		this.code = code;
		this.internalCode = details.internalCode;
		this.subcode = details.subcode;
		this.retryable = details.retryable;
	}
}

export class ApiPendingError extends Error {
	retryAfterSeconds: number;

	constructor(message = "Request is still processing", retryAfterSeconds = 2) {
		super(message);
		this.retryAfterSeconds = retryAfterSeconds;
	}
}

function parseRetryAfterSeconds(value: string | null | undefined): number {
	if (!value) {
		return 2;
	}
	const seconds = Number.parseInt(value, 10);
	return Number.isFinite(seconds) && seconds > 0 ? seconds : 2;
}

function normalizeApiErrorInfo(
	value: ApiErrorInfoPayload | null | undefined,
): ApiErrorDetails {
	if (!value || typeof value !== "object") {
		return {};
	}

	return {
		internalCode:
			typeof value.internal_code === "string" ? value.internal_code : undefined,
		subcode:
			typeof value.subcode === "string" && isApiSubcode(value.subcode)
				? value.subcode
				: undefined,
		retryable:
			typeof value.retryable === "boolean" ? value.retryable : undefined,
	};
}

function extractApiError(error: unknown): ApiError | null {
	if (typeof error !== "object" || error === null) {
		return null;
	}

	const response =
		"response" in error && typeof error.response === "object"
			? error.response
			: null;
	if (response === null || response === undefined) {
		return null;
	}

	const data = "data" in response ? response.data : null;
	if (typeof data !== "object" || data === null) {
		return null;
	}

	const code = "code" in data ? data.code : null;
	const message = "msg" in data ? data.msg : null;
	if (typeof code !== "number" || typeof message !== "string") {
		return null;
	}

	const errorInfo =
		"error" in data && typeof data.error === "object" ? data.error : null;

	return new ApiError(
		code as ErrorCodeType,
		message,
		normalizeApiErrorInfo(errorInfo as ApiErrorInfoPayload | null),
	);
}

async function unwrap<T>(
	promise: Promise<{
		status: number;
		headers?: { [key: string]: unknown };
		data: ApiResponse<T>;
	}>,
): Promise<T> {
	const { status, headers, data: resp } = await promise;
	if (status === 202) {
		const retryAfter =
			typeof headers?.["retry-after"] === "string"
				? headers["retry-after"]
				: undefined;
		throw new ApiPendingError(
			"Request is still processing",
			parseRetryAfterSeconds(retryAfter),
		);
	}
	if (resp.code !== ErrorCode.Success) {
		throw new ApiError(resp.code, resp.msg, normalizeApiErrorInfo(resp.error));
	}
	return resp.data as T;
}

export const api = {
	get: <T>(url: string, config?: ApiRequestConfig) =>
		unwrap<T>(client.get(url, config)),
	post: <T>(url: string, data?: unknown, config?: ApiRequestConfig) =>
		unwrap<T>(client.post(url, data, config)),
	put: <T>(url: string, data?: unknown, config?: ApiRequestConfig) =>
		unwrap<T>(client.put(url, data, config)),
	patch: <T>(url: string, data?: unknown, config?: ApiRequestConfig) =>
		unwrap<T>(client.patch(url, data, config)),
	delete: <T>(url: string, config?: ApiRequestConfig) =>
		unwrap<T>(client.delete(url, config)),
	client,
};
