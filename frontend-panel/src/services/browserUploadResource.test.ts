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
function setup(initial = step()) {
	const coordinateBrowserUpload = vi.fn();
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
		run: () =>
			runBrowserUpload(
				service,
				"session",
				new File(["abcdefgh"], "file"),
				initial,
				options,
			),
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
		expect(f.coordinateBrowserUpload.mock.calls[0][1]).toEqual({
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
		expect(f.coordinateBrowserUpload.mock.calls[0][1]).toBe(
			f.coordinateBrowserUpload.mock.calls[1][1],
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
		expect(f.coordinateBrowserUpload).toHaveBeenCalledTimes(3);
		expect(f.execute).toHaveBeenCalledOnce();
	});
	it("does not report observations or execute another instruction after cancellation", async () => {
		const f = setup();
		f.execute.mockImplementation(async () => {
			f.options.isAborted.mockReturnValue(true);
			return { status: 200, headers: {}, body: "", body_truncated: false };
		});
		await expect(f.run()).rejects.toMatchObject({ isAborted: true });
		expect(f.coordinateBrowserUpload).not.toHaveBeenCalled();
	});
});
