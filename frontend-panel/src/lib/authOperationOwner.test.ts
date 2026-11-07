import { beforeEach, describe, expect, it } from "vitest";
import {
	assertCurrentAuthOperation,
	captureUploadOperation,
	isCurrentAuthOperation,
	setAuthOperationOwner,
} from "./authOperationOwner";

describe("asynchronous operation ownership", () => {
	beforeEach(() => setAuthOperationOwner(null, true));

	it("keeps the selected workspace and survives same-account refresh", () => {
		setAuthOperationOwner(11);
		const workspace = { kind: "team" as const, teamId: 42 };
		const binding = captureUploadOperation(workspace);
		workspace.teamId = 99;
		setAuthOperationOwner(11);
		expect(binding.workspace).toEqual({ kind: "team", teamId: 42 });
		expect(isCurrentAuthOperation(binding)).toBe(true);
	});

	it("aborts all pending requests before logout can finish", () => {
		setAuthOperationOwner(11);
		const binding = captureUploadOperation({ kind: "personal" });
		setAuthOperationOwner(null);
		expect(binding.signal.aborted).toBe(true);
		expect(() => assertCurrentAuthOperation(binding)).toThrow();
	});

	it("cannot revive old tasks after logging back into the same account", () => {
		setAuthOperationOwner(11);
		const binding = captureUploadOperation({ kind: "personal" });
		setAuthOperationOwner(null);
		setAuthOperationOwner(11);
		expect(isCurrentAuthOperation(binding)).toBe(false);
		expect(captureUploadOperation({ kind: "personal" }).generation).not.toBe(
			binding.generation,
		);
	});

	it("rejects a different account even when its personal workspace looks identical", () => {
		setAuthOperationOwner(11);
		const binding = captureUploadOperation({ kind: "personal" });
		setAuthOperationOwner(12);
		expect(isCurrentAuthOperation(binding)).toBe(false);
	});
});
