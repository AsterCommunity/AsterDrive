import { beforeEach, describe, expect, it, vi } from "vitest";
import { createSearchService, searchService } from "@/services/searchService";
import type { SearchParams } from "@/types/api";

const { apiGet } = vi.hoisted(() => ({
	apiGet: vi.fn(),
}));

vi.mock("@/services/http", () => ({
	api: {
		get: apiGet,
	},
}));

describe("searchService", () => {
	beforeEach(() => {
		apiGet.mockReset();
	});

	it("serializes non-empty search params into the query string", () => {
		const params = {
			q: "report",
			category: "image",
			extensions: "pdf,docx",
			folder_id: 7,
			mime_type: "",
			limit: 50,
			type: null,
			offset: 0,
		} satisfies SearchParams;

		searchService.search(params);

		expect(apiGet).toHaveBeenCalledWith(
			"/search?q=report&category=image&extensions=pdf%2Cdocx&folder_id=7&limit=50&offset=0",
			undefined,
		);

		const teamSearchService = createSearchService({ kind: "team", teamId: 3 });
		teamSearchService.search(params);

		expect(apiGet).toHaveBeenNthCalledWith(
			2,
			"/teams/3/search?q=report&category=image&extensions=pdf%2Cdocx&folder_id=7&limit=50&offset=0",
			undefined,
		);
	});

	it("forwards abort signals to the request layer", () => {
		const controller = new AbortController();

		searchService.search({ q: "report" }, { signal: controller.signal });

		expect(apiGet).toHaveBeenCalledWith("/search?q=report", {
			signal: controller.signal,
		});
	});
});
