import { config } from "@/config/app";
import {
	buildWorkspacePath,
	PERSONAL_WORKSPACE,
	type Workspace,
} from "@/lib/workspace";
import { bindWorkspaceService } from "@/stores/workspaceStore";
import type {
	ChunkUploadResponse,
	CompletedPart,
	FileInfo,
	InitUploadResponse,
	RecoverableUploadSession,
	UploadProgressResponse,
} from "@/types/api";
import { type ApiResponse, ErrorCode, isApiSubcode } from "@/types/api-helpers";
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
	isAborted: boolean;
	retryable: boolean;
	status?: number;

	constructor(
		message: string,
		options?: {
			isAborted?: boolean;
			retryable?: boolean;
			status?: number;
		},
	) {
		super(message);
		this.name = "UploadRequestError";
		this.isAborted = options?.isAborted ?? false;
		this.retryable = options?.retryable ?? false;
		this.status = options?.status;
	}
}

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
				data,
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
								code?: number;
								msg?: string;
								data?: ChunkUploadResponse;
							};
							if (resp.code === 0) {
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
						reject(
							new UploadRequestError(
								parseApiMessage(xhr.responseText) ??
									`chunk upload failed: ${xhr.status}`,
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
			if (resp.data.code !== ErrorCode.Success) {
				throw new ApiError(resp.data.code, resp.data.msg, {
					internalCode: resp.data.error?.internal_code ?? undefined,
					subcode:
						resp.data.error?.subcode && isApiSubcode(resp.data.error.subcode)
							? resp.data.error.subcode
							: undefined,
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
			onCreateXhr?: (xhr: XMLHttpRequest) => void,
		): Promise<string> => {
			return new Promise((resolve, reject) => {
				const xhr = new XMLHttpRequest();
				onCreateXhr?.(xhr);
				xhr.open("PUT", presignedUrl);
				xhr.setRequestHeader("Content-Type", "application/octet-stream");

				if (onProgress) {
					xhr.upload.onprogress = (e) => {
						if (e.lengthComputable) onProgress(e.loaded, e.total);
					};
				}

				xhr.onload = () => {
					if (xhr.status >= 200 && xhr.status < 300) {
						const etag = xhr.getResponseHeader("ETag") ?? "";
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
	};
}

export const uploadService = bindWorkspaceService(createUploadService);
