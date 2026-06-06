import { describe, expect, it } from "vitest";
import { canBrowserRenderImage } from "@/lib/browserImageSupport";

describe("browserImageSupport", () => {
	it("allows image formats that browsers can render directly", () => {
		expect(canBrowserRenderImage({ name: "photo.JPG", mime_type: "" })).toBe(
			true,
		);
		expect(
			canBrowserRenderImage({ name: "upload", mime_type: "image/png" }),
		).toBe(true);
		expect(
			canBrowserRenderImage({
				name: "logo",
				mime_type: " image/svg+xml; charset=utf-8 ",
			}),
		).toBe(true);
		expect(
			canBrowserRenderImage({
				name: "cover.webp",
				mime_type: "application/octet-stream",
			}),
		).toBe(true);
	});

	it("rejects image formats that need backend conversion first", () => {
		expect(
			canBrowserRenderImage({
				name: "capture.nef",
				mime_type: "image/x-nikon-nef",
			}),
		).toBe(false);
		expect(
			canBrowserRenderImage({ name: "photo.heic", mime_type: "image/heic" }),
		).toBe(false);
		expect(
			canBrowserRenderImage({ name: "scan.tiff", mime_type: "image/tiff" }),
		).toBe(false);
		expect(
			canBrowserRenderImage({ name: "modern.avif", mime_type: "image/avif" }),
		).toBe(false);
	});

	it("lets known unsupported extensions override browser-renderable MIME types", () => {
		expect(
			canBrowserRenderImage({ name: "capture.NEF", mime_type: "image/jpeg" }),
		).toBe(false);
		expect(
			canBrowserRenderImage({ name: "modern.avif", mime_type: "image/png" }),
		).toBe(false);
	});

	it("handles whitespace, missing names, hidden files, and MIME casing", () => {
		expect(
			canBrowserRenderImage({
				name: "  scan.jpeg  ",
				mime_type: "application/octet-stream",
			}),
		).toBe(true);
		expect(
			canBrowserRenderImage({
				name: undefined,
				mime_type: " IMAGE/PNG; charset=binary ",
			}),
		).toBe(true);
		expect(
			canBrowserRenderImage({
				name: ".jpg",
				mime_type: "application/octet-stream",
			}),
		).toBe(false);
		expect(
			canBrowserRenderImage({
				name: "archive.",
				mime_type: "application/octet-stream",
			}),
		).toBe(false);
	});

	it("does not infer support from extensionless generic image MIME types", () => {
		expect(
			canBrowserRenderImage({
				name: "upload",
				mime_type: "image/x-canon-cr2",
			}),
		).toBe(false);
		expect(
			canBrowserRenderImage({
				name: "upload",
				mime_type: "application/octet-stream",
			}),
		).toBe(false);
	});
});
