import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useArchivePreviewState } from "@/components/files/preview/viewers/archive/useArchivePreviewState";
import { DialogLifecycle } from "@/components/ui/dialog-lifecycle";
import { ApiPendingError } from "@/services/http";
import { deferred } from "@/test/deferred";
import type { ArchivePreviewManifest } from "@/types/api";

const manifest: ArchivePreviewManifest = {
	schema_version: 2,
	format: "zip",
	source_blob_id: 10,
	source_hash: "hash",
	generated_at: "2026-01-02T03:04:05Z",
	entry_count: 1,
	file_count: 1,
	directory_count: 0,
	total_uncompressed_size: 5,
	truncated: false,
	extract_compatibility: {
		supported: true,
		reason: null,
	},
	entries: [
		{
			path: "readme.txt",
			name: "readme.txt",
			parent: null,
			kind: "file",
			size: 5,
			compressed_size: 5,
			modified_at: null,
		},
	],
};

describe("useArchivePreviewState", () => {
	it("marks encoding switches as pending loads and clears stale UI state", async () => {
		let resolveSecondLoad: (value: ArchivePreviewManifest) => void = () => {};
		const loadManifest = vi
			.fn<
				(options?: {
					signal?: AbortSignal;
					filenameEncoding?: "auto" | "utf8" | "gb18030" | "cp437";
				}) => Promise<ArchivePreviewManifest>
			>()
			.mockResolvedValueOnce(manifest)
			.mockImplementationOnce(
				() =>
					new Promise<ArchivePreviewManifest>((resolve) => {
						resolveSecondLoad = resolve;
					}),
			);
		const { result } = renderHook(() => useArchivePreviewState(loadManifest));

		await waitFor(() => {
			expect(result.current[0].manifest).toBe(manifest);
		});
		act(() => {
			result.current[1]({ type: "queryChanged", query: "readme" });
			result.current[1]({
				type: "currentFolderChanged",
				currentFolder: "docs",
			});
			result.current[1]({
				type: "filenameEncodingChanged",
				filenameEncoding: "gb18030",
			});
		});

		expect(result.current[0]).toMatchObject({
			manifest: null,
			query: "",
			currentFolder: null,
			loading: true,
			pending: true,
			error: null,
			filenameEncoding: "gb18030",
		});
		await waitFor(() => {
			expect(loadManifest).toHaveBeenLastCalledWith(
				expect.objectContaining({ filenameEncoding: "gb18030" }),
			);
		});

		await act(async () => {
			resolveSecondLoad(manifest);
		});

		expect(result.current[0]).toMatchObject({
			manifest,
			loading: false,
			pending: false,
			error: null,
		});
	});

	it("clears failed state when encoding changes", async () => {
		const loadManifest = vi
			.fn<
				(options?: {
					signal?: AbortSignal;
					filenameEncoding?: "auto" | "utf8" | "gb18030" | "cp437";
				}) => Promise<ArchivePreviewManifest>
			>()
			.mockRejectedValueOnce(new Error("boom"))
			.mockResolvedValueOnce(manifest);
		const { result } = renderHook(() => useArchivePreviewState(loadManifest));

		await waitFor(() => {
			expect(result.current[0].error).toBe("generic");
		});
		act(() => {
			result.current[1]({
				type: "filenameEncodingChanged",
				filenameEncoding: "cp437",
			});
		});

		expect(result.current[0]).toMatchObject({
			error: null,
			loading: true,
			pending: true,
			manifest: null,
		});
		await waitFor(() => {
			expect(result.current[0].manifest).toBe(manifest);
		});
	});

	it("keeps pending true across polling retries", async () => {
		vi.useFakeTimers();
		let resolveSecondLoad: (value: ArchivePreviewManifest) => void = () => {};
		const loadManifest = vi
			.fn<
				(options?: {
					signal?: AbortSignal;
					filenameEncoding?: "auto" | "utf8" | "gb18030" | "cp437";
				}) => Promise<ArchivePreviewManifest>
			>()
			.mockRejectedValueOnce(new ApiPendingError("pending", 1))
			.mockImplementationOnce(
				() =>
					new Promise<ArchivePreviewManifest>((resolve) => {
						resolveSecondLoad = resolve;
					}),
			);

		try {
			const { result } = renderHook(() => useArchivePreviewState(loadManifest));

			await act(async () => {
				await Promise.resolve();
			});
			expect(result.current[0]).toMatchObject({
				loading: true,
				pending: true,
			});

			await act(async () => {
				vi.advanceTimersByTime(1000);
				await Promise.resolve();
				await Promise.resolve();
			});

			expect(result.current[0].pending).toBe(true);
			expect(loadManifest).toHaveBeenCalledTimes(2);
			await act(async () => {
				resolveSecondLoad(manifest);
				await Promise.resolve();
				await Promise.resolve();
			});
			expect(result.current[0].manifest).toBe(manifest);
			expect(result.current[0].pending).toBe(false);
		} finally {
			vi.useRealTimers();
		}
	});
	it("aborts an exiting archive request without clearing a previously rendered manifest", async () => {
		const pending = deferred<ArchivePreviewManifest>();
		const load = vi
			.fn()
			.mockResolvedValueOnce(manifest)
			.mockReturnValueOnce(pending.promise);
		let open = true;
		const wrapper = ({ children }: { children: React.ReactNode }) => (
			<DialogLifecycle open={open} target={null}>
				{() => children}
			</DialogLifecycle>
		);
		const { result, rerender } = renderHook(
			() => useArchivePreviewState(load),
			{ wrapper },
		);
		await waitFor(() => expect(result.current[0].manifest).toEqual(manifest));
		act(() => result.current[1]({ type: "retryRequested" }));
		await waitFor(() => expect(load).toHaveBeenCalledTimes(2));
		const signal = load.mock.calls[1][0].signal as AbortSignal;
		open = false;
		rerender();
		expect(signal.aborted).toBe(true);
		await act(async () => {
			pending.resolve({ ...manifest, entry_count: 99 });
			await pending.promise;
		});
		expect(result.current[0].manifest?.entry_count).toBe(1);
	});
});
