import { describe, expect, it, vi } from "vitest";
import { createBlobDisplayUrl } from "./blobDisplayUrl";

describe("blob display URLs", () => {
	it("preserves SVG bytes and animation without creating a same-origin navigable Blob", async () => {
		const create = vi.spyOn(URL, "createObjectURL");
		const svg =
			'<svg xmlns="http://www.w3.org/2000/svg"><script>unsafe()</script><animate attributeName="x" dur="1s"/></svg>';
		const url = await createBlobDisplayUrl(
			new Blob([svg], { type: "image/svg+xml;charset=utf-8" }),
		);
		expect(url).toMatch(/^data:image\/svg\+xml/);
		expect(atob(url.slice(url.indexOf(",") + 1))).toBe(svg);
		expect(create).not.toHaveBeenCalled();
		create.mockRestore();
	});

	it("keeps ordinary image Blob URLs and their explicit native MIME", async () => {
		const create = vi
			.spyOn(URL, "createObjectURL")
			.mockReturnValue("blob:ordinary");
		const png = new Blob(["png"], { type: "image/png" });
		expect(await createBlobDisplayUrl(png)).toBe("blob:ordinary");
		expect(create).toHaveBeenCalledWith(png);
		create.mockRestore();
	});
});
