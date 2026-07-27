import { describe, expect, it } from "vitest";
import type { InitUploadResponse } from "@/services/uploadService";
import { resolveChunkConcurrency } from "./uploadAreaResumableUploadRunners";

function initWithMax(
	max?: number,
	chunkOrdering: "unordered" | "sequential" = "unordered",
): InitUploadResponse {
	return {
		mode: "chunked",
		upload_id: "upload-1",
		chunk_size: 1024,
		total_chunks: 2,
		upload_scheduling:
			max === undefined
				? undefined
				: {
						chunk_ordering: chunkOrdering,
						max_chunk_concurrency: max,
					},
	};
}

describe("resolveChunkConcurrency", () => {
	it("clamps the client worker count to the backend maximum", () => {
		expect(resolveChunkConcurrency(initWithMax(1), 3)).toBe(1);
		expect(resolveChunkConcurrency(initWithMax(2), 3)).toBe(2);
		expect(resolveChunkConcurrency(initWithMax(16), 3)).toBe(3);
	});

	it("honors sequential ordering even if the maximum is larger", () => {
		expect(resolveChunkConcurrency(initWithMax(4, "sequential"), 3)).toBe(1);
	});

	it("keeps the compatibility fallback when scheduling metadata is absent", () => {
		expect(resolveChunkConcurrency(initWithMax(), 3)).toBe(3);
		expect(resolveChunkConcurrency(initWithMax(0), 3)).toBe(3);
	});

	it("normalizes invalid and fractional compatibility fallbacks", () => {
		for (const fallback of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
			expect(resolveChunkConcurrency(initWithMax(), fallback)).toBe(1);
		}
		expect(resolveChunkConcurrency(initWithMax(), 3.9)).toBe(3);
	});

	it("ignores invalid backend maxima and floors fractional maxima", () => {
		for (const backendMax of [-1, 0, Number.NaN, Number.POSITIVE_INFINITY]) {
			expect(resolveChunkConcurrency(initWithMax(backendMax), 4)).toBe(4);
		}
		expect(resolveChunkConcurrency(initWithMax(2.9), 4)).toBe(2);
		expect(resolveChunkConcurrency(initWithMax(2), 1)).toBe(1);
	});

	it("keeps sequential scheduling at one for invalid limits and fallbacks", () => {
		expect(
			resolveChunkConcurrency(
				initWithMax(Number.NaN, "sequential"),
				Number.POSITIVE_INFINITY,
			),
		).toBe(1);
	});
});
