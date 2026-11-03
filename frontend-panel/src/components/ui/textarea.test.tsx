import { render, screen } from "@testing-library/react";
import { createRef } from "react";
import { describe, expect, it } from "vitest";
import { Textarea } from "@/components/ui/textarea";

describe("Textarea", () => {
	it("forwards props and refs to the underlying textarea", () => {
		const ref = createRef<HTMLTextAreaElement>();

		render(
			<Textarea
				ref={ref}
				aria-label="description"
				rows={4}
				defaultValue="AsterDrive"
			/>,
		);

		const textarea = screen.getByLabelText("description");
		expect(ref.current).toBe(textarea);
		expect(textarea).toHaveAttribute("rows", "4");
		expect(textarea).toHaveValue("AsterDrive");
	});

	it("uses an inset focus ring and accepts custom classes", () => {
		render(<Textarea aria-label="styled-textarea" className="min-h-32" />);

		const textarea = screen.getByLabelText("styled-textarea");
		expect(textarea).toHaveAttribute("data-slot", "textarea");
		expect(textarea).toHaveAttribute("data-theme-surface", "control");
		expect(textarea).toHaveClass(
			"rounded-lg",
			"focus-visible:ring-inset",
			"aria-invalid:ring-inset",
			"min-h-32",
		);
	});
});
