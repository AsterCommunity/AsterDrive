import { describe, expect, it } from "vitest";
import { ensureZipExtension } from "@/lib/downloadFilenames";

describe("ensureZipExtension", () => {
	it("adds the ZIP suffix once and normalizes empty names", () => {
		expect(ensureZipExtension("bundle")).toBe("bundle.zip");
		expect(ensureZipExtension("Bundle.ZIP")).toBe("Bundle.ZIP");
		expect(ensureZipExtension("  ")).toBe("asterdrive-download.zip");
	});
});
