import { describe, expect, it } from "vitest";
import { getArchivePreviewFormatCapabilities } from "./archivePreviewFormatCapabilities";

describe("archive preview format capabilities", () => {
	it("keeps filename encoding available while the manifest format is pending", () => {
		expect(getArchivePreviewFormatCapabilities(null)).toEqual({
			filenameEncoding: true,
		});
		expect(getArchivePreviewFormatCapabilities(undefined)).toEqual({
			filenameEncoding: true,
		});
	});

	it("maps supported archive formats to their preview capabilities", () => {
		expect(getArchivePreviewFormatCapabilities("zip")).toEqual({
			filenameEncoding: true,
		});
		expect(getArchivePreviewFormatCapabilities("7z")).toEqual({
			filenameEncoding: false,
		});
	});

	it("falls back to conservative defaults for unknown formats", () => {
		expect(getArchivePreviewFormatCapabilities("tar" as never)).toEqual({
			filenameEncoding: false,
		});
	});
});
