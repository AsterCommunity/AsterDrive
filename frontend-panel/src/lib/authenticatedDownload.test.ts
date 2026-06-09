import { beforeEach, describe, expect, it, vi } from "vitest";
import { startAuthenticatedDownload } from "@/lib/authenticatedDownload";

const mockState = vi.hoisted(() => ({
	ensureFreshSession: vi.fn(),
	loggerError: vi.fn(),
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: {
		getState: () => ({
			ensureFreshSession: mockState.ensureFreshSession,
		}),
	},
}));

vi.mock("@/lib/logger", () => ({
	logger: {
		error: (...args: unknown[]) => mockState.loggerError(...args),
	},
}));

describe("startAuthenticatedDownload", () => {
	beforeEach(() => {
		mockState.ensureFreshSession.mockReset();
		mockState.ensureFreshSession.mockResolvedValue(undefined);
		mockState.loggerError.mockReset();
	});

	it("ensures the session is fresh before starting a browser download", async () => {
		const createElement = document.createElement.bind(document);
		const anchor = createElement("a");
		const clickSpy = vi.spyOn(anchor, "click").mockImplementation(() => {});
		const createElementSpy = vi
			.spyOn(document, "createElement")
			.mockImplementation(((tagName: string) =>
				tagName === "a"
					? anchor
					: createElement(tagName)) as typeof document.createElement);

		await startAuthenticatedDownload("/files/24/download");

		expect(mockState.ensureFreshSession).toHaveBeenCalledTimes(1);
		expect(anchor.getAttribute("href")).toBe("/api/v1/files/24/download");
		expect(anchor.download).toBe("");
		expect(anchor.isConnected).toBe(false);
		expect(clickSpy).toHaveBeenCalledTimes(1);

		createElementSpy.mockRestore();
		clickSpy.mockRestore();
	});

	it("does not start the browser download when session refresh fails", async () => {
		const error = new Error("refresh failed");
		mockState.ensureFreshSession.mockRejectedValue(error);
		const createElementSpy = vi.spyOn(document, "createElement");

		await expect(startAuthenticatedDownload("/files/24/download")).rejects.toBe(
			error,
		);

		expect(createElementSpy).not.toHaveBeenCalled();
		expect(mockState.loggerError).toHaveBeenCalledWith(
			"authenticated download session refresh failed",
			"/files/24/download",
			error,
		);
		createElementSpy.mockRestore();
	});
});
