import { config } from "@/config/app";
import { joinApiUrl } from "@/lib/apiUrl";
import {
	buildWorkspacePath,
	PERSONAL_WORKSPACE,
	type Workspace,
	workspaceEquals,
} from "@/lib/workspace";
import { api } from "@/services/http";
import { bindWorkspaceService } from "@/stores/workspaceStore";
import type {
	BatchResult,
	TaskInfo,
	WorkspaceRef,
	WorkspaceTransferCopyRequest,
	WorkspaceTransferMoveRequest,
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

interface WorkspaceCopyDispatcher {
	batchCopy: (
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number | null,
	) => Promise<BatchResult>;
	singleFileCopy?: (
		fileId: number,
		targetFolderId: number | null,
	) => Promise<BatchResult>;
	singleFolderCopy?: (
		folderId: number,
		targetFolderId: number | null,
	) => Promise<BatchResult>;
	copyToWorkspace: (
		destinationWorkspace: Workspace,
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number | null,
	) => Promise<BatchResult>;
}

interface WorkspaceMoveDispatcher {
	batchMove: (
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number | null,
	) => Promise<BatchResult>;
	singleFileMove?: (
		fileId: number,
		targetFolderId: number | null,
	) => Promise<BatchResult>;
	singleFolderMove?: (
		folderId: number,
		targetFolderId: number | null,
	) => Promise<BatchResult>;
	moveToWorkspace: (
		destinationWorkspace: Workspace,
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number | null,
	) => Promise<BatchResult>;
}

interface ResolveCopyDispatchInput {
	currentWorkspace: Workspace;
	targetWorkspace: Workspace;
	fileIds: number[];
	folderIds: number[];
	targetFolderId: number | null;
	dispatcher?: WorkspaceCopyDispatcher;
}

export function resolveCopyDispatch({
	currentWorkspace,
	targetWorkspace,
	fileIds,
	folderIds,
	targetFolderId,
	dispatcher = batchService,
}: ResolveCopyDispatchInput) {
	if (!workspaceEquals(currentWorkspace, targetWorkspace)) {
		return dispatcher.copyToWorkspace(
			targetWorkspace,
			fileIds,
			folderIds,
			targetFolderId,
		);
	}

	if (fileIds.length + folderIds.length === 1) {
		if (fileIds.length === 1 && dispatcher.singleFileCopy) {
			return dispatcher.singleFileCopy(fileIds[0], targetFolderId);
		}
		if (folderIds.length === 1 && dispatcher.singleFolderCopy) {
			return dispatcher.singleFolderCopy(folderIds[0], targetFolderId);
		}
		throw new Error("single-item copy dispatcher is required");
	}

	return dispatcher.batchCopy(fileIds, folderIds, targetFolderId);
}

export function singleOperationResult(
	operationPromise: Promise<unknown>,
): Promise<BatchResult> {
	return operationPromise.then(() => ({ succeeded: 1, failed: 0, errors: [] }));
}

export function resolveMoveDispatch({
	currentWorkspace,
	targetWorkspace,
	fileIds,
	folderIds,
	targetFolderId,
	dispatcher = batchService,
}: {
	currentWorkspace: Workspace;
	targetWorkspace: Workspace;
	fileIds: number[];
	folderIds: number[];
	targetFolderId: number | null;
	dispatcher?: WorkspaceMoveDispatcher;
}) {
	if (!workspaceEquals(currentWorkspace, targetWorkspace)) {
		return dispatcher.moveToWorkspace(
			targetWorkspace,
			fileIds,
			folderIds,
			targetFolderId,
		);
	}

	if (fileIds.length + folderIds.length === 1) {
		if (fileIds.length === 1 && dispatcher.singleFileMove) {
			return dispatcher.singleFileMove(fileIds[0], targetFolderId);
		}
		if (folderIds.length === 1 && dispatcher.singleFolderMove) {
			return dispatcher.singleFolderMove(folderIds[0], targetFolderId);
		}
		throw new Error("single-item move dispatcher is required");
	}

	return dispatcher.batchMove(fileIds, folderIds, targetFolderId);
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

		moveToWorkspace: (
			destinationWorkspace: Workspace,
			fileIds: number[],
			folderIds: number[],
			targetFolderId: number | null,
		) => {
			const payload: WorkspaceTransferMoveRequest = {
				source_workspace: workspaceToTransferRef(workspace),
				file_ids: fileIds,
				folder_ids: folderIds,
				destination_workspace: workspaceToTransferRef(destinationWorkspace),
				target_folder_id: targetFolderId,
			};
			return api.post<BatchResult>("/workspace-transfer/move", payload);
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
