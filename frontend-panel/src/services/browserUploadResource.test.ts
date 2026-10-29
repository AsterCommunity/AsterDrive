import { afterEach, describe, expect, it, vi } from "vitest";
import type { BrowserUploadStep } from "@/types/api";
import { runBrowserUpload } from "./browserUploadResource";

function step(offset = 0, completed = false): BrowserUploadStep {
	return {
		executor_version: 1,
		protocol: { id: "plugin.unknown.to.browser", version: 99 },
		confirmed_offset: offset,
		progress_revision: offset,
		provider_completed: completed,
		instruction: completed
			? null
			: {
					request_id: `id-${offset}`,
					response_body_limit: 65536,
					timeout_ms: 60000,
					request: {
						url: "https://plugin.invalid",
						method: "POST",
						headers: {},
						body: { start: offset, end: Math.min(offset + 4, 8) },
						response_headers: ["x-arbitrary"],
					},
				},
	};
}
function setup(initial = step(), file = new File(["abcdefgh"], "file")) {
	const coordinateBrowserUpload = vi.fn();
	if (!initial.provider_completed)
		coordinateBrowserUpload.mockResolvedValueOnce(initial);
	const execute = vi.fn().mockResolvedValue({
		status: 298,
		headers: { "x-arbitrary": "yes" },
		body: "opaque",
		body_truncated: false,
	});
	const service = { coordinateBrowserUpload, executeBrowserUpload: execute };
	const options = {
		execute,
		isAborted: vi.fn(() => false),
		onProgress: vi.fn(),
	};
	return {
		service,
		options,
		coordinateBrowserUpload,
		execute,
		run: () => runBrowserUpload(service, "session", file, initial, options),
	};
}
afterEach(() => vi.useRealTimers());
describe("browser upload coordination resource", () => {
	it("accepts arbitrary plugin identities and reports an opaque observation", async () => {
		const f = setup();
		f.coordinateBrowserUpload
			.mockResolvedValueOnce(step(3))
			.mockResolvedValueOnce(step(8, true));
		await f.run();
		expect(f.coordinateBrowserUpload.mock.calls[1][1]).toEqual({
			request_id: "id-0",
			observation: {
				status: 298,
				headers: { "x-arbitrary": "yes" },
				body: "opaque",
				body_truncated: false,
			},
		});
		expect(f.execute.mock.calls.map((c) => c[0].request.body.start)).toEqual([
			0, 3,
		]);
	});
	it("queries before executing a restored session with no instruction", async () => {
		const initial = { ...step(2), instruction: null };
		const f = setup(initial);
		f.coordinateBrowserUpload.mockReset();
		f.coordinateBrowserUpload
			.mockResolvedValueOnce(step(2))
			.mockResolvedValueOnce(step(8, true));
		await f.run();
		expect(f.coordinateBrowserUpload.mock.calls[0][1]).toEqual({});
		expect(f.execute).toHaveBeenCalledOnce();
	});
	it("skips transfer when provider completion has already been confirmed", async () => {
		const f = setup(step(8, true));
		await f.run();
		expect(f.execute).not.toHaveBeenCalled();
	});
	it.each([
		{ ...step(), executor_version: 2 },
		{ ...step(), confirmed_offset: -1 },
		{ ...step(), confirmed_offset: 9 },
		{ ...step(), confirmed_offset: 0.5 },
		{ ...step(), confirmed_offset: Number.MAX_SAFE_INTEGER + 1 },
		{ ...step(7), provider_completed: true, instruction: null },
	])("rejects invalid initial executor state", async (initial) => {
		const f = setup(initial);
		await expect(f.run()).rejects.toThrow("invalid coordinated upload state");
		expect(f.execute).not.toHaveBeenCalled();
	});
	it("rejects a changed protocol identity or a regressing offset", async () => {
		for (const next of [
			{ ...step(4), protocol: { id: "changed", version: 99 } },
			step(1),
		]) {
			const f = setup(step(2));
			f.coordinateBrowserUpload.mockResolvedValue(next);
			await expect(f.run()).rejects.toThrow("invalid coordinated upload state");
		}
	});

	it("allows plugin control phases to advance without changing the file offset", async () => {
		const f = setup();
		for (let revision = 1; revision <= 4; revision++)
			f.coordinateBrowserUpload.mockResolvedValueOnce({
				...step(),
				progress_revision: revision,
			});
		f.coordinateBrowserUpload.mockResolvedValueOnce({
			...step(8, true),
			progress_revision: 5,
		});
		await f.run();
		expect(f.execute).toHaveBeenCalledTimes(5);
	});
	it("bounds stalled acknowledgements", async () => {
		vi.useFakeTimers();
		const f = setup();
		f.coordinateBrowserUpload.mockResolvedValue(step());
		const pending = f.run();
		const assertion = expect(pending).rejects.toThrow("made no progress");
		await vi.runAllTimersAsync();
		await assertion;
		expect(f.execute).toHaveBeenCalledTimes(3);
	});
	it("retries a lost backend response using the same report, without sending file bytes twice", async () => {
		vi.useFakeTimers();
		const f = setup();
		f.coordinateBrowserUpload
			.mockRejectedValueOnce(
				Object.assign(new Error("lost"), { retryable: true }),
			)
			.mockResolvedValueOnce(step(8, true));
		const pending = f.run();
		await vi.runAllTimersAsync();
		await pending;
		expect(f.execute).toHaveBeenCalledOnce();
		expect(f.coordinateBrowserUpload.mock.calls[1][1]).toBe(
			f.coordinateBrowserUpload.mock.calls[2][1],
		);
	});
	it("bounds backend retries and propagates terminal failures", async () => {
		vi.useFakeTimers();
		const f = setup();
		f.coordinateBrowserUpload.mockRejectedValue(
			Object.assign(new Error("unavailable"), { retryable: true }),
		);
		const pending = f.run();
		const assertion = expect(pending).rejects.toThrow("unavailable");
		await vi.runAllTimersAsync();
		await assertion;
		expect(f.coordinateBrowserUpload).toHaveBeenCalledTimes(4);
		expect(f.execute).toHaveBeenCalledOnce();
	});
	it("does not report observations or execute another instruction after cancellation", async () => {
		const f = setup();
		f.execute.mockImplementation(async () => {
			f.options.isAborted.mockReturnValue(true);
			return { status: 200, headers: {}, body: "", body_truncated: false };
		});
		await expect(f.run()).rejects.toMatchObject({ isAborted: true });
		expect(f.coordinateBrowserUpload).toHaveBeenCalledTimes(1);
	});
	it("reconciles a stale persisted instruction before sending any file bytes", async () => {
		const f = setup();
		f.coordinateBrowserUpload
			.mockReset()
			.mockResolvedValueOnce(step(4))
			.mockResolvedValueOnce(step(8, true));
		await f.run();
		expect(f.coordinateBrowserUpload.mock.calls[0][1]).toEqual({});
		expect(f.execute).toHaveBeenCalledOnce();
		expect(f.execute.mock.calls[0][0].request.body).toEqual({
			start: 4,
			end: 8,
		});
	});
	it("does not upload when reconciliation reveals an already completed provider session", async () => {
		const f = setup();
		f.coordinateBrowserUpload.mockReset().mockResolvedValue(step(8, true));
		await f.run();
		expect(f.execute).not.toHaveBeenCalled();
	});
	it("rejects a missing next instruction without resending file bytes", async () => {
		const f = setup();
		f.coordinateBrowserUpload.mockResolvedValue({
			...step(4),
			instruction: null,
		});
		await expect(f.run()).rejects.toThrow("missing browser upload instruction");
		expect(f.execute).toHaveBeenCalledOnce();
		expect(f.coordinateBrowserUpload).toHaveBeenCalledTimes(2);
	});
	it("bounds pathological partial acknowledgements within one planned fragment", async () => {
		const initial = step();
		if (initial.instruction?.request.body)
			initial.instruction.request.body.end = 100;
		const f = setup(initial, new File(["x".repeat(100)], "file"));
		for (let offset = 1; offset <= 33; offset++)
			f.coordinateBrowserUpload.mockResolvedValueOnce(step(offset));
		await expect(f.run()).rejects.toThrow("too many partial fragments");
		expect(f.execute).toHaveBeenCalledTimes(33);
	});
	it("executes empty-body control instructions without interpreting the plugin protocol", async () => {
		const initial = step();
		if (initial.instruction) initial.instruction.request.body = null;
		const f = setup(initial);
		f.coordinateBrowserUpload.mockResolvedValueOnce(step(8, true));
		await f.run();
		expect(f.execute.mock.calls[0][0].request.body).toBeNull();
	});
	it.each([
		{ ...step(), progress_revision: -1 },
		{ ...step(), progress_revision: 0.5 },
		{ ...step(), progress_revision: Number.MAX_SAFE_INTEGER + 1 },
		{ ...step(8, true), instruction: step().instruction },
		{ ...step(), protocol: { id: "plugin.unknown.to.browser", version: 100 } },
	])("rejects malformed or incompatible coordination state", async (next) => {
		const f = setup();
		f.coordinateBrowserUpload.mockResolvedValue(next);
		await expect(f.run()).rejects.toThrow("invalid coordinated upload state");
		expect(f.execute).toHaveBeenCalledOnce();
	});
	it("stops immediately when initial coordination fails permanently", async () => {
		const error = new Error("session expired");
		const f = setup();
		f.coordinateBrowserUpload.mockReset().mockRejectedValue(error);
		await expect(f.run()).rejects.toBe(error);
		expect(f.execute).not.toHaveBeenCalled();
		expect(f.coordinateBrowserUpload).toHaveBeenCalledOnce();
	});
	it("cancellation while awaiting coordination discards the returned instruction", async () => {
		const f = setup();
		f.coordinateBrowserUpload.mockReset().mockImplementation(async () => {
			f.options.isAborted.mockReturnValue(true);
			return step(4);
		});
		await expect(f.run()).rejects.toMatchObject({ isAborted: true });
		expect(f.execute).not.toHaveBeenCalled();
	});
});
