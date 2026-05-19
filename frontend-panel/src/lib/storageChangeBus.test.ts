import { afterEach, describe, expect, it, vi } from "vitest";
import { logger } from "@/lib/logger";
import {
	publishStorageChange,
	subscribeStorageChange,
} from "@/lib/storageChangeBus";
import type { StorageChangeEventPayload } from "@/lib/storageEventEcho";

vi.mock("@/lib/logger", () => ({
	logger: {
		error: vi.fn(),
	},
}));

const event = {
	affected_parent_ids: [],
	affects_quota: false,
	at: "2026-05-19T00:00:00Z",
	file_ids: [],
	folder_ids: [],
	kind: "sync.required",
	root_affected: false,
	storage_delta: null,
	workspace: { kind: "personal" },
} satisfies StorageChangeEventPayload;

describe("storageChangeBus", () => {
	afterEach(() => {
		vi.clearAllMocks();
	});

	it("continues dispatching when one listener throws", () => {
		const first = vi.fn(() => {
			throw new Error("boom");
		});
		const second = vi.fn();
		const unsubscribeFirst = subscribeStorageChange(first);
		const unsubscribeSecond = subscribeStorageChange(second);

		try {
			publishStorageChange(event);
		} finally {
			unsubscribeFirst();
			unsubscribeSecond();
		}

		expect(first).toHaveBeenCalledWith(event);
		expect(second).toHaveBeenCalledWith(event);
		expect(logger.error).toHaveBeenCalledWith(
			"storage change listener failed",
			expect.any(Error),
		);
	});

	it("logs async listener rejections without rethrowing", async () => {
		const error = new Error("async boom");
		const listener = vi.fn(() => Promise.reject(error));
		const unsubscribe = subscribeStorageChange(listener);

		try {
			publishStorageChange(event);
			await Promise.resolve();
		} finally {
			unsubscribe();
		}

		expect(listener).toHaveBeenCalledWith(event);
		expect(logger.error).toHaveBeenCalledWith(
			"storage change listener failed",
			error,
		);
	});
});
