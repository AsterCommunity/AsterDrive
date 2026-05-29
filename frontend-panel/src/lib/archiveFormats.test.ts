import { describe, expect, it } from "vitest";
import {
	detectArchiveFormatByName,
	isExtractableArchiveFileName,
	isSupportedArchiveFormat,
	isSupportedArchivePreviewFile,
} from "@/lib/archiveFormats";

describe("archive format helpers", () => {
	it("detects supported archive formats from normalized file extensions", () => {
		expect(detectArchiveFormatByName(" bundle.ZIP ")).toBe("zip");
		expect(detectArchiveFormatByName("backup.7z")).toBe("7z");
		expect(detectArchiveFormatByName("archive")).toBeNull();
		expect(detectArchiveFormatByName("archive.tar")).toBeNull();
	});

	it("guards supported archive format identifiers", () => {
		expect(isSupportedArchiveFormat("zip")).toBe(true);
		expect(isSupportedArchiveFormat("7z")).toBe(true);
		expect(isSupportedArchiveFormat("tar")).toBe(false);
	});

	it("identifies extractable archive file names", () => {
		expect(isExtractableArchiveFileName("photos.zip")).toBe(true);
		expect(isExtractableArchiveFileName("photos.7z")).toBe(true);
		expect(isExtractableArchiveFileName("photos.rar")).toBe(false);
	});

	it("uses extensions before falling back to supported preview MIME types", () => {
		expect(
			isSupportedArchivePreviewFile({
				name: "archive.7z",
				mime_type: "application/octet-stream",
			}),
		).toBe(true);
		expect(
			isSupportedArchivePreviewFile({
				name: "download.bin",
				mime_type: "application/X-7Z-COMPRESSED",
			}),
		).toBe(true);
		expect(
			isSupportedArchivePreviewFile({
				name: "download.bin",
				mime_type: "application/x-zip-compressed",
			}),
		).toBe(true);
		expect(
			isSupportedArchivePreviewFile({
				name: "download.bin",
				mime_type: null,
			}),
		).toBe(false);
		expect(
			isSupportedArchivePreviewFile({
				name: "download.bin",
			}),
		).toBe(false);
	});
});
