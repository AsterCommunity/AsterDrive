import { afterEach, describe, expect, it, vi } from "vitest";
import { resolveApiResourceUrl } from "@/lib/apiUrl";

const appConfig = await import("@/config/app");

describe("resolveApiResourceUrl", () => {
	afterEach(() => {
		vi.restoreAllMocks();
	});

	it("joins workspace API paths to the configured API base URL", () => {
		expect(resolveApiResourceUrl("/files/7/download")).toBe(
			"/api/v1/files/7/download",
		);
		expect(resolveApiResourceUrl("files/7/download")).toBe(
			"/api/v1/files/7/download",
		);
	});

	it("keeps resource URLs that are already addressable by the browser unchanged", () => {
		for (const url of [
			"https://cdn.example.com/file.pdf",
			"http://cdn.example.com/file.pdf",
			"blob:pdf-preview",
			"/api/v1/files/7/download",
			"/d/token/file.pdf",
			"/pv/token/file.pdf",
		]) {
			expect(resolveApiResourceUrl(url)).toBe(url);
		}
	});

	it("joins public share API paths because they live under the API base URL", () => {
		expect(resolveApiResourceUrl("/s/share-token/download")).toBe(
			"/api/v1/s/share-token/download",
		);
	});

	it("uses the runtime API base URL value", () => {
		vi.spyOn(appConfig.config, "apiBaseUrl", "get").mockReturnValue(
			"https://api.example.com/v1///",
		);

		expect(resolveApiResourceUrl("/files/7/download")).toBe(
			"https://api.example.com/v1/files/7/download",
		);
	});
});
