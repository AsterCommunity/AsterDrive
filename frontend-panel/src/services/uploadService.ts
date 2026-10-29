import { isAxiosError } from "axios";
import { config } from "@/config/app";
import { logger } from "@/lib/logger";
import { getUploadFrontendClientId } from "@/lib/uploadClientId";
import {
	buildWorkspacePath,
	PERSONAL_WORKSPACE,
	type Workspace,
} from "@/lib/workspace";
import { bindWorkspaceService } from "@/stores/workspaceStore";
import type {
	ApiErrorCode,
	ApiErrorInfo,
	ApiResponse,
	BrowserUploadInstruction,
	BrowserUploadObservation,
	BrowserUploadReport,
	BrowserUploadStep,
	ChunkUploadResponse,
	CompletedPart,
	FileInfo,
	InitUploadResponse,
	PresignedUploadRequest,
	RecoverableUploadSession,
	UploadProgressResponse,
} from "@/types/api";
import {
	ApiErrorCode as ApiErrorCodeValue,
	isApiErrorCode,
} from "@/types/api-helpers";
import { CSRF_HEADER_NAME, getCsrfToken } from "./csrf";
import { ApiError, api } from "./http";

export type {
	ChunkUploadResponse,
	CompletedPart,
	InitUploadResponse,
	PresignedUploadRequest,
	RecoverableUploadSession,
	UploadProgressResponse,
};

export class UploadRequestError extends Error {
	authFailure: boolean;
	isAborted: boolean;
	retryable: boolean;
	status?: number;

	constructor(
		message: string,
		options?: {
			authFailure?: boolean;
			isAborted?: boolean;
			retryable?: boolean;
			status?: number;
		},
	) {
		super(message);
		this.name = "UploadRequestError";
		this.authFailure = options?.authFailure ?? false;
		this.isAborted = options?.isAborted ?? false;
		this.retryable = options?.retryable ?? false;
		this.status = options?.status;
	}
}

type PresignedUploadOptions = {
	headers?: Record<string, string>;
	onCreateXhr?: (xhr: XMLHttpRequest) => void;
	requireEtag?: boolean;
};

type ProviderResumableUploadOptions = {
	onCreateXhr?: (xhr: XMLHttpRequest) => void;
	onProgress?: (loaded: number, total: number) => void;
};

function isRetryableHttpStatus(status: number): boolean {
	return status === 408 || status === 429 || status >= 500;
}

function parseApiMessage(responseText: string): string | null {
	if (!responseText) return null;
	try {
		const parsed = JSON.parse(responseText) as { msg?: string };
		return typeof parsed.msg === "string" ? parsed.msg : null;
	} catch {
		return null;
	}
}

function normalizeApiEnvelope<T>(parsed: Partial<ApiResponse<T>>): {
	code?: ApiErrorCode;
	msg?: ApiResponse["msg"];
	retryable?: ApiErrorInfo["retryable"];
} {
	const errorInfo =
		typeof parsed.error === "object" && parsed.error !== null
			? (parsed.error as Partial<ApiErrorInfo>)
			: undefined;
	return {
		code:
			typeof parsed.code === "string" && isApiErrorCode(parsed.code)
				? parsed.code
				: undefined,
		msg: typeof parsed.msg === "string" ? parsed.msg : undefined,
		retryable: errorInfo?.retryable === true,
	};
}

function parseApiErrorResponse(
	responseText: string,
): ReturnType<typeof normalizeApiEnvelope> | null {
	if (!responseText) return null;
	try {
		return normalizeApiEnvelope(
			JSON.parse(responseText) as Partial<ApiResponse>,
		);
	} catch {
		return null;
	}
}

function isTokenUploadErrorCode(code: ApiErrorCode | undefined): boolean {
	return (
		code === ApiErrorCodeValue.TokenExpired ||
		code === ApiErrorCodeValue.TokenInvalid ||
		code === ApiErrorCodeValue.TokenMissing ||
		code === ApiErrorCodeValue.RefreshTokenReuseDetected
	);
}

export function isRetryableUploadError(error: unknown): boolean {
	return (
		typeof error === "object" &&
		error !== null &&
		"retryable" in error &&
		(error as { retryable?: boolean }).retryable === true
	);
}

export function buildUploadPath(workspace: Workspace, path: string) {
	return buildWorkspacePath(workspace, path);
}

