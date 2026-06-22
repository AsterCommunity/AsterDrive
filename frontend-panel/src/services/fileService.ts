import { config } from "@/config/app";
import { joinApiUrl } from "@/lib/apiUrl";
import { absoluteAppUrl } from "@/lib/publicSiteUrl";
import type {
	FileResourceDeliveryMode,
	ReadyFileResourceHandle,
} from "@/lib/resourceRequest";
import { buildWorkspacePath, type Workspace } from "@/lib/workspace";
import { bindWorkspaceService } from "@/stores/workspaceStore";
import type {
	ApiErrorCode,
	ArchiveFilenameEncoding,
	ArchivePreviewManifest,
	CreateOfflineDownloadTaskParams,
	DirectLinkTokenInfo,
	FileInfo,
	FileResourceHandleRequest,
	FileVersion,
	FolderAncestorItem,
	FolderContents,
	FolderInfo,
	FolderListParams,
	MediaMetadataInfo,
	PreviewLinkInfo,
	TaskInfo,
	WopiLaunchSession,
} from "@/types/api";
import { ApiErrorCode as ApiErrorCodeValue } from "@/types/api-helpers";
import {
	type ArchivePreviewRequestOptions,
	archivePreviewRequestConfig,
} from "./archivePreviewRequestConfig";
import { ApiError, type ApiRequestConfig, api } from "./http";

type ServiceRequestOptions = Pick<ApiRequestConfig, "signal">;

interface ApiFileResourceHandle {
	identity: {
		cache_key: string;
		etag?: string | null;
		scope?: "personal" | "team" | "share" | null;
	};
	request: {
		url: string;
		credentials: "include" | "omit";
		conditional_headers: "allowed" | "forbidden";
		redirect_policy: "same_origin_only" | "may_cross_origin";
	};
	delivery: {
		mode: FileResourceDeliveryMode;
		mime_type?: string | null;
	};
}

function toReadyFileResourceHandle(
	handle: ApiFileResourceHandle,
): ReadyFileResourceHandle {
	return {
		kind: "ready",
		identity: {
			cacheKey: handle.identity.cache_key,
			etag: handle.identity.etag ?? null,
			scope: handle.identity.scope ?? undefined,
		},
		request: {
			url: handle.request.url,
			credentials: handle.request.credentials,
			conditionalHeaders: handle.request.conditional_headers,
			redirectPolicy: handle.request.redirect_policy,
		},
		delivery: {
			mode: handle.delivery.mode,
			mimeType: handle.delivery.mime_type ?? undefined,
		},
	};
}

function encodeFileName(fileName: string) {
	return encodeURIComponent(fileName);
}

