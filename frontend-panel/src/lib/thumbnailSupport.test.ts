import { describe, expect, it } from "vitest";
import {
	getThumbnailExtension,
	supportsThumbnailExtension,
} from "@/lib/thumbnailSupport";

describe("thumbnailSupport", () => {
	it("normalizes thumbnail file extensions", () => {
		expect(getThumbnailExtension(" Photo.JPG ")).toBe("jpg");
		expect(getThumbnailExtension("archive.tar.gz")).toBe("gz");
		expect(getThumbnailExtension(".gitignore")).toBe("");
		expect(getThumbnailExtension("no-extension")).toBe("");
		expect(getThumbnailExtension("trailing.")).toBe("");
	});

	it("checks configured thumbnail support case-insensitively", () => {
		expect(
			supportsThumbnailExtension("cover.MP3", [" .jpg ", ".mp3", "png"]),
		).toBe(true);
		expect(supportsThumbnailExtension("cover.mp3", [])).toBe(false);
		expect(supportsThumbnailExtension("cover.mp3", undefined)).toBe(false);
		expect(supportsThumbnailExtension("cover", ["mp3"])).toBe(false);
		expect(supportsThumbnailExtension("cover.flac", ["mp3"])).toBe(false);
	});
});