export function createUploadService(workspace: Workspace = PERSONAL_WORKSPACE) {
	return {
		initUpload: (data: {
			filename: string;
			total_size: number;
			mime_type?: string;
			folder_id?: number | null;
			relative_path?: string;
		}) =>
			api.post<InitUploadResponse>(
				buildUploadPath(workspace, "/files/upload/init"),
				{
					...data,
					frontend_client_id: getUploadFrontendClientId(),
				},
			),

		uploadChunk: (
			uploadId: string,
			chunkNumber: number,
			data: Blob,
			onProgress?: (loaded: number, total: number) => void,
			onCreateXhr?: (xhr: XMLHttpRequest) => void,
		): Promise<ChunkUploadResponse> => {
			return new Promise((resolve, reject) => {
				const xhr = new XMLHttpRequest();
				onCreateXhr?.(xhr);
				xhr.open(
					"PUT",
					`${config.apiBaseUrl}${buildUploadPath(
						workspace,
						`/files/upload/${uploadId}/${chunkNumber}`,
					)}`,
				);
				xhr.withCredentials = true;
				xhr.setRequestHeader("Content-Type", "application/octet-stream");
				const csrfToken = getCsrfToken();
				if (csrfToken) {
					xhr.setRequestHeader(CSRF_HEADER_NAME, csrfToken);
				}

				if (onProgress) {
					xhr.upload.onprogress = (e) => {
						if (e.lengthComputable) onProgress(e.loaded, e.total);
					};
				}

				xhr.onload = () => {
					if (xhr.status >= 200 && xhr.status < 300) {
						try {
							const parsed = JSON.parse(xhr.responseText) as Partial<
								ApiResponse<ChunkUploadResponse>
							>;
							const resp = normalizeApiEnvelope(parsed);
							if (resp.code === ApiErrorCodeValue.Success) {
								resolve(parsed.data as ChunkUploadResponse);
							} else {
								reject(
									new UploadRequestError(
										resp.msg ?? `chunk upload failed: ${xhr.status}`,
										{
											status: xhr.status,
											retryable: resp.retryable === true,
										},
									),
								);
							}
						} catch (error) {
							reject(
								new UploadRequestError(
									error instanceof Error
										? error.message
										: "failed to parse upload response",
									{
										status: xhr.status,
										retryable: isRetryableHttpStatus(xhr.status),
									},
								),
							);
						}
					} else {
						const apiError = parseApiErrorResponse(xhr.responseText);
						const authFailure = isTokenUploadErrorCode(apiError?.code);
						reject(
							new UploadRequestError(
								apiError?.msg ?? `chunk upload failed: ${xhr.status}`,
								{
									status: xhr.status,
									authFailure,
									retryable:
										authFailure ||
										apiError?.retryable === true ||
										isRetryableHttpStatus(xhr.status),
								},
							),
						);
					}
				};
				xhr.onerror = () =>
					reject(
						new UploadRequestError("network error", {
							retryable: true,
						}),
					);
				xhr.onabort = () =>
					reject(
						new UploadRequestError("upload aborted", {
							isAborted: true,
							retryable: false,
							status: xhr.status,
						}),
					);
				xhr.send(data);
			});
		},

		streamUploadBody: (
			uploadId: string,
			data: Blob,
			onProgress?: (loaded: number, total: number) => void,
			onCreateXhr?: (xhr: XMLHttpRequest) => void,
		): Promise<FileInfo> =>
			new Promise((resolve, reject) => {
				const xhr = new XMLHttpRequest();
				onCreateXhr?.(xhr);
				xhr.open(
					"PUT",
					`${config.apiBaseUrl}${buildUploadPath(
						workspace,
						`/files/upload/${uploadId}/body`,
					)}`,
				);
				xhr.withCredentials = true;
				xhr.setRequestHeader("Content-Type", "application/octet-stream");
				const csrfToken = getCsrfToken();
				if (csrfToken) xhr.setRequestHeader(CSRF_HEADER_NAME, csrfToken);
				if (onProgress) {
					xhr.upload.onprogress = (event) => {
						if (event.lengthComputable) onProgress(event.loaded, event.total);
					};
				}
				xhr.onload = () => {
					const apiError = parseApiErrorResponse(xhr.responseText);
					if (xhr.status >= 200 && xhr.status < 300) {
						try {
							const parsed = JSON.parse(
								xhr.responseText,
							) as ApiResponse<FileInfo>;
							if (parsed.code === ApiErrorCodeValue.Success && parsed.data) {
								resolve(parsed.data);
								return;
							}
						} catch {}
					}
					reject(
						new UploadRequestError(
							apiError?.msg ?? `stream upload failed: ${xhr.status}`,
							{
								status: xhr.status,
								authFailure: isTokenUploadErrorCode(apiError?.code),
								retryable:
									apiError?.retryable === true ||
									isRetryableHttpStatus(xhr.status),
							},
						),
					);
				};
				xhr.onerror = () =>
					reject(new UploadRequestError("network error", { retryable: true }));
				xhr.onabort = () =>
					reject(
						new UploadRequestError("upload aborted", {
							isAborted: true,
							retryable: false,
							status: xhr.status,
						}),
					);
				xhr.send(data);
			}),

		completeUpload: async (
			uploadId: string,
			parts?: CompletedPart[],
		): Promise<FileInfo> => {
			const resp = await api.client.post<ApiResponse<FileInfo>>(
				buildUploadPath(workspace, `/files/upload/${uploadId}/complete`),
				parts ? { parts } : undefined,
				{ timeout: 0 },
			);
			if (resp.data.code !== ApiErrorCodeValue.Success) {
				throw new ApiError(resp.data.code, resp.data.msg, {
					retryable: resp.data.error?.retryable ?? undefined,
				});
			}
			return resp.data.data as FileInfo;
		},

		cancelUpload: (uploadId: string) =>
			api.delete<void>(buildUploadPath(workspace, `/files/upload/${uploadId}`)),

		getProgress: (uploadId: string) =>
			api.get<UploadProgressResponse>(
				buildUploadPath(workspace, `/files/upload/${uploadId}`),
			),

		listRecoverableSessions: () =>
			api.get<RecoverableUploadSession[]>(
				buildUploadPath(workspace, "/files/upload/sessions"),
				{
					params: {
						frontend_client_id: getUploadFrontendClientId(),
					},
				},
			),

		presignParts: (uploadId: string, partNumbers: number[]) =>
			api.post<Record<number, PresignedUploadRequest>>(
				buildUploadPath(workspace, `/files/upload/${uploadId}/presign-parts`),
				{
					part_numbers: partNumbers,
				},
			),

		presignedUpload: (
			presignedUrl: string,
			file: File | Blob,
			onProgress?: (loaded: number, total: number) => void,
			options: PresignedUploadOptions = {},
		): Promise<string> => {
			return new Promise((resolve, reject) => {
				const xhr = new XMLHttpRequest();
				const blockId = new URL(presignedUrl).searchParams.get("blockid");
				const requireEtag = options.requireEtag ?? true;
				const requestHeaders = options.headers ?? {};
				const logFailure = (event: "http-error" | "network-error") => {
					logger.debug("presigned upload failed", {
						event,
						method: "PUT",
						url: presignedUrl,
						requestHeaderNames: Object.keys(requestHeaders).sort(),
						bodySize: file.size,
						status: xhr.status,
						statusText: xhr.statusText,
						responseUrl: xhr.responseURL,
						responseHeaders: xhr.getAllResponseHeaders(),
						responseText: xhr.responseText,
					});
				};
				options.onCreateXhr?.(xhr);
				xhr.open("PUT", presignedUrl);
				for (const [name, value] of Object.entries(requestHeaders)) {
					xhr.setRequestHeader(name, value);
				}

				if (onProgress) {
					xhr.upload.onprogress = (e) => {
						if (e.lengthComputable) onProgress(e.loaded, e.total);
					};
				}

				xhr.onload = () => {
					if (xhr.status >= 200 && xhr.status < 300) {
						if (!requireEtag) {
							resolve(blockId ?? "");
							return;
						}
						const etag = xhr.getResponseHeader("ETag") ?? "";
						if (!etag && blockId) {
							resolve(blockId);
							return;
						}
						if (!etag) {
							reject(
								new UploadRequestError(
									"Presigned upload did not return ETag header. Check CORS ExposeHeaders configuration.",
									{ status: xhr.status, retryable: false },
								),
							);
							return;
						}
						resolve(etag);
					} else {
						logFailure("http-error");
						reject(
							new UploadRequestError(
								parseApiMessage(xhr.responseText) ??
									`Presigned upload failed: ${xhr.status}`,
								{
									status: xhr.status,
									retryable: isRetryableHttpStatus(xhr.status),
								},
							),
						);
					}
				};
				xhr.onerror = () => {
					logFailure("network-error");
					reject(
						new UploadRequestError("network error", {
							retryable: true,
						}),
					);
				};
				xhr.onabort = () =>
					reject(
						new UploadRequestError("upload aborted", {
							isAborted: true,
							status: xhr.status,
							retryable: false,
						}),
					);
				xhr.send(file);
			});
		},

		coordinateBrowserUpload: async (
			uploadId: string,
			report: BrowserUploadReport = {},
		) => {
			try {
				return await api.post<BrowserUploadStep>(
					buildUploadPath(workspace, `/files/upload/${uploadId}/provider-step`),
					report,
				);
			} catch (error) {
				if (
					isAxiosError(error) &&
					(["ERR_NETWORK", "ECONNABORTED", "ETIMEDOUT"].includes(
						error.code ?? "",
					) ||
						(error.response && isRetryableHttpStatus(error.response.status)))
				) {
					throw new UploadRequestError(
						"browser upload coordination temporarily unavailable",
						{ retryable: true, status: error.response?.status },
					);
				}
				throw error;
			}
		},

		executeBrowserUpload: (
			instruction: BrowserUploadInstruction,
			file: Blob,
			options: ProviderResumableUploadOptions = {},
		): Promise<BrowserUploadObservation> =>
			new Promise((resolve, reject) => {
				const request = instruction.request;
				const range = request.body;
				const url = new URL(request.url, location.href);
				const forbiddenHeader = (name: string) =>
					/^(authorization|proxy-authorization|cookie2?|host|origin|referer|content-length|connection|transfer-encoding|accept-encoding|x-csrf-token|set-cookie|sec-.*|proxy-.*|access-control-.*)$/i.test(
						name,
					);
				if (
					!["http:", "https:"].includes(url.protocol) ||
					url.origin === location.origin ||
					url.username ||
					url.password ||
					url.hash ||
					!["GET", "PUT", "POST", "PATCH", "DELETE"].includes(request.method) ||
					!Number.isSafeInteger(file.size) ||
					!Number.isSafeInteger(instruction.response_body_limit) ||
					instruction.response_body_limit <= 0 ||
					instruction.response_body_limit > 65536 ||
					!Number.isSafeInteger(instruction.timeout_ms) ||
					instruction.timeout_ms <= 0 ||
					instruction.timeout_ms > 60000 ||
					Object.keys(request.headers).some(forbiddenHeader) ||
					request.response_headers.some(forbiddenHeader) ||
					(range &&
						(!Number.isSafeInteger(range.start) ||
							!Number.isSafeInteger(range.end) ||
							range.start < 0 ||
							range.end <= range.start ||
							range.end > file.size ||
							request.method === "GET"))
				) {
					reject(new UploadRequestError("invalid browser upload instruction"));
					return;
				}
				const xhr = new XMLHttpRequest();
				xhr.open(request.method, url.href);
				xhr.withCredentials = false;
				xhr.timeout = instruction.timeout_ms;
				for (const [name, value] of Object.entries(request.headers))
					xhr.setRequestHeader(name, value);
				let settled = false;
				const observe = (networkFailure = false, oversized = false) => {
					if (settled) return;
					settled = true;
					const headers: Record<string, string> = {};
					if (!networkFailure)
						for (const name of request.response_headers) {
							const value = xhr.getResponseHeader(name);
							if (value !== null) headers[name.toLowerCase()] = value;
						}
					const bytes = new TextEncoder().encode(
						networkFailure ? "" : xhr.responseText,
					);
					const truncated =
						oversized || bytes.length > instruction.response_body_limit;
					const body = truncated
						? new TextDecoder().decode(
								bytes.slice(
									0,
									Math.max(0, instruction.response_body_limit - 3),
								),
							)
						: networkFailure
							? ""
							: xhr.responseText;
					resolve({
						status: networkFailure ? 0 : xhr.status,
						headers,
						body,
						body_truncated: truncated,
					});
				};
				xhr.onload = () => observe();
				xhr.onerror = () => observe(true);
				xhr.ontimeout = () => observe(true);
				xhr.onprogress = (event) => {
					if (event.loaded > instruction.response_body_limit) {
						observe(true, true);
						xhr.abort();
					}
				};
				xhr.onabort = () => {
					if (settled) return;
					settled = true;
					reject(new UploadRequestError("upload aborted", { isAborted: true }));
				};
				xhr.upload.onprogress = (event) => {
					if (event.lengthComputable)
						options.onProgress?.(event.loaded, event.total);
				};
				options.onCreateXhr?.(xhr);
				// An onCreateXhr cancellation can happen before send().
				if (!settled)
					xhr.send(range ? file.slice(range.start, range.end) : null);
			}),
	};
}

export const uploadService = bindWorkspaceService(createUploadService);
