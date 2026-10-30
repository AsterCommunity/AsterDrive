import { beforeEach, describe, expect, it, vi } from "vitest";
import { startAdminContentDownload } from "./adminContentDownload";

const mocks = vi.hoisted(() => ({
	fresh: vi.fn(),
	init: vi.fn(),
	native: vi.fn(),
}));
vi.mock("@/stores/authStore", () => ({
	useAuthStore: { getState: () => ({ ensureFreshSession: mocks.fresh }) },
}));
vi.mock("@/services/adminContentAccessService", () => ({
	adminContentAccessService: { initDownload: mocks.init },
}));
vi.mock("@/lib/authenticatedDownload", () => ({
	startBrowserDownload: mocks.native,
}));
describe("administrator download initialization", () => {
	beforeEach(() => {
		vi.resetAllMocks();
		mocks.fresh.mockResolvedValue(undefined);
	});
	it.each([
		"https://objects.test/file?signature=private",
		"/admin/content-access/grant/download",
	])("delivers %s through the browser after audited init", async (url) => {
		mocks.init.mockResolvedValue({
			resource: { request: { url } },
			name: "report.txt",
		});
		await startAdminContentDownload("scope-grant", 42, 99);
		expect(mocks.fresh).toHaveBeenCalledOnce();
		expect(mocks.init).toHaveBeenCalledWith(42, {
			access_id: "scope-grant",
			revision_id: 99,
		});
		expect(mocks.native).toHaveBeenCalledWith(url, "report.txt");
		expect(mocks.fresh.mock.invocationCallOrder[0]).toBeLessThan(
			mocks.init.mock.invocationCallOrder[0],
		);
		expect(mocks.init.mock.invocationCallOrder[0]).toBeLessThan(
			mocks.native.mock.invocationCallOrder[0],
		);
	});
	it("withholds downloads when initialization or session renewal fails", async () => {
		mocks.init.mockRejectedValueOnce(new Error("audit unavailable"));
		await expect(startAdminContentDownload("grant", 42)).rejects.toThrow(
			"audit unavailable",
		);
		expect(mocks.native).not.toHaveBeenCalled();
		mocks.init.mockClear();
		mocks.fresh.mockRejectedValueOnce(new Error("revoked"));
		await expect(startAdminContentDownload("grant", 42)).rejects.toThrow(
			"revoked",
		);
		expect(mocks.init).not.toHaveBeenCalled();
	});
});
