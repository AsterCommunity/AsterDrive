import { fireEvent, render, screen } from "@testing-library/react";
import { createRef } from "react";
import { describe, expect, it, vi } from "vitest";
import { Textarea } from "@/components/ui/textarea";

describe("Textarea", () => {
	it("forwards its ref, value, events and disabled state", () => {
		const ref = createRef<HTMLTextAreaElement>();
		const onChange = vi.fn();
		const { rerender } = render(
			<Textarea
				ref={ref}
				aria-label="description"
				defaultValue="old"
				onChange={onChange}
			/>,
		);
		const textarea = screen.getByLabelText("description");
		expect(ref.current).toBe(textarea);
		fireEvent.change(textarea, { target: { value: "new" } });
		expect(onChange).toHaveBeenCalledOnce();
		rerender(<Textarea aria-label="description" disabled />);
		expect(textarea).toBeDisabled();
	});

	it("shares inset focus and invalid indicators and accepts sizing classes", () => {
		render(
			<Textarea
				aria-label="description"
				aria-invalid="true"
				className="min-h-32"
			/>,
		);
		const textarea = screen.getByLabelText("description");
		expect(textarea).toHaveAttribute("data-slot", "textarea");
		expect(textarea).toHaveClass(
			"min-h-32",
			"focus-visible:inset-ring-3",
			"aria-invalid:inset-ring-3",
			"aria-invalid:focus-visible:inset-ring-destructive/50",
		);
		expect(textarea).not.toHaveClass(
			"focus-visible:ring-3",
			"aria-invalid:ring-3",
		);
	});
});
