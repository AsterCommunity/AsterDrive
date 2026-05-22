import { beforeEach, describe, expect, it, vi } from "vitest";
import { mediaDataSupportService } from "@/services/mediaDataSupportService";

const apiGet = vi.hoisted(() => vi.fn());

vi.mock("@/services/http", () => ({
	api: {
		get: apiGet,
	},
}));

describe("mediaDataSupportService", () => {
	beforeEach(() => {
		apiGet.mockReset();
	});

	it("loads public media data support from the public endpoint", () => {
		mediaDataSupportService.get();

		expect(apiGet).toHaveBeenCalledWith("/public/media-data-support");
	});
});
