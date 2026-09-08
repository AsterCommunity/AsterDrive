import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AdminDetailPageShell } from "@/components/layout/AdminDetailPageShell";

describe("AdminDetailPageShell", () => {
	it("keeps detail navigation, header actions, and the constrained content boundary together", () => {
		const onBack = vi.fn();
		const { container } = render(
			<AdminDetailPageShell
				actions={<button type="button">save</button>}
				backLabel="Back to records"
				contentClassName="flex-1 overflow-hidden"
				description="Record settings"
				onBack={onBack}
				title="Record"
			>
				<div>detail content</div>
			</AdminDetailPageShell>,
		);

		fireEvent.click(screen.getByRole("button", { name: /back to records/i }));

		expect(onBack).toHaveBeenCalledTimes(1);
		expect(screen.getByRole("heading", { name: "Record" })).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "save" })).toBeInTheDocument();
		expect(screen.getByText("detail content").parentElement).toHaveClass(
			"min-h-0",
			"min-w-0",
			"flex-1",
			"overflow-hidden",
		);
		expect(container.querySelectorAll(".slide-in-from-top-1")).toHaveLength(2);
	});
});
