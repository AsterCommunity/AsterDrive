import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { FormFieldLabel } from "@/components/common/FormFieldLabel";

describe("FormFieldLabel", () => {
	it("shows a compact required marker only for required fields", () => {
		const { rerender } = render(
			<FormFieldLabel htmlFor="name" required>
				Name
			</FormFieldLabel>,
		);

		const marker = screen.getByText("Name");
		expect(marker).toHaveClass(
			"gap-0",
			"after:ml-0.5",
			"after:text-destructive",
			"after:content-['*']",
		);

		rerender(<FormFieldLabel htmlFor="name">Name</FormFieldLabel>);
		expect(screen.getByText("Name")).not.toHaveClass("after:content-['*']");
	});
});
