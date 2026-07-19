import { config } from "@/config/app";
import { getUploadFrontendClientId } from "@/lib/uploadClientId";
import {
	buildWorkspacePath,
	PERSONAL_WORKSPACE,
	type Workspace,
} from "@/lib/workspace";
import { bindWorkspaceService } from "@/stores/workspaceStore";
import type {
	ApiErrorCode,
	ApiResponse,
	ChunkUploadResponse,
	CompletedPart,
	FileInfo,
	InitUploadResponse,
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

function parseApiErrorResponse(responseText: string): {
	code?: ApiErrorCode;
	msg?: string;
} | null {
	if (!responseText) return null;
	try {
		const parsed = JSON.parse(responseText) as {
			code?: unknown;
			msg?: unknown;
		};
		return {
			code:
				typeof parsed.code === "string" && isApiErrorCode(parsed.code)
					? parsed.code
					: undefined,
			msg: typeof parsed.msg === "string" ? parsed.msg : undefined,
		};
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
							const resp = JSON.parse(xhr.responseText) as {
								code?: ApiErrorCode;
								msg?: string;
								data?: ChunkUploadResponse;
							};
							if (resp.code === ApiErrorCodeValue.Success) {
								resolve(resp.data as ChunkUploadResponse);
							} else {
								reject(
									new UploadRequestError(
										resp.msg ?? `chunk upload failed: ${xhr.status}`,
										{
											status: xhr.status,
											retryable: false,
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
									retryable: authFailure || isRetryableHttpStatus(xhr.status),
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
			api.post<Record<number, string>>(
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
				options.onCreateXhr?.(xhr);
				xhr.open("PUT", presignedUrl);
				xhr.setRequestHeader("Content-Type", "application/octet-stream");
				for (const [name, value] of Object.entries(options.headers ?? {})) {
					xhr.setRequestHeader(name, value);
				}

				if (onProgress) {
					xhr.upload.onprogress = (e) => {
						if (e.lengthComputable) onProgress(e.loaded, e.total);
					};
				}

				xhr.onload = () => {
					if (xhr.status >= 200 && xhr.status < 300) {
						const etag = xhr.getResponseHeader("ETag") ?? "";
						if (!etag && blockId) {
							resolve(blockId);
							return;
						}
						if (!etag) {
							if (!requireEtag) {
								resolve("");
								return;
							}
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
							status: xhr.status,
							retryable: false,
						}),
					);
				xhr.send(file);
			});
		},

		providerResumableUpload: (
			uploadUrl: string,
			data: Blob,
			start: number,
			totalSize: number,
			options: ProviderResumableUploadOptions = {},
		): Promise<void> => {
			return new Promise((resolve, reject) => {
				if (data.size === 0) {
					reject(
						new UploadRequestError("provider upload chunk cannot be empty", {
							retryable: false,
						}),
					);
					return;
				}
				const xhr = new XMLHttpRequest();
				options.onCreateXhr?.(xhr);
				xhr.open("PUT", uploadUrl);
				xhr.withCredentials = false;
				xhr.setRequestHeader("Content-Type", "application/octet-stream");
				const end = start + data.size - 1;
				xhr.setRequestHeader(
					"Content-Range",
					`bytes ${start}-${end}/${totalSize}`,
				);

				if (options.onProgress) {
					xhr.upload.onprogress = (event) => {
						if (event.lengthComputable) {
							options.onProgress?.(event.loaded, event.total);
						}
					};
				}

				xhr.onload = () => {
					if (xhr.status >= 200 && xhr.status < 300) {
						resolve();
						return;
					}
					reject(
						new UploadRequestError(
							parseApiMessage(xhr.responseText) ??
								`Provider upload failed: ${xhr.status}`,
							{
								status: xhr.status,
								retryable:
									xhr.status === 416 || isRetryableHttpStatus(xhr.status),
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
			});
		},
	};
}

export const uploadService = bindWorkspaceService(createUploadService);
