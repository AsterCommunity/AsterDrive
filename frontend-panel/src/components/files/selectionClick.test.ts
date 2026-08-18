import { beforeEach, describe, expect, it, vi } from "vitest";
import { applySelectionModifiers } from "@/components/files/selectionClick";

const mockState = vi.hoisted(() => ({
	toggleFileSelection: vi.fn(),
	toggleFolderSelection: vi.fn(),
	selectRangeTo: vi.fn(),
}));

vi.mock("@/stores/fileStore", () => ({
	useFileStore: {
		getState: () => mockState,
	},
}));

const noModifiers = { metaKey: false, ctrlKey: false, shiftKey: false };

describe("applySelectionModifiers", () => {
	beforeEach(() => {
		mockState.toggleFileSelection.mockReset();
		mockState.toggleFolderSelection.mockReset();
		mockState.selectRangeTo.mockReset();
	});

	it("returns false without modifiers so the caller proceeds to open", () => {
		expect(applySelectionModifiers(noModifiers, { type: "file", id: 1 })).toBe(
			false,
		);
		expect(mockState.toggleFileSelection).not.toHaveBeenCalled();
		expect(mockState.selectRangeTo).not.toHaveBeenCalled();
	});

	it.each([{ metaKey: true }, { ctrlKey: true }])(
		"toggles the item on Cmd/Ctrl+click (%o)",
		(modifier) => {
			expect(
				applySelectionModifiers(
					{ ...noModifiers, ...modifier },
					{ type: "file", id: 7 },
				),
			).toBe(true);
			expect(mockState.toggleFileSelection).toHaveBeenCalledWith(7);

			applySelectionModifiers(
				{ ...noModifiers, ...modifier },
				{ type: "folder", id: 3 },
			);
			expect(mockState.toggleFolderSelection).toHaveBeenCalledWith(3);
			expect(mockState.selectRangeTo).not.toHaveBeenCalled();
		},
	);

	it("range-selects on Shift+click", () => {
		expect(
			applySelectionModifiers(
				{ ...noModifiers, shiftKey: true },
				{ type: "folder", id: 5 },
			),
		).toBe(true);
		expect(mockState.selectRangeTo).toHaveBeenCalledWith("folder", 5);
		expect(mockState.toggleFileSelection).not.toHaveBeenCalled();
	});
});
