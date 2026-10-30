import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DialogLifecycle } from "@/components/ui/dialog-lifecycle";
import { deferred } from "@/test/deferred";
import { useVideoPreviewResource } from "./useVideoPreviewResource";

describe("video preview lifecycle", () => {
	it.each(["success", "failure"])(
		"ignores stream-session %s during exit",
		async (outcome) => {
			const pending = deferred<{ path: string }>();
			const createMediaStreamSession = vi.fn(() => pending.promise);
			let open = true;
			const wrapper = ({ children }: { children: React.ReactNode }) => (
				<DialogLifecycle open={open} target={null}>
					{() => children}
				</DialogLifecycle>
			);
			const { result, rerender } = renderHook(
				() =>
					useVideoPreviewResource({
						fileName: "clip.mp4",
						resource: "/video",
						createMediaStreamSession: createMediaStreamSession as never,
					}),
				{ wrapper },
			);
			await waitFor(() =>
				expect(createMediaStreamSession).toHaveBeenCalledTimes(1),
			);
			open = false;
			rerender();
			await act(async () => {
				if (outcome === "success") pending.resolve({ path: "/late" });
				else pending.reject(new Error("late video"));
				await pending.promise.catch(() => {});
			});
			expect(result.current.resolvedPath).toBeNull();
			expect(result.current.error).toBeNull();
		},
	);
});
