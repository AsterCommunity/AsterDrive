import { describe, expect, it } from "vitest";
import {
	classifySharedFile,
	compoundExtensionFromName,
	extensionFromName,
} from "./shareFileClassification";

describe("shareFileClassification", () => {
	it("uses extensionless and dotfile names as classification keys", () => {
		expect(extensionFromName("Dockerfile")).toBe("dockerfile");
		expect(extensionFromName(" Makefile ")).toBe("makefile");
		expect(extensionFromName(".env")).toBe("env");
		expect(extensionFromName("archive.")).toBe("");
		expect(extensionFromName("app.TS")).toBe("ts");
		expect(extensionFromName(" backup.TAR.GZ ")).toBe("gz");
	});

	it("detects supported compound archive extensions", () => {
		expect(compoundExtensionFromName(" backup.TAR.GZ ")).toBe("tar.gz");
		expect(compoundExtensionFromName("package.tar.zst")).toBe("tar.zst");
		expect(compoundExtensionFromName("image.gz")).toBeNull();
	});

	it("prefers extension categories before falling back to MIME types", () => {
		expect(
			classifySharedFile("backup.bin", "application/octet-stream", "tar.gz"),
		).toBe("archive");
		expect(classifySharedFile("backup.zip", "image/png", null)).toBe("archive");
		expect(
			classifySharedFile("sheet.XLSX", "application/octet-stream", null),
		).toBe("spreadsheet");
		expect(
			classifySharedFile("slides.key", "application/octet-stream", null),
		).toBe("presentation");
		expect(
			classifySharedFile("photo.heic", "application/octet-stream", null),
		).toBe("image");
		expect(
			classifySharedFile("movie.m2ts", "application/octet-stream", null),
		).toBe("video");
		expect(
			classifySharedFile("song.opus", "application/octet-stream", null),
		).toBe("audio");
		expect(
			classifySharedFile("paper.markdown", "application/octet-stream", null),
		).toBe("document");
		expect(classifySharedFile("Dockerfile", "text/plain", null)).toBe("code");
		expect(classifySharedFile("Makefile", "text/plain", null)).toBe("code");
		expect(classifySharedFile("index.ts", "video/mp2t", null)).toBe("code");
		expect(classifySharedFile("component.tsx", "text/plain", null)).toBe(
			"code",
		);
	});

	it("falls back to MIME types when the file extension is unknown", () => {
		const cases = [
			["image/png", "asset.unknown", "image"],
			["video/mp4", "stream.unknown", "video"],
			["audio/ogg", "track.unknown", "audio"],
			["application/pdf", "manual.unknown", "document"],
			["text/x-log", "notes.unknown", "document"],
			["application/vnd.ms-excel", "sheet.unknown", "spreadsheet"],
			["text/csv", "export.unknown", "spreadsheet"],
			[
				"application/vnd.openxmlformats-officedocument.presentationml.presentation",
				"slides.unknown",
				"presentation",
			],
			["application/x-7z-compressed", "bundle.unknown", "archive"],
			["application/activity+json", "payload.unknown", "code"],
			["application/xml", "config.unknown", "code"],
			["application/octet-stream", "blob.unknown", "other"],
		] as const;

		for (const [mimeType, name, category] of cases) {
			expect(classifySharedFile(name, mimeType, null)).toBe(category);
		}
	});
});
