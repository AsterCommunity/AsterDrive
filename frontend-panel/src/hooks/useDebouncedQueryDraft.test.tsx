import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useDebouncedQueryDraft } from "@/hooks/useDebouncedQueryDraft";

afterEach(() => vi.useRealTimers());
describe("useDebouncedQueryDraft", () => {
	it("retains edits across unrelated filter replacements but cancels them on history navigation", () => {
		vi.useFakeTimers();
		const commit = vi.fn();
		const { result, rerender } = renderHook(
			({ sourceKey, preserveUnchanged }) =>
				useDebouncedQueryDraft({
					initial: "",
					sourceKey,
					preserveUnchanged,
					normalize: (value) => value,
					onCommit: commit,
				}),
			{ initialProps: { sourceKey: "1", preserveUnchanged: true } },
		);
		act(() => result.current.setDraft("pending"));
		rerender({ sourceKey: "2", preserveUnchanged: true });
		expect(result.current.draft).toBe("pending");
		act(() => vi.advanceTimersByTime(350));
		expect(commit).toHaveBeenCalledExactlyOnceWith("pending");
		rerender({ sourceKey: "3", preserveUnchanged: false });
		expect(result.current.draft).toBe("");
		act(() => vi.advanceTimersByTime(350));
		expect(commit).toHaveBeenCalledTimes(1);
	});
	it("renders raw input immediately and only commits the last rapid edit", () => {
		vi.useFakeTimers();
		const commit = vi.fn();
		const { result } = renderHook(() =>
			useDebouncedQueryDraft({
				initial: "",
				sourceKey: "first",
				normalize: (value) => value.trim(),
				onCommit: commit,
			}),
		);
		act(() => result.current.setDraft("a"));
		act(() => vi.advanceTimersByTime(200));
		act(() => result.current.setDraft("ab "));
		expect(result.current.draft).toBe("ab ");
		expect(commit).not.toHaveBeenCalled();
		act(() => vi.advanceTimersByTime(350));
		expect(commit).toHaveBeenCalledExactlyOnceWith("ab");
	});
	it("preserves raw text on its own URL commit but cancels pending input on navigation", () => {
		vi.useFakeTimers();
		const commit = vi.fn();
		const { result, rerender } = renderHook(
			({ initial, sourceKey }) =>
				useDebouncedQueryDraft({
					initial,
					sourceKey,
					normalize: (value) => value.trim(),
					onCommit: commit,
				}),
			{ initialProps: { initial: "", sourceKey: "1" } },
		);
		act(() => result.current.setDraft("hello "));
		act(() => vi.advanceTimersByTime(350));
		rerender({ initial: "hello", sourceKey: "2" });
		expect(result.current.draft).toBe("hello ");
		act(() => result.current.setDraft("stale"));
		rerender({ initial: "back", sourceKey: "3" });
		act(() => vi.advanceTimersByTime(500));
		expect(result.current.draft).toBe("back");
		expect(commit).toHaveBeenCalledTimes(1);
	});
	it("does not commit invalid input or edits after reversal, reset or unmount", () => {
		vi.useFakeTimers();
		const commit = vi.fn();
		const { result, unmount } = renderHook(() =>
			useDebouncedQueryDraft({
				initial: "",
				sourceKey: "1",
				normalize: (value) => (value === "invalid" ? null : value),
				onCommit: commit,
			}),
		);
		act(() => result.current.setDraft("invalid"));
		act(() => vi.advanceTimersByTime(500));
		act(() => result.current.setDraft("temporary"));
		act(() => result.current.setDraft(""));
		act(() => vi.advanceTimersByTime(500));
		act(() => result.current.setDraft("reset me"));
		act(() => result.current.resetDraft());
		act(() => vi.advanceTimersByTime(500));
		act(() => result.current.setDraft("unmounted"));
		unmount();
		act(() => vi.advanceTimersByTime(500));
		expect(commit).not.toHaveBeenCalled();
	});
});
