import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { AnimatedTreeGroup } from "./AnimatedTreeGroup";

describe("AnimatedTreeGroup", () => {
	it("keeps tree row content mounted and visible while opening", () => {
		const { container, rerender } = render(
			<AnimatedTreeGroup open={false}>
				<div>Child Folder</div>
			</AnimatedTreeGroup>,
		);

		const closedGroup = container.firstElementChild as HTMLElement | null;
		const closedContent = closedGroup?.firstElementChild as HTMLElement | null;

		expect(screen.getByText("Child Folder")).toBeInTheDocument();
		expect(closedGroup).toHaveAttribute("aria-hidden", "true");
		expect(closedGroup).toHaveAttribute("inert");
		expect(closedGroup).toHaveClass("grid-rows-[0fr]");
		expect(closedContent).toHaveClass("scale-y-0");

		rerender(
			<AnimatedTreeGroup open>
				<div>Child Folder</div>
			</AnimatedTreeGroup>,
		);

		const group = container.firstElementChild as HTMLElement | null;
		const content = group?.firstElementChild as HTMLElement | null;

		expect(screen.getByText("Child Folder")).toBeInTheDocument();
		expect(group).toHaveAttribute("aria-hidden", "false");
		expect(group).not.toHaveAttribute("inert");
		expect(group).toHaveClass("grid-rows-[1fr]");
		expect(content).toHaveClass("scale-y-100");
	});

	it("hides the collapsed subtree from accessibility and interaction", () => {
		const { rerender } = render(
			<AnimatedTreeGroup open>
				<div>Child Folder</div>
			</AnimatedTreeGroup>,
		);

		expect(screen.getByText("Child Folder")).toBeInTheDocument();

		rerender(
			<AnimatedTreeGroup open={false}>
				<div>Child Folder</div>
			</AnimatedTreeGroup>,
		);

		expect(screen.getByText("Child Folder")).toBeInTheDocument();
		expect(
			screen.getByText("Child Folder").closest("[aria-hidden]"),
		).toHaveAttribute("aria-hidden", "true");
	});
});
