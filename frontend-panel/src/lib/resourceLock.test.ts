import { describe, expect, it } from "vitest";
import { isDirectResourceLock, isResourceLocked } from "@/lib/resourceLock";

describe("resourceLock", () => {
	it("distinguishes unlocked, direct, and inherited projections", () => {
		expect(isResourceLocked({ state: "unlocked" })).toBe(false);
		expect(isResourceLocked({ state: "direct", mode: "exclusive" })).toBe(true);
		expect(
			isResourceLocked({
				state: "inherited",
				root: { kind: "workspace_root" },
				mode: "shared",
			}),
		).toBe(true);
	});

	it("recognizes only direct lock projections as directly releasable", () => {
		expect(isDirectResourceLock({ state: "unlocked" })).toBe(false);
		expect(isDirectResourceLock({ state: "direct", mode: "exclusive" })).toBe(
			true,
		);
		expect(
			isDirectResourceLock({
				state: "inherited",
				root: { kind: "folder", folder_id: 3 },
				mode: "exclusive",
			}),
		).toBe(false);
	});
});
