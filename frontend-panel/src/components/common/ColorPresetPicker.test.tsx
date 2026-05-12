import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ColorPresetPicker } from "@/components/common/ColorPresetPicker";

const mockState = vi.hoisted(() => ({
	colorPreset: "#16a34a",
	setColorPreset: vi.fn(),
}));

vi.mock("@/stores/themeStore", () => ({
	COLOR_PRESETS: {
		blue: "#2563eb",
		green: "#16a34a",
		purple: "#9333ea",
		orange: "#f97316",
	},
	isColorPreset: (value: unknown) =>
		typeof value === "string" && /^#[0-9a-f]{6}$/i.test(value),
	useThemeStore: () => ({
		colorPreset: mockState.colorPreset,
		setColorPreset: mockState.setColorPreset,
	}),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: () => <span data-testid="check-icon" />,
}));

describe("ColorPresetPicker", () => {
	beforeEach(() => {
		mockState.colorPreset = "#16a34a";
		mockState.setColorPreset.mockReset();
	});

	it("highlights the selected preset and shows a check icon", () => {
		const { container } = render(<ColorPresetPicker />);
		const buttons = screen.getAllByRole("button");

		expect(buttons).toHaveLength(4);
		expect(screen.getByLabelText("Custom color")).toBeInTheDocument();
		expect(screen.getByTestId("check-icon")).toBeInTheDocument();
		expect(container.querySelector(".ring-2")).toBeInTheDocument();
	});

	it("switches to the clicked preset", () => {
		render(<ColorPresetPicker />);

		fireEvent.click(screen.getByRole("button", { name: "Orange" }));

		expect(mockState.setColorPreset).toHaveBeenCalledWith("#f97316");
	});

	it("accepts an arbitrary custom color from the color picker", () => {
		render(<ColorPresetPicker />);

		fireEvent.change(screen.getByLabelText("Custom color"), {
			target: { value: "#0f766e" },
		});

		expect(mockState.setColorPreset).toHaveBeenCalledWith("#0f766e");
	});

	it("does not expose the raw color value as visible text", () => {
		render(<ColorPresetPicker />);

		expect(screen.queryByText("#16a34a")).not.toBeInTheDocument();
	});
});
