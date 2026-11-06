import {
	assertCurrentAuthOperation,
	type UploadOperationBinding,
} from "@/lib/authOperationOwner";
import { removePendingEmptyFile } from "@/lib/uploadPersistence";
import { createFileService } from "@/services/fileService";

interface CreatePendingEmptyFileOptions {
	taskId: string;
	idempotencyKey: string;
	filename: string;
	baseFolderId: number | null;
	relativePath: string | null;
	binding: UploadOperationBinding;
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
	binding,
	requests,
}: CreatePendingEmptyFileOptions): Promise<"completed" | "aborted"> {
	assertCurrentAuthOperation(binding);
	const controller = new AbortController();
	requests.set(taskId, controller);
	try {
		await createFileService(binding.workspace).createEmptyFile(
			filename,
			baseFolderId,
			relativePath ?? undefined,
			{
				signal: AbortSignal.any([controller.signal, binding.signal]),
				idempotencyKey,
			},
		);
		if (controller.signal.aborted || binding.signal.aborted) return "aborted";
		assertCurrentAuthOperation(binding);
		removePendingEmptyFile(taskId);
		return "completed";
	} catch (error) {
		if (controller.signal.aborted) return "aborted";
		throw error;
	} finally {
		requests.delete(taskId);
	}
}
