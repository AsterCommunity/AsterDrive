import { config } from "@/config/app";
import { joinApiUrl } from "@/lib/apiUrl";
import { absoluteAppUrl } from "@/lib/publicSiteUrl";
import { buildWorkspacePath, type Workspace } from "@/lib/workspace";
import { bindWorkspaceService } from "@/stores/workspaceStore";
import type {
	ArchivePreviewManifest,
	BatchResult,
	FolderContents,
	FolderListParams,
	MediaMetadataInfo,
	PreviewLinkInfo,
	ShareInfo,
	ShareListQuery,
	SharePage,
	SharePublicInfo,
	ShareStreamSessionInfo,
	ShareTarget,
} from "@/types/api";
import { type ApiRequestConfig, api } from "./http";

type ServiceRequestOptions = Pick<ApiRequestConfig, "signal">;

function workspaceSharesPrefix(workspace: Workspace) {
	return buildWorkspacePath(workspace, "/shares");
}

export function createShareService(workspace: Workspace) {
	if (workspace == null) {
		throw new Error("workspace is required");
	}

	return {
		create: (data: {
			target: ShareTarget;
			password?: string;
			expires_at?: string;
			max_downloads?: number;
		}) => api.post<ShareInfo>(workspaceSharesPrefix(workspace), data),

		listMine: (params?: ShareListQuery) =>
			api.get<SharePage>(workspaceSharesPrefix(workspace), { params }),

		update: (
			id: number,
			data: {
				password?: string;
				expires_at: string | null;
				max_downloads: number;
			},
		) =>
			api.patch<ShareInfo>(`${workspaceSharesPrefix(workspace)}/${id}`, data),

		delete: (id: number) =>
			api.delete<void>(`${workspaceSharesPrefix(workspace)}/${id}`),

		batchDelete: (shareIds: number[]) =>
			api.post<BatchResult>(
				`${workspaceSharesPrefix(workspace)}/batch-delete`,
				{
					share_ids: shareIds,
				},
			),

		getInfo: (token: string) => api.get<SharePublicInfo>(`/s/${token}`),

		verifyPassword: (token: string, password: string) =>
			api.post<void>(`/s/${token}/verify`, { password }),

		pagePath: (token: string) => `/s/${token}`,

		pageUrl: (token: string) => absoluteAppUrl(`/s/${token}`),

		downloadPath: (token: string) => `/s/${token}/download`,

		createPreviewLink: (token: string) =>
			api.post<PreviewLinkInfo>(`/s/${token}/preview-link`),

		getArchivePreview: (token: string, options?: ServiceRequestOptions) =>
			api.get<ArchivePreviewManifest>(`/s/${token}/archive-preview`, options),

		getMediaMetadata: (token: string, options?: ServiceRequestOptions) =>
			api.get<MediaMetadataInfo>(`/s/${token}/media-metadata`, options),

		createStreamSession: (token: string) =>
			api.post<ShareStreamSessionInfo>(`/s/${token}/stream-session`),

		thumbnailPath: (token: string) => `/s/${token}/thumbnail`,

		imagePreviewPath: (token: string) => `/s/${token}/image-preview`,

		downloadFolderPath: (token: string, fileId: number) =>
			`/s/${token}/files/${fileId}/download`,

		folderFileThumbnailPath: (token: string, fileId: number) =>
			`/s/${token}/files/${fileId}/thumbnail`,

		folderFileImagePreviewPath: (token: string, fileId: number) =>
			`/s/${token}/files/${fileId}/image-preview`,

		createFolderFilePreviewLink: (token: string, fileId: number) =>
			api.post<PreviewLinkInfo>(`/s/${token}/files/${fileId}/preview-link`),

		getFolderFileArchivePreview: (
			token: string,
			fileId: number,
			options?: ServiceRequestOptions,
		) =>
			api.get<ArchivePreviewManifest>(
				`/s/${token}/files/${fileId}/archive-preview`,
				options,
			),

		getFolderFileMediaMetadata: (
			token: string,
			fileId: number,
			options?: ServiceRequestOptions,
		) =>
			api.get<MediaMetadataInfo>(
				`/s/${token}/files/${fileId}/media-metadata`,
				options,
			),

		createFolderFileStreamSession: (token: string, fileId: number) =>
			api.post<ShareStreamSessionInfo>(
				`/s/${token}/files/${fileId}/stream-session`,
			),

		downloadUrl: (token: string) =>
			joinApiUrl(config.apiBaseUrl, `/s/${token}/download`),

		downloadFolderFileUrl: (token: string, fileId: number) =>
			joinApiUrl(config.apiBaseUrl, `/s/${token}/files/${fileId}/download`),

		listContent: (token: string, params?: FolderListParams) =>
			api.get<FolderContents>(`/s/${token}/content`, { params }),

		listSubfolderContent: (
			token: string,
			folderId: number,
			params?: FolderListParams,
		) =>
			api.get<FolderContents>(`/s/${token}/folders/${folderId}/content`, {
				params,
			}),
	};
}

export const shareService = bindWorkspaceService(createShareService);
