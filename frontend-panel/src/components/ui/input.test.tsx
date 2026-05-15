import { fireEvent, render, screen } from "@testing-library/react";
import { createRef, useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Input } from "@/components/ui/input";

function ControlledInput({ type = "text" }: { type?: string }) {
	const [value, setValue] = useState("abcdef");

	return (
		<Input
			aria-label="controlled-input"
			type={type}
			value={value}
			onChange={(event) => setValue(event.currentTarget.value)}
		/>
	);
}

describe("Input", () => {
	it("forwards object and callback refs to the underlying input", () => {
		const objectRef = createRef<HTMLInputElement>();
		const callbackRef = vi.fn();

		render(
			<>
				<Input aria-label="object-ref" ref={objectRef} />
				<Input aria-label="callback-ref" ref={callbackRef} />
			</>,
		);

		expect(objectRef.current).toBe(screen.getByLabelText("object-ref"));
		expect(callbackRef).toHaveBeenCalledWith(
			screen.getByLabelText("callback-ref"),
		);
	});

	it("restores a text selection after controlled value changes", () => {
		render(<ControlledInput />);

		const input = screen.getByLabelText("controlled-input") as HTMLInputElement;
		input.focus();
		input.setSelectionRange(2, 4, "forward");

		fireEvent.change(input, {
			target: {
				selectionDirection: "forward",
				selectionEnd: 5,
				selectionStart: 3,
				value: "abcXdef",
			},
		});

		expect(input).toHaveValue("abcXdef");
		expect(input.selectionStart).toBe(3);
		expect(input.selectionEnd).toBe(5);
		expect(input.selectionDirection).toBe("forward");
	});

	it("captures selection from select, keyup, and mouseup but clears it on blur", () => {
		const onBlur = vi.fn();
		const onKeyUp = vi.fn();
		const onMouseUp = vi.fn();
		const onSelect = vi.fn();

		render(
			<Input
				aria-label="selection-events"
				defaultValue="abcdef"
				onBlur={onBlur}
				onKeyUp={onKeyUp}
				onMouseUp={onMouseUp}
				onSelect={onSelect}
			/>,
		);

		const input = screen.getByLabelText("selection-events") as HTMLInputElement;
		input.focus();
		input.setSelectionRange(1, 3);

		fireEvent.select(input);
		fireEvent.keyUp(input);
		fireEvent.mouseUp(input);
		fireEvent.blur(input);

		expect(onSelect).toHaveBeenCalled();
		expect(onKeyUp).toHaveBeenCalled();
		expect(onMouseUp).toHaveBeenCalled();
		expect(onBlur).toHaveBeenCalled();
	});

	it("does not try to restore text selection for unsupported input types", () => {
		render(<ControlledInput type="number" />);

		const input = screen.getByLabelText("controlled-input") as HTMLInputElement;
		input.focus();

		fireEvent.change(input, {
			target: {
				value: "123",
			},
		});

		expect(input).toHaveValue(123);
	});

	it("keeps the default design-system classes and accepts custom classes", () => {
		render(<Input aria-label="styled-input" className="custom-input" />);

		const input = screen.getByLabelText("styled-input");

		expect(input).toHaveAttribute("data-slot", "input");
		expect(input).toHaveAttribute("data-theme-surface", "control");
		expect(input).toHaveClass("h-8", "rounded-lg", "custom-input");
	});
});
