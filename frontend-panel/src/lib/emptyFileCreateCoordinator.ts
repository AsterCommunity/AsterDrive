import { removePendingEmptyFile } from "@/lib/uploadPersistence";
import type { Workspace } from "@/lib/workspace";
import { createFileService } from "@/services/fileService";

interface CreatePendingEmptyFileOptions {
	taskId: string;
	idempotencyKey: string;
	filename: string;
	baseFolderId: number | null;
	relativePath: string | null;
	workspace: Workspace;
	requests: Map<string, AbortController>;
}

/**
 * Executes the recoverable remote-resource operation for a metadata-only file.
 * UI task state remains component-owned; request cancellation and replay-record
 * cleanup stay atomic from the caller's point of view.
 */
export async function createPendingEmptyFile({
	taskId,
	idempotencyKey,
	filename,
	baseFolderId,
	relativePath,
	workspace,
	requests,
}: CreatePendingEmptyFileOptions): Promise<"completed" | "aborted"> {
	const controller = new AbortController();
	requests.set(taskId, controller);
	try {
		await createFileService(workspace).createEmptyFile(
			filename,
			baseFolderId,
			relativePath ?? undefined,
			{
				signal: controller.signal,
				idempotencyKey,
			},
		);
		if (controller.signal.aborted) return "aborted";
		removePendingEmptyFile(taskId);
		return "completed";
	} catch (error) {
		if (controller.signal.aborted) return "aborted";
		throw error;
	} finally {
		requests.delete(taskId);
	}
}
