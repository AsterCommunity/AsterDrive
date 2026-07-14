import { config } from "@/config/app";
import { joinApiUrl } from "@/lib/apiUrl";
import { absoluteAppUrl } from "@/lib/publicSiteUrl";
import { buildWorkspacePath, type Workspace } from "@/lib/workspace";
import { bindWorkspaceService } from "@/stores/workspaceStore";
import type {
	ArchivePreviewManifest,
	BatchDeleteSharesRequest,
	BatchResult,
	CreateShareRequest,
	FolderAncestorItem,
	FolderContents,
	FolderListParams,
	MediaMetadataInfo,
	PreviewLinkInfo,
	ShareInfo,
	ShareListQuery,
	SharePage,
	SharePublicInfo,
	ShareStreamSessionInfo,
	UpdateShareRequest,
	VerifySharePasswordRequest,
} from "@/types/api";
import {
	type ArchivePreviewRequestOptions,
	archivePreviewRequestConfig,
} from "./archivePreviewRequestConfig";
import {
	buildArchiveDownloadPayload,
	type StreamTicketInfo,
	triggerStreamingDownload,
} from "./batchService";
import { type ApiRequestConfig, api } from "./http";

type ServiceRequestOptions = Pick<ApiRequestConfig, "signal">;

function workspaceSharesPrefix(workspace: Workspace) {
	return buildWorkspacePath(workspace, "/shares");
}

function buildSharedArchiveDownloadUrl(ticket: StreamTicketInfo) {
	if (/^https?:\/\//.test(ticket.download_path)) {
		return ticket.download_path;
	}

	return joinApiUrl(config.apiBaseUrl, ticket.download_path);
}

export function createShareService(workspace: Workspace) {
	if (workspace == null) {
		throw new Error("workspace is required");
	}

	return {
		create: (data: CreateShareRequest) =>
			api.post<ShareInfo>(workspaceSharesPrefix(workspace), data),

		listMine: (params?: ShareListQuery) =>
			api.get<SharePage>(workspaceSharesPrefix(workspace), { params }),

		update: (id: number, data: UpdateShareRequest) =>
			api.patch<ShareInfo>(`${workspaceSharesPrefix(workspace)}/${id}`, data),

		delete: (id: number) =>
			api.delete<void>(`${workspaceSharesPrefix(workspace)}/${id}`),

		batchDelete: (data: BatchDeleteSharesRequest) =>
			api.post<BatchResult>(
				`${workspaceSharesPrefix(workspace)}/batch-delete`,
				data,
			),

		getInfo: (token: string) => api.get<SharePublicInfo>(`/s/${token}`),

		verifyPassword: (token: string, data: VerifySharePasswordRequest) =>
			api.post<void>(`/s/${token}/verify`, data),

		pagePath: (token: string) => `/s/${token}`,

		folderPagePath: (token: string, folderId: number) =>
			`/s/${token}/folder/${folderId}`,

		pageUrl: (token: string) => absoluteAppUrl(`/s/${token}`),

		downloadPath: (token: string) => `/s/${token}/download`,

		createPreviewLink: (token: string) =>
			api.post<PreviewLinkInfo>(`/s/${token}/preview-link`),

		getArchivePreview: (
			token: string,
			options?: ArchivePreviewRequestOptions,
		) =>
			api.get<ArchivePreviewManifest>(
				`/s/${token}/archive-preview`,
				archivePreviewRequestConfig(options),
			),

		getMediaMetadata: (token: string, options?: ServiceRequestOptions) =>
			api.get<MediaMetadataInfo>(`/s/${token}/media-metadata`, options),

		createStreamSession: (token: string) =>
			api.post<ShareStreamSessionInfo>(`/s/${token}/stream-session`),

		streamArchiveDownload: (
			token: string,
			fileIds: number[],
			folderIds: number[],
			archiveName?: string,
		) =>
			api
				.post<StreamTicketInfo>(
					`/s/${token}/archive-download`,
					buildArchiveDownloadPayload(fileIds, folderIds, archiveName),
				)
				.then((ticket) => {
					triggerStreamingDownload(buildSharedArchiveDownloadUrl(ticket));
				}),

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
			options?: ArchivePreviewRequestOptions,
		) =>
			api.get<ArchivePreviewManifest>(
				`/s/${token}/files/${fileId}/archive-preview`,
				archivePreviewRequestConfig(options),
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

		getSubfolderAncestors: (token: string, folderId: number) =>
			api.get<FolderAncestorItem[]>(
				`/s/${token}/folders/${folderId}/ancestors`,
			),
	};
}

export const shareService = bindWorkspaceService(createShareService);
