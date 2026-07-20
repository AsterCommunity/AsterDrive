import { describe, expect, it } from "vitest";
import { ensureZipExtension } from "@/lib/downloadFilenames";

describe("ensureZipExtension", () => {
	it("adds the ZIP suffix once and normalizes empty names", () => {
		expect(ensureZipExtension("bundle")).toBe("bundle.zip");
		expect(ensureZipExtension("Bundle.ZIP")).toBe("Bundle.ZIP");
		expect(ensureZipExtension("  ")).toBe("asterdrive-download.zip");
	});

	it("trims names while preserving unicode, dotfiles, and non-ZIP suffixes", () => {
		expect(ensureZipExtension("  项目归档  ")).toBe("项目归档.zip");
		expect(ensureZipExtension("archive.zip.tmp")).toBe("archive.zip.tmp.zip");
		expect(ensureZipExtension(".zip")).toBe(".zip");
		expect(ensureZipExtension("backup.tar.gz")).toBe("backup.tar.gz.zip");
	});
});