export function createFileService(workspace: Workspace) {
	return {
		listRoot: (params?: FolderListParams, options?: ServiceRequestOptions) =>
			api.get<FolderContents>(buildWorkspacePath(workspace, "/folders"), {
				...(options ?? {}),
				params,
			}),

		listFolder: (
			id: number,
			params?: FolderListParams,
			options?: ServiceRequestOptions,
		) =>
			api.get<FolderContents>(buildWorkspacePath(workspace, `/folders/${id}`), {
				...(options ?? {}),
				params,
			}),

		getFolderAncestors: (id: number, options?: ServiceRequestOptions) =>
			api.get<FolderAncestorItem[]>(
				buildWorkspacePath(workspace, `/folders/${id}/ancestors`),
				options,
			),

		getFolderInfo: (id: number) =>
			api.get<FolderInfo>(buildWorkspacePath(workspace, `/folders/${id}/info`)),

		createFolder: (name: string, parentId?: number | null) =>
			api.post<FolderInfo>(buildWorkspacePath(workspace, "/folders"), {
				name,
				parent_id: parentId ?? null,
			}),

		deleteFolder: (id: number) =>
			api.delete<void>(buildWorkspacePath(workspace, `/folders/${id}`)),

		renameFolder: (id: number, name: string) =>
			api.patch<FolderInfo>(buildWorkspacePath(workspace, `/folders/${id}`), {
				name,
			}),

		getFile: (id: number) =>
			api.get<FileInfo>(buildWorkspacePath(workspace, `/files/${id}`)),

		getDirectLinkToken: (id: number) =>
			api.get<DirectLinkTokenInfo>(
				buildWorkspacePath(workspace, `/files/${id}/direct-link`),
			),

		getArchivePreview: (id: number, options?: ArchivePreviewRequestOptions) =>
			api.get<ArchivePreviewManifest>(
				buildWorkspacePath(workspace, `/files/${id}/archive-preview`),
				archivePreviewRequestConfig(options),
			),

		getMediaMetadata: (id: number, options?: ServiceRequestOptions) =>
			api.get<MediaMetadataInfo>(
				buildWorkspacePath(workspace, `/files/${id}/media-metadata`),
				options,
			),

		createPreviewLink: (id: number) =>
			api.post<PreviewLinkInfo>(
				buildWorkspacePath(workspace, `/files/${id}/preview-link`),
			),

		resolveResourceHandle: (id: number, data: FileResourceHandleRequest) =>
			api
				.post<ApiFileResourceHandle>(
					buildWorkspacePath(workspace, `/files/${id}/resource-handle`),
					data,
				)
				.then(toReadyFileResourceHandle),

		createWopiSession: (id: number, appKey: string) =>
			api.post<WopiLaunchSession>(
				buildWorkspacePath(workspace, `/files/${id}/wopi/open`),
				{
					app_key: appKey,
				},
			),

		deleteFile: (id: number) =>
			api.delete<void>(buildWorkspacePath(workspace, `/files/${id}`)),

		renameFile: (id: number, name: string) =>
			api.patch<FileInfo>(buildWorkspacePath(workspace, `/files/${id}`), {
				name,
			}),

		downloadPath: (id: number) =>
			buildWorkspacePath(workspace, `/files/${id}/download`),

		downloadUrl: (id: number) =>
			joinApiUrl(
				config.apiBaseUrl,
				buildWorkspacePath(workspace, `/files/${id}/download`),
			),

		directPath: (token: string, fileName: string) =>
			`/d/${token}/${encodeFileName(fileName)}`,

		directUrl: (token: string, fileName: string) =>
			absoluteAppUrl(`/d/${token}/${encodeFileName(fileName)}`),

		forceDownloadPath: (token: string, fileName: string) =>
			`/d/${token}/${encodeFileName(fileName)}?download=1`,

		forceDownloadUrl: (token: string, fileName: string) =>
			absoluteAppUrl(`/d/${token}/${encodeFileName(fileName)}?download=1`),

		thumbnailPath: (id: number) =>
			buildWorkspacePath(workspace, `/files/${id}/thumbnail`),

		imagePreviewPath: (id: number) =>
			buildWorkspacePath(workspace, `/files/${id}/image-preview`),

		setFileLock: (id: number, locked: boolean) =>
			api.post<FileInfo>(buildWorkspacePath(workspace, `/files/${id}/lock`), {
				locked,
			}),

		setFolderLock: (id: number, locked: boolean) =>
			api.post<FolderInfo>(
				buildWorkspacePath(workspace, `/folders/${id}/lock`),
				{
					locked,
				},
			),

		createEmptyFile: (name: string, folderId?: number | null) =>
			api.post<FileInfo>(buildWorkspacePath(workspace, "/files/new"), {
				name,
				folder_id: folderId ?? null,
			}),

		copyFile: (id: number, folderId?: number | null) =>
			api.post<FileInfo>(buildWorkspacePath(workspace, `/files/${id}/copy`), {
				folder_id: folderId ?? null,
			}),

		createArchiveExtractTask: (
			id: number,
			targetFolderId?: number | null,
			outputFolderName?: string,
			filenameEncoding?: ArchiveFilenameEncoding,
		) =>
			api.post<TaskInfo>(
				buildWorkspacePath(workspace, `/files/${id}/extract`),
				{
					...(targetFolderId === undefined
						? {}
						: { target_folder_id: targetFolderId }),
					...(outputFolderName === undefined
						? {}
						: { output_folder_name: outputFolderName }),
					...(filenameEncoding === undefined
						? {}
						: { filename_encoding: filenameEncoding }),
				},
			),

		createOfflineDownloadTask: (params: CreateOfflineDownloadTaskParams) =>
			api.post<TaskInfo>(
				buildWorkspacePath(workspace, "/tasks/offline-download"),
				params,
			),

		copyFolder: (id: number, parentId?: number | null) =>
			api.post<FolderInfo>(
				buildWorkspacePath(workspace, `/folders/${id}/copy`),
				{
					parent_id: parentId ?? null,
				},
			),

		updateContent: async (id: number, content: string, etag?: string) => {
			const headers: Record<string, string> = {
				"Content-Type": "application/octet-stream",
			};
			if (etag) headers["If-Match"] = etag;
			try {
				const resp = await api.client.put(
					buildWorkspacePath(workspace, `/files/${id}/content`),
					content,
					{
						headers,
					},
				);
				return resp.data.data as FileInfo;
			} catch (err: unknown) {
				if (err && typeof err === "object") {
					const response = (
						err as {
							response?: {
								status: number;
								data?: {
									code?: ApiErrorCode;
									msg?: string;
									error?: {
										retryable?: boolean;
									} | null;
								};
							} | null;
						}
					).response;
					if (response != null) {
						const status = response.status;
						const body = response.data;
						const apiErr = new ApiError(
							body?.code ?? ApiErrorCodeValue.InternalServerError,
							body?.msg ?? `HTTP ${status}`,
							{
								retryable: body?.error?.retryable ?? undefined,
								status,
							},
						);
						throw apiErr;
					}
				}
				throw err;
			}
		},

		listVersions: (id: number) =>
			api.get<FileVersion[]>(
				buildWorkspacePath(workspace, `/files/${id}/versions`),
			),

		restoreVersion: (fileId: number, versionId: number) =>
			api.post<FileInfo>(
				buildWorkspacePath(
					workspace,
					`/files/${fileId}/versions/${versionId}/restore`,
				),
			),

		deleteVersion: (fileId: number, versionId: number) =>
			api.delete<void>(
				buildWorkspacePath(workspace, `/files/${fileId}/versions/${versionId}`),
			),
	};
}

// `fileService` methods resolve the current workspace when invoked, so cached
// or destructured method references still follow workspace changes. Use
// `createFileService(workspace)` for an explicit stable workspace instance.
export const fileService = bindWorkspaceService(createFileService);
