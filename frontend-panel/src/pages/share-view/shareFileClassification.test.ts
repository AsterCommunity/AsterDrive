import { describe, expect, it } from "vitest";
import {
	classifySharedFile,
	extensionFromName,
} from "./shareFileClassification";

describe("shareFileClassification", () => {
	it("uses extensionless and dotfile names as classification keys", () => {
		expect(extensionFromName("Dockerfile")).toBe("dockerfile");
		expect(extensionFromName(" Makefile ")).toBe("makefile");
		expect(extensionFromName(".env")).toBe("env");
		expect(extensionFromName("archive.")).toBe("");
		expect(extensionFromName("app.TS")).toBe("ts");
	});

	it("classifies extensionless build files and TypeScript files as code", () => {
		expect(classifySharedFile("Dockerfile", "text/plain", null)).toBe("code");
		expect(classifySharedFile("Makefile", "text/plain", null)).toBe("code");
		expect(classifySharedFile("index.ts", "video/mp2t", null)).toBe("code");
		expect(classifySharedFile("component.tsx", "text/plain", null)).toBe(
			"code",
		);
	});

	it("falls back to MIME types when the file extension is unknown", () => {
		expect(classifySharedFile("asset", "image/png", null)).toBe("image");
		expect(classifySharedFile("stream", "video/mp4", null)).toBe("video");
		expect(classifySharedFile("track", "audio/ogg", null)).toBe("audio");
		expect(classifySharedFile("manual", "application/pdf", null)).toBe(
			"document",
		);
		expect(classifySharedFile("notes", "text/x-log", null)).toBe("document");
		expect(classifySharedFile("sheet", "application/vnd.ms-excel", null)).toBe(
			"spreadsheet",
		);
		expect(
			classifySharedFile(
				"slides",
				"application/vnd.ms-powerpoint.presentation",
				null,
			),
		).toBe("presentation");
		expect(
			classifySharedFile("bundle", "application/x-7z-compressed", null),
		).toBe("archive");
		expect(
			classifySharedFile("payload", "application/activity+json", null),
		).toBe("code");
		expect(classifySharedFile("blob", "application/octet-stream", null)).toBe(
			"other",
		);
	});
});
