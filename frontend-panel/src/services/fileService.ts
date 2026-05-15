import { config } from "@/config/app";
import { joinApiUrl } from "@/lib/apiUrl";
import { absoluteAppUrl } from "@/lib/publicSiteUrl";
import { buildWorkspacePath, type Workspace } from "@/lib/workspace";
import { bindWorkspaceService } from "@/stores/workspaceStore";
import type {
	DirectLinkTokenInfo,
	ErrorCode,
	FileInfo,
	FileVersion,
	FolderAncestorItem,
	FolderContents,
	FolderInfo,
	FolderListParams,
	PreviewLinkInfo,
	TaskInfo,
	WopiLaunchSession,
} from "@/types/api";
import { ApiError, type ApiRequestConfig, api } from "./http";

type ServiceRequestOptions = Pick<ApiRequestConfig, "signal">;

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

		createPreviewLink: (id: number) =>
			api.post<PreviewLinkInfo>(
				buildWorkspacePath(workspace, `/files/${id}/preview-link`),
			),

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
				},
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
									code?: number;
									msg?: string;
									error?: {
										internal_code?: string;
										subcode?: string;
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
							(body?.code ?? status) as ErrorCode,
							body?.msg ?? `HTTP ${status}`,
							{
								internalCode: body?.error?.internal_code ?? undefined,
								subcode: body?.error?.subcode ?? undefined,
								retryable: body?.error?.retryable ?? undefined,
							},
						);
						(apiErr as ApiError & { status: number }).status = status;
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
