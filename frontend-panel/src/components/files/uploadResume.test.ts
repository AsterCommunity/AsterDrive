import { describe, expect, it } from "vitest";
import {
	CHUNK_PROCESSING_PROGRESS,
	getProcessingProgress,
	getResumePlan,
	SERVER_FINALIZE_PROGRESS,
} from "@/components/files/uploadResume";

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

	it("maps provider resumable statuses to sequential resume or completion", () => {
		expect(getResumePlan("provider_resumable", "uploading")).toBe("upload");
		expect(getResumePlan("provider_resumable", "assembling")).toBe("complete");
		expect(getResumePlan("provider_resumable", "completed")).toBe("complete");
		expect(getResumePlan("provider_resumable", "failed")).toBe("restart");
	});

	it("maps stream and single-request presigned statuses conservatively", () => {
		expect(getResumePlan("stream", "uploading")).toBe("upload");
		expect(getResumePlan("stream", "assembling")).toBe("complete");
		expect(getResumePlan("stream", "completed")).toBe("complete");
		expect(getResumePlan("stream", "failed")).toBe("restart");
		expect(getResumePlan("stream", "presigned")).toBe("restart");
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
		expect(getProcessingProgress("provider_resumable")).toBe(
			SERVER_FINALIZE_PROGRESS,
		);
		expect(getProcessingProgress("stream")).toBe(SERVER_FINALIZE_PROGRESS);
		expect(getProcessingProgress(null)).toBe(SERVER_FINALIZE_PROGRESS);
	});
});
