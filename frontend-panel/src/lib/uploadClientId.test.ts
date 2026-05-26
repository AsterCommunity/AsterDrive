import { beforeEach, describe, expect, it, vi } from "vitest";
import { STORAGE_KEYS } from "@/config/app";
import { getUploadFrontendClientId } from "@/lib/uploadClientId";

describe("uploadClientId", () => {
	beforeEach(() => {
		localStorage.clear();
		vi.restoreAllMocks();
	});

	it("persists and reuses a frontend upload client UUID", () => {
		const first = getUploadFrontendClientId();
		const second = getUploadFrontendClientId();

		expect(first).toBe(second);
		expect(localStorage.getItem(STORAGE_KEYS.uploadFrontendClientId)).toBe(
			first,
		);
		expect(first).toMatch(
			/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i,
		);
	});

	it("replaces invalid stored values", () => {
		localStorage.setItem(STORAGE_KEYS.uploadFrontendClientId, "not-a-uuid");

		const clientId = getUploadFrontendClientId();

		expect(clientId).not.toBe("not-a-uuid");
		expect(localStorage.getItem(STORAGE_KEYS.uploadFrontendClientId)).toBe(
			clientId,
		);
	});

	it("reuses an in-memory UUID when localStorage is unavailable", () => {
		vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
			throw new Error("storage blocked");
		});

		const first = getUploadFrontendClientId();
		const second = getUploadFrontendClientId();

		expect(second).toBe(first);
		expect(first).toMatch(
			/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i,
		);
	});
});
