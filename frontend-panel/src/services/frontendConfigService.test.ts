import { beforeEach, describe, expect, it, vi } from "vitest";
import { frontendConfigService } from "@/services/frontendConfigService";

const apiGet = vi.hoisted(() => vi.fn());

vi.mock("@/services/http", () => ({
	api: {
		get: apiGet,
	},
}));

describe("frontendConfigService", () => {
	beforeEach(() => {
		apiGet.mockReset();
	});

	it("loads public frontend config from the public endpoint", () => {
		frontendConfigService.get();

		expect(apiGet).toHaveBeenCalledWith("/public/frontend-config");
	});
});
