import type { UploadProgressResponse } from "@/types/api";

export type UploadMode =
	| "direct"
	| "chunked"
	| "presigned"
	| "presigned_multipart";

export type ResumePlan = "upload" | "complete" | "restart";

export const CHUNK_PROCESSING_PROGRESS = 95;
export const SERVER_FINALIZE_PROGRESS = 95;

export function getResumePlan(
	mode: UploadMode,
	status: UploadProgressResponse["status"],
): ResumePlan {
	if (mode === "chunked") {
		if (status === "uploading") return "upload";
		if (status === "assembling" || status === "completed") return "complete";
		return "restart";
	}

	if (mode === "presigned_multipart") {
		if (status === "presigned") return "upload";
		if (status === "assembling" || status === "completed") return "complete";
		return "restart";
	}

	if (mode === "presigned") {
		if (
			status === "presigned" ||
			status === "assembling" ||
			status === "completed"
		) {
			return "complete";
		}
		return "restart";
	}

	return "restart";
}

export function getProcessingProgress(mode: UploadMode | null): number {
	return mode === "chunked"
		? CHUNK_PROCESSING_PROGRESS
		: SERVER_FINALIZE_PROGRESS;
}
