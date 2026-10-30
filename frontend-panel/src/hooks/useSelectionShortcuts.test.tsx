import { fireEvent, render, renderHook } from "@testing-library/react";
import { useLayoutEffect } from "react";
import { describe, expect, it, vi } from "vitest";
import {
	shouldIgnoreKeyboardTarget,
	useSelectionShortcuts,
} from "@/hooks/useSelectionShortcuts";

function SelectionOnCommit({
	ids,
	selectItems,
	clearSelection,
}: {
	ids: number[];
	selectItems: (ids: number[]) => void;
	clearSelection: () => void;
}) {
	useSelectionShortcuts({ selectAll: () => selectItems(ids), clearSelection });
	useLayoutEffect(() => {
		if (ids.length > 0) {
			fireEvent.keyDown(document, { metaKey: true, key: "a" });
		}
	}, [ids]);
	return <div>{ids.join(",")}</div>;
}

describe("shouldIgnoreKeyboardTarget", () => {
	it("ignores editable form controls and contenteditable targets", () => {
		const input = document.createElement("input");
		const textarea = document.createElement("textarea");
		const select = document.createElement("select");
		const editable = document.createElement("div");
		Object.defineProperty(editable, "isContentEditable", {
			configurable: true,
			value: true,
		});

		expect(shouldIgnoreKeyboardTarget(input)).toBe(true);
		expect(shouldIgnoreKeyboardTarget(textarea)).toBe(true);
		expect(shouldIgnoreKeyboardTarget(select)).toBe(true);
		expect(shouldIgnoreKeyboardTarget(editable)).toBe(true);
		expect(shouldIgnoreKeyboardTarget(document.body)).toBeFalsy();
		expect(shouldIgnoreKeyboardTarget(null)).toBeFalsy();
	});
});

describe("useSelectionShortcuts", () => {
	it("uses the committed selection before passive effects run", () => {
		const selectItems = vi.fn();
		const clearSelection = vi.fn();
		const { rerender } = render(
			<SelectionOnCommit
				ids={[]}
				selectItems={selectItems}
				clearSelection={clearSelection}
			/>,
		);
		rerender(
			<SelectionOnCommit
				ids={[1, 2]}
				selectItems={selectItems}
				clearSelection={clearSelection}
			/>,
		);
		expect(selectItems).toHaveBeenCalledExactlyOnceWith([1, 2]);
	});

	it("handles select-all and clear-selection shortcuts", () => {
		const clearSelection = vi.fn();
		const selectAll = vi.fn();

		renderHook(() =>
			useSelectionShortcuts({
				selectAll,
				clearSelection,
			}),
		);

		fireEvent.keyDown(document, {
			ctrlKey: true,
			key: "a",
		});
		fireEvent.keyDown(document, {
			key: "Escape",
		});

		expect(selectAll).toHaveBeenCalledTimes(1);
		expect(clearSelection).toHaveBeenCalledTimes(1);
	});

	it("ignores shortcuts from input fields or when disabled", () => {
		const clearSelection = vi.fn();
		const selectAll = vi.fn();
		const input = document.createElement("input");
		document.body.appendChild(input);

		const { unmount } = renderHook(() =>
			useSelectionShortcuts({
				selectAll,
				clearSelection,
			}),
		);

		fireEvent.keyDown(input, {
			metaKey: true,
			key: "a",
		});
		fireEvent.keyDown(input, {
			key: "Escape",
		});

		expect(selectAll).not.toHaveBeenCalled();
		expect(clearSelection).not.toHaveBeenCalled();

		unmount();

		renderHook(() =>
			useSelectionShortcuts({
				selectAll,
				clearSelection,
				enabled: false,
			}),
		);

		fireEvent.keyDown(document, {
			ctrlKey: true,
			key: "a",
		});

		expect(selectAll).not.toHaveBeenCalled();
		input.remove();
	});

	it("uses the latest callbacks after async list state changes", () => {
		const initialSelectAll = vi.fn();
		const nextSelectAll = vi.fn();
		const clearSelection = vi.fn();

		const { rerender } = renderHook(
			({ selectAll }) =>
				useSelectionShortcuts({
					selectAll,
					clearSelection,
				}),
			{ initialProps: { selectAll: initialSelectAll } },
		);

		rerender({ selectAll: nextSelectAll });

		fireEvent.keyDown(document, {
			metaKey: true,
			key: "a",
		});

		expect(initialSelectAll).not.toHaveBeenCalled();
		expect(nextSelectAll).toHaveBeenCalledTimes(1);
	});

	it("ignores shortcuts while the browser is composing IME text", () => {
		const clearSelection = vi.fn();
		const selectAll = vi.fn();

		renderHook(() =>
			useSelectionShortcuts({
				selectAll,
				clearSelection,
			}),
		);

		fireEvent.keyDown(document, {
			ctrlKey: true,
			key: "a",
			keyCode: 229,
		});
		fireEvent.keyDown(document, {
			key: "Escape",
			keyCode: 229,
		});

		expect(selectAll).not.toHaveBeenCalled();
		expect(clearSelection).not.toHaveBeenCalled();
	});
});
