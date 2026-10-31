import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DialogLifecycle } from "@/components/ui/dialog-lifecycle";
import { useFileEditorSession } from "@/hooks/useFileEditorSession";
import { deferred } from "@/test/deferred";

const mockState = vi.hoisted(() => ({
	setFileLock: vi.fn(),
	updateContent: vi.fn(),
	handleApiError: vi.fn(),
	toastSuccess: vi.fn(),
	toastError: vi.fn(),
}));

vi.mock("@/services/fileService", () => ({
	fileService: {
		setFileLock: mockState.setFileLock,
		updateContent: mockState.updateContent,
	},
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: mockState.handleApiError,
}));

vi.mock("sonner", () => ({
	toast: {
		success: mockState.toastSuccess,
		error: mockState.toastError,
	},
}));

function createOptions(
	overrides: Partial<Parameters<typeof useFileEditorSession>[0]> = {},
) {
	return {
		fileId: 7,
		initialContent: "initial",
		etag: '"etag-1"',
		messages: {
			saved: "Saved",
			editedByOthers: "Edited elsewhere",
		},
		...overrides,
	};
}

describe("useFileEditorSession", () => {
	beforeEach(() => {
		mockState.setFileLock.mockReset();
		mockState.updateContent.mockReset();
		mockState.handleApiError.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.toastError.mockReset();
		mockState.setFileLock.mockResolvedValue(undefined);
		mockState.updateContent.mockResolvedValue({});
	});

	it("starts editing after locking and tracks dirty state", async () => {
		const { result } = renderHook(() => useFileEditorSession(createOptions()));

		await act(async () => {
			await result.current.startEditing();
		});

		expect(mockState.setFileLock).toHaveBeenCalledWith(7, true);
		expect(result.current.editing).toBe(true);
		expect(result.current.dirty).toBe(false);

		act(() => {
			result.current.setEditContent("changed");
		});

		expect(result.current.editContent).toBe("changed");
		expect(result.current.dirty).toBe(true);
	});

	it("syncs new initial content only while not editing", async () => {
		const { result, rerender } = renderHook(
			(props: Parameters<typeof useFileEditorSession>[0]) =>
				useFileEditorSession(props),
			{
				initialProps: createOptions(),
			},
		);

		rerender(createOptions({ initialContent: "server update" }));
		expect(result.current.editContent).toBe("server update");

		await act(async () => {
			await result.current.startEditing();
		});
		act(() => {
			result.current.setEditContent("local draft");
		});

		rerender(createOptions({ initialContent: "new server update" }));
		expect(result.current.editContent).toBe("local draft");
	});

	it("reports lock acquisition failures through handleApiError", async () => {
		const error = new Error("lock failed");
		mockState.setFileLock.mockRejectedValueOnce(error);
		const { result } = renderHook(() => useFileEditorSession(createOptions()));

		await act(async () => {
			await result.current.startEditing();
		});

		expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		expect(result.current.editing).toBe(false);
	});

	it("cancels editing, restores initial content, and ignores unlock failures", async () => {
		mockState.setFileLock
			.mockResolvedValueOnce(undefined)
			.mockRejectedValueOnce(new Error("unlock failed"));
		const { result } = renderHook(() => useFileEditorSession(createOptions()));

		await act(async () => {
			await result.current.startEditing();
		});
		act(() => {
			result.current.setEditContent("draft");
		});

		await act(async () => {
			await result.current.cancelEditing();
		});

		expect(result.current.editing).toBe(false);
		expect(result.current.editContent).toBe("initial");
		expect(mockState.setFileLock).toHaveBeenNthCalledWith(2, 7, false);
	});

	it("saves content, shows success feedback, and unlocks afterwards", async () => {
		const onSaved = vi.fn().mockResolvedValue(undefined);
		const { result } = renderHook(() =>
			useFileEditorSession(createOptions({ onSaved })),
		);

		await act(async () => {
			await result.current.startEditing();
		});
		act(() => {
			result.current.setEditContent("saved content");
		});

		await act(async () => {
			await result.current.save();
		});

		expect(mockState.updateContent).toHaveBeenCalledWith(
			7,
			"saved content",
			'"etag-1"',
		);
		expect(mockState.toastSuccess).toHaveBeenCalledWith("Saved");
		expect(onSaved).toHaveBeenCalledTimes(1);
		expect(mockState.setFileLock).toHaveBeenNthCalledWith(2, 7, false);
		expect(result.current.editing).toBe(false);
		expect(result.current.saving).toBe(false);
	});

	it("reports edit conflicts without leaving edit mode", async () => {
		const onConflict = vi.fn();
		mockState.updateContent.mockRejectedValueOnce({ status: 412 });
		const { result } = renderHook(() =>
			useFileEditorSession(createOptions({ onConflict })),
		);

		await act(async () => {
			await result.current.startEditing();
		});
		act(() => {
			result.current.setEditContent("conflicting content");
		});

		await act(async () => {
			await result.current.save();
		});

		expect(mockState.toastError).toHaveBeenCalledWith("Edited elsewhere");
		expect(onConflict).toHaveBeenCalledTimes(1);
		expect(mockState.handleApiError).not.toHaveBeenCalled();
		expect(result.current.editing).toBe(true);
		expect(result.current.saving).toBe(false);
	});

	it("releases the file lock when unmounted during editing", async () => {
		const { result, unmount } = renderHook(() =>
			useFileEditorSession(createOptions()),
		);

		await act(async () => {
			await result.current.startEditing();
		});

		unmount();

		await waitFor(() => {
			expect(mockState.setFileLock).toHaveBeenNthCalledWith(2, 7, false);
		});
	});
	it.each(["success", "failure"])(
		"releases late lock acquisition without touching a replacement session: %s",
		async (outcome) => {
			const pending = deferred<void>();
			mockState.setFileLock.mockReturnValueOnce(pending.promise);
			const { result, unmount } = renderHook(() =>
				useFileEditorSession(createOptions()),
			);
			let work: Promise<void> | undefined;
			act(() => {
				work = result.current.startEditing();
			});
			unmount();
			await act(async () => {
				if (outcome === "success") pending.resolve();
				else pending.reject(new Error("late lock failure"));
				await work;
			});
			if (outcome === "success")
				expect(mockState.setFileLock).toHaveBeenLastCalledWith(7, false);
			expect(mockState.handleApiError).not.toHaveBeenCalled();
		},
	);

	it.each(["success", "failure", "active-failure"])(
		"separates save side effects and live UI: %s",
		async (outcome) => {
			const pending = deferred<void>();
			mockState.updateContent.mockReturnValueOnce(pending.promise);
			const onSaved = vi.fn();
			const { result, unmount } = renderHook(() =>
				useFileEditorSession(createOptions({ onSaved })),
			);
			let work: Promise<void> | undefined;
			act(() => {
				work = result.current.save();
			});
			if (outcome !== "active-failure") unmount();
			const error = new Error("save failure");
			await act(async () => {
				if (outcome === "success") pending.resolve();
				else pending.reject(error);
				await work;
			});
			if (outcome === "success") {
				expect(onSaved).toHaveBeenCalledTimes(1);
				expect(mockState.setFileLock).toHaveBeenCalledWith(7, false);
			} else expect(onSaved).not.toHaveBeenCalled();
			if (outcome === "active-failure")
				expect(mockState.handleApiError).toHaveBeenCalledWith(error);
			else expect(mockState.handleApiError).not.toHaveBeenCalled();
		},
	);

	it("releases the edit lock and beforeunload subscription as exit starts", async () => {
		// Keep the wrapper identity stable; a close must not masquerade as an unmount.
		let open = true;
		const wrapper = ({ children }: { children: React.ReactNode }) => (
			<DialogLifecycle open={open} target={null}>
				{() => children}
			</DialogLifecycle>
		);
		const { result, rerender } = renderHook(
			() => useFileEditorSession(createOptions()),
			{ wrapper },
		);
		await act(async () => {
			await result.current.startEditing();
		});
		const event = new Event("beforeunload", { cancelable: true });
		window.dispatchEvent(event);
		expect(event.defaultPrevented).toBe(true);
		open = false;
		rerender();
		await waitFor(() =>
			expect(mockState.setFileLock).toHaveBeenCalledWith(7, false),
		);
		const after = new Event("beforeunload", { cancelable: true });
		window.dispatchEvent(after);
		expect(after.defaultPrevented).toBe(false);
	});
});
