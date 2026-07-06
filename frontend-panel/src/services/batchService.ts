import { config } from "@/config/app";
import { joinApiUrl } from "@/lib/apiUrl";
import {
	buildWorkspacePath,
	PERSONAL_WORKSPACE,
	type Workspace,
} from "@/lib/workspace";
import { api } from "@/services/http";
import { bindWorkspaceService } from "@/stores/workspaceStore";
import type {
	BatchResult,
	TaskInfo,
	WorkspaceRef,
	WorkspaceTransferCopyRequest,
} from "@/types/api";

export interface StreamTicketInfo {
	token: string;
	download_path: string;
	expires_at: string;
}

export function triggerStreamingDownload(url: string) {
	const iframe = document.createElement("iframe");
	iframe.style.display = "none";
	document.body.appendChild(iframe);
	iframe.src = url;

	window.setTimeout(() => {
		iframe.remove();
	}, 60_000);
}

export function buildArchiveDownloadPayload(
	fileIds: number[],
	folderIds: number[],
	archiveName?: string,
) {
	return {
		file_ids: fileIds,
		folder_ids: folderIds,
		...(archiveName === undefined ? {} : { archive_name: archiveName }),
	};
}

export function workspaceToTransferRef(workspace: Workspace): WorkspaceRef {
	return workspace.kind === "team"
		? { kind: "team", team_id: workspace.teamId }
		: { kind: "personal" };
}

function buildArchiveDownloadUrl(
	workspace: Workspace,
	ticket: StreamTicketInfo,
) {
	if (/^https?:\/\//.test(ticket.download_path)) {
		return ticket.download_path;
	}

	return joinApiUrl(
		config.apiBaseUrl,
		buildWorkspacePath(workspace, `/batch/archive-download/${ticket.token}`),
	);
}

export function createBatchService(workspace: Workspace = PERSONAL_WORKSPACE) {
	return {
		batchDelete: (fileIds: number[], folderIds: number[]) =>
			api.post<BatchResult>(buildWorkspacePath(workspace, "/batch/delete"), {
				file_ids: fileIds,
				folder_ids: folderIds,
			}),

		batchMove: (
			fileIds: number[],
			folderIds: number[],
			targetFolderId: number | null,
		) =>
			api.post<BatchResult>(buildWorkspacePath(workspace, "/batch/move"), {
				file_ids: fileIds,
				folder_ids: folderIds,
				target_folder_id: targetFolderId,
			}),

		batchCopy: (
			fileIds: number[],
			folderIds: number[],
			targetFolderId: number | null,
		) =>
			api.post<BatchResult>(buildWorkspacePath(workspace, "/batch/copy"), {
				file_ids: fileIds,
				folder_ids: folderIds,
				target_folder_id: targetFolderId,
			}),

		copyToWorkspace: (
			destinationWorkspace: Workspace,
			fileIds: number[],
			folderIds: number[],
			targetFolderId: number | null,
		) => {
			const payload: WorkspaceTransferCopyRequest = {
				source_workspace: workspaceToTransferRef(workspace),
				file_ids: fileIds,
				folder_ids: folderIds,
				destination_workspace: workspaceToTransferRef(destinationWorkspace),
				target_folder_id: targetFolderId,
			};
			return api.post<BatchResult>("/workspace-transfer/copy", payload);
		},

		streamArchiveDownload: (
			fileIds: number[],
			folderIds: number[],
			archiveName?: string,
		) =>
			api
				.post<StreamTicketInfo>(
					buildWorkspacePath(workspace, "/batch/archive-download"),
					buildArchiveDownloadPayload(fileIds, folderIds, archiveName),
				)
				.then((ticket) => {
					triggerStreamingDownload(buildArchiveDownloadUrl(workspace, ticket));
				}),

		createArchiveCompressTask: (
			fileIds: number[],
			folderIds: number[],
			archiveName?: string,
			targetFolderId?: number | null,
		) =>
			api.post<TaskInfo>(
				buildWorkspacePath(workspace, "/batch/archive-compress"),
				{
					file_ids: fileIds,
					folder_ids: folderIds,
					...(archiveName === undefined ? {} : { archive_name: archiveName }),
					...(targetFolderId === undefined
						? {}
						: { target_folder_id: targetFolderId }),
				},
			),
	};
}

export const batchService = bindWorkspaceService(createBatchService);
