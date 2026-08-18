import { renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useFileListKeyboardNavigation } from "@/components/files/useFileListKeyboardNavigation";

const mockState = vi.hoisted(() => ({
	store: {
		moveSelectionBy: vi.fn(),
		extendSelectionBy: vi.fn(),
		selectionCount: vi.fn(() => 1),
		selectionFocus: { type: "file", id: 7 } as {
			type: "file" | "folder";
			id: number;
		} | null,
	},
}));

vi.mock("@/stores/fileStore", () => ({
	useFileStore: {
		getState: () => mockState.store,
	},
}));

function pressKey(key: string, init: KeyboardEventInit = {}, target?: Element) {
	const event = new KeyboardEvent("keydown", {
		key,
		bubbles: true,
		cancelable: true,
		...init,
	});
	(target ?? document.body).dispatchEvent(event);
	return event;
}

describe("useFileListKeyboardNavigation", () => {
	beforeEach(() => {
		mockState.store.moveSelectionBy.mockReset();
		mockState.store.extendSelectionBy.mockReset();
		mockState.store.selectionCount.mockReset().mockReturnValue(1);
		mockState.store.selectionFocus = { type: "file", id: 7 };
		document.body.innerHTML = "";
	});

	it("moves the selection by one step horizontally and by columnCount vertically", () => {
		renderHook(() => useFileListKeyboardNavigation({ columnCount: 6 }));

		pressKey("ArrowRight");
		expect(mockState.store.moveSelectionBy).toHaveBeenLastCalledWith(1);

		pressKey("ArrowLeft");
		expect(mockState.store.moveSelectionBy).toHaveBeenLastCalledWith(-1);

		pressKey("ArrowDown");
		expect(mockState.store.moveSelectionBy).toHaveBeenLastCalledWith(6);

		pressKey("ArrowUp");
		expect(mockState.store.moveSelectionBy).toHaveBeenLastCalledWith(-6);
	});

	it("extends the selection on Shift+arrow without moving the anchor", () => {
		renderHook(() => useFileListKeyboardNavigation({ columnCount: 6 }));

		pressKey("ArrowDown", { shiftKey: true });
		expect(mockState.store.extendSelectionBy).toHaveBeenCalledWith(6);
		expect(mockState.store.moveSelectionBy).not.toHaveBeenCalled();
	});

	it("ignores horizontal arrows in list mode", () => {
		renderHook(() =>
			useFileListKeyboardNavigation({ columnCount: 1, horizontal: false }),
		);

		pressKey("ArrowLeft");
		pressKey("ArrowRight");
		expect(mockState.store.moveSelectionBy).not.toHaveBeenCalled();

		pressKey("ArrowDown");
		expect(mockState.store.moveSelectionBy).toHaveBeenCalledWith(1);
	});

	it("prevents default scrolling on handled arrows", () => {
		renderHook(() => useFileListKeyboardNavigation({ columnCount: 6 }));
		const event = pressKey("ArrowDown");
		expect(event.defaultPrevented).toBe(true);
	});

	it("ignores keydowns from inputs, dialogs and mod combinations", () => {
		renderHook(() => useFileListKeyboardNavigation({ columnCount: 6 }));

		const input = document.createElement("input");
		document.body.appendChild(input);
		pressKey("ArrowDown", {}, input);

		const dialog = document.createElement("div");
		dialog.setAttribute("role", "dialog");
		const inner = document.createElement("div");
		dialog.appendChild(inner);
		document.body.appendChild(dialog);
		pressKey("ArrowDown", {}, inner);

		pressKey("ArrowDown", { metaKey: true });
		pressKey("ArrowDown", { ctrlKey: true });
		pressKey("ArrowDown", { altKey: true });

		expect(mockState.store.moveSelectionBy).not.toHaveBeenCalled();
	});

	it("scrolls the focused item into view after moving", () => {
		const scrollToItem = vi.fn();
		renderHook(() =>
			useFileListKeyboardNavigation({ columnCount: 6, scrollToItem }),
		);

		pressKey("ArrowDown");
		expect(scrollToItem).toHaveBeenCalledWith({ type: "file", id: 7 });
	});

	it("opens the focused item on Enter only when exactly one item is selected", () => {
		const onOpenFocused = vi.fn();
		renderHook(() =>
			useFileListKeyboardNavigation({ columnCount: 6, onOpenFocused }),
		);

		pressKey("Enter");
		expect(onOpenFocused).toHaveBeenCalledWith({ type: "file", id: 7 });

		onOpenFocused.mockReset();
		mockState.store.selectionCount.mockReturnValue(3);
		pressKey("Enter");
		expect(onOpenFocused).not.toHaveBeenCalled();
	});

	it("lets native click handle Enter on focusable card elements", () => {
		const onOpenFocused = vi.fn();
		renderHook(() =>
			useFileListKeyboardNavigation({ columnCount: 6, onOpenFocused }),
		);

		const card = document.createElement("div");
		card.setAttribute("role", "button");
		document.body.appendChild(card);
		pressKey("Enter", {}, card);

		expect(onOpenFocused).not.toHaveBeenCalled();
	});

	it("blurs a focused card on arrow navigation so Enter follows the selection", () => {
		// 复现场景：点击卡片留下 DOM 焦点，方向键把选择移走后，
		// Enter 不能再落到旧焦点卡片上。
		const card = document.createElement("div");
		card.setAttribute("data-file-list-item", "");
		card.tabIndex = 0;
		document.body.appendChild(card);
		card.focus();
		expect(document.activeElement).toBe(card);

		renderHook(() => useFileListKeyboardNavigation({ columnCount: 6 }));
		pressKey("ArrowDown");

		expect(document.activeElement).not.toBe(card);
		expect(mockState.store.moveSelectionBy).toHaveBeenCalledWith(6);
	});

	it("does nothing when disabled", () => {
		renderHook(() =>
			useFileListKeyboardNavigation({ columnCount: 6, enabled: false }),
		);

		pressKey("ArrowDown");
		expect(mockState.store.moveSelectionBy).not.toHaveBeenCalled();
	});
});
