import { render, screen } from "@testing-library/react";
import { createRef } from "react";
import { describe, expect, it } from "vitest";
import { Textarea } from "@/components/ui/textarea";

describe("Textarea", () => {
	it("forwards its ref and keeps the shared text-control styling", () => {
		const ref = createRef<HTMLTextAreaElement>();

		render(
			<Textarea
				aria-label="styled-textarea"
				className="custom-textarea"
				ref={ref}
			/>,
		);

		const textarea = screen.getByLabelText("styled-textarea");

		expect(ref.current).toBe(textarea);
		expect(textarea).toHaveAttribute("data-slot", "textarea");
		expect(textarea).toHaveAttribute("data-theme-surface", "control");
		expect(textarea).toHaveClass(
			"min-h-20",
			"resize-y",
			"rounded-lg",
			"focus-visible:inset-ring-3",
			"focus-visible:inset-ring-ring/30",
			"aria-invalid:inset-ring-3",
			"aria-invalid:inset-ring-destructive/20",
			"aria-invalid:focus-visible:inset-ring-destructive/45",
			"forced-colors:focus-visible:-outline-offset-2",
			"forced-colors:focus-visible:outline-2",
			"custom-textarea",
		);
		expect(textarea).not.toHaveClass(
			"focus-visible:ring-3",
			"aria-invalid:ring-3",
		);
	});
});
