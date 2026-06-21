import { describe, expect, it } from "vitest";
import {
	CHUNK_PROCESSING_PROGRESS,
	getProcessingProgress,
	getResumePlan,
	SERVER_FINALIZE_PROGRESS,
} from "@/components/files/uploadResume";
import type { UploadSessionStatus } from "@/types/api";

describe("uploadResume", () => {
	it("maps chunked session statuses to the expected resume plan", () => {
		expect(getResumePlan("chunked", "uploading")).toBe("upload");
		expect(getResumePlan("chunked", "assembling")).toBe("complete");
		expect(getResumePlan("chunked", "completed")).toBe("complete");
		expect(getResumePlan("chunked", "failed")).toBe("restart");
		expect(getResumePlan("chunked", "presigned")).toBe("restart");
	});

	it("maps multipart presigned statuses to the expected resume plan", () => {
		expect(getResumePlan("presigned_multipart", "presigned")).toBe("upload");
		expect(getResumePlan("presigned_multipart", "assembling")).toBe("complete");
		expect(getResumePlan("presigned_multipart", "completed")).toBe("complete");
		expect(getResumePlan("presigned_multipart", "uploading")).toBe("restart");
		expect(getResumePlan("presigned_multipart", "failed")).toBe("restart");
	});

	it("maps direct and single-request presigned statuses conservatively", () => {
		const statuses: UploadSessionStatus[] = [
			"uploading",
			"assembling",
			"completed",
			"failed",
			"presigned",
		];

		for (const status of statuses) {
			expect(getResumePlan("direct", status)).toBe("restart");
		}
		expect(getResumePlan("presigned", "presigned")).toBe("complete");
		expect(getResumePlan("presigned", "assembling")).toBe("complete");
		expect(getResumePlan("presigned", "completed")).toBe("complete");
		expect(getResumePlan("presigned", "uploading")).toBe("restart");
		expect(getResumePlan("presigned", "failed")).toBe("restart");
	});

	it("uses chunk processing progress only for chunked assembly", () => {
		expect(getProcessingProgress("chunked")).toBe(CHUNK_PROCESSING_PROGRESS);
		expect(getProcessingProgress("presigned_multipart")).toBe(
			SERVER_FINALIZE_PROGRESS,
		);
		expect(getProcessingProgress("presigned")).toBe(SERVER_FINALIZE_PROGRESS);
		expect(getProcessingProgress("direct")).toBe(SERVER_FINALIZE_PROGRESS);
		expect(getProcessingProgress(null)).toBe(SERVER_FINALIZE_PROGRESS);
	});
});
