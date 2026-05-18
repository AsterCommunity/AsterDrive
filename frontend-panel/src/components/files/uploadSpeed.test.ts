import { describe, expect, it } from "vitest";
import { createUploadSpeedTracker } from "@/components/files/uploadSpeed";

describe("createUploadSpeedTracker", () => {
	it("waits for the minimum sample window before reporting speed", () => {
		let now = 1_000;
		const tracker = createUploadSpeedTracker(0, () => now);

		now += 100;
		expect(tracker.sample(512)).toEqual({
			uploadedBytes: 512,
			speedBps: undefined,
		});

		now += 150;
		expect(tracker.sample(1_024)).toEqual({
			uploadedBytes: 1_024,
			speedBps: 4_096,
		});
	});

	it("smooths later speed samples", () => {
		let now = 0;
		const tracker = createUploadSpeedTracker(0, () => now);

		now = 500;
		expect(tracker.sample(1_000).speedBps).toBe(2_000);

		now = 1_000;
		expect(tracker.sample(3_000).speedBps).toBe(2_600);
	});

	it("reports zero speed when no bytes move during a full sample window", () => {
		let now = 0;
		const tracker = createUploadSpeedTracker(0, () => now);

		now = 500;
		expect(tracker.sample(1_000).speedBps).toBe(2_000);

		now = 1_000;
		expect(tracker.sample(1_000)).toEqual({
			uploadedBytes: 1_000,
			speedBps: 0,
		});
	});

	it("resets the current rate when uploaded bytes move backwards", () => {
		let now = 0;
		const tracker = createUploadSpeedTracker(1_000, () => now);

		now = 500;
		expect(tracker.sample(2_000).speedBps).toBe(2_000);

		now = 1_000;
		expect(tracker.sample(500)).toEqual({
			uploadedBytes: 500,
			speedBps: undefined,
		});

		now = 1_500;
		expect(tracker.sample(1_500).speedBps).toBe(2_000);
	});

	it("stops with the final byte count and clears speed", () => {
		let now = 0;
		const tracker = createUploadSpeedTracker(0, () => now);

		now = 500;
		expect(tracker.sample(2_000).speedBps).toBe(4_000);
		expect(tracker.stop(4_096)).toEqual({
			uploadedBytes: 4_096,
			speedBps: undefined,
		});
	});
});
