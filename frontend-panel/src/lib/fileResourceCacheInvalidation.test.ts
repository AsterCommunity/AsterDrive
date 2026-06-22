import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	invalidateAllFileResourceCaches,
	invalidateFileResourceCachesForMutation,
} from "@/lib/fileResourceCacheInvalidation";

const mockState = vi.hoisted(() => ({
	invalidateBlobUrl: vi.fn(),
	invalidateTextContent: vi.fn(),
}));

vi.mock("@/hooks/useBlobUrl", () => ({
	invalidateBlobUrl: (...args: unknown[]) =>
		mockState.invalidateBlobUrl(...args),
}));

vi.mock("@/hooks/useTextContent", () => ({
	invalidateTextContent: (...args: unknown[]) =>
		mockState.invalidateTextContent(...args),
}));

describe("fileResourceCacheInvalidation", () => {
	beforeEach(() => {
		mockState.invalidateBlobUrl.mockReset();
		mockState.invalidateTextContent.mockReset();
	});

	it("invalidates text, original blob, thumbnail, and image preview cache keys", () => {
		invalidateFileResourceCachesForMutation({
			download: "/files/7/download",
			thumbnail: "/files/7/thumbnail",
			imagePreview: "/files/7/image-preview",
		});

		expect(mockState.invalidateTextContent).toHaveBeenCalledWith(
			"/files/7/download",
		);
		expect(mockState.invalidateBlobUrl).toHaveBeenNthCalledWith(
			1,
			"/files/7/download",
		);
		expect(mockState.invalidateBlobUrl).toHaveBeenNthCalledWith(
			2,
			"/files/7/thumbnail",
		);
		expect(mockState.invalidateBlobUrl).toHaveBeenNthCalledWith(
			3,
			"/files/7/image-preview",
		);
	});

	it("can invalidate only the original resource when no derived paths exist", () => {
		invalidateFileResourceCachesForMutation({
			download: "/files/8/download",
		});

		expect(mockState.invalidateTextContent).toHaveBeenCalledWith(
			"/files/8/download",
		);
		expect(mockState.invalidateBlobUrl).toHaveBeenCalledWith(
			"/files/8/download",
		);
	});

	it("clears all file resource caches", () => {
		invalidateAllFileResourceCaches();

		expect(mockState.invalidateBlobUrl).toHaveBeenCalledWith();
		expect(mockState.invalidateTextContent).toHaveBeenCalledWith();
	});
});
