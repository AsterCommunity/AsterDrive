import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AnimatedTreeGroup } from "./AnimatedTreeGroup";

describe("AnimatedTreeGroup", () => {
	beforeEach(() => {
		vi.useRealTimers();
	});

	it("keeps tree row content visible while opening", async () => {
		const { container, rerender } = render(
			<AnimatedTreeGroup open={false}>
				<div>Child Folder</div>
			</AnimatedTreeGroup>,
		);

		expect(screen.queryByText("Child Folder")).not.toBeInTheDocument();

		rerender(
			<AnimatedTreeGroup open>
				<div>Child Folder</div>
			</AnimatedTreeGroup>,
		);

		const group = container.firstElementChild as HTMLElement | null;
		const content = group?.firstElementChild as HTMLElement | null;

		expect(screen.getByText("Child Folder")).toBeInTheDocument();
		expect(group?.style.transitionProperty).toBe("max-height");
		expect(content?.style.transitionProperty).toBe("transform");
		expect(group?.style.opacity).toBe("");
		expect(content?.style.opacity).toBe("");
	});

	it("unmounts children after the collapse motion finishes", async () => {
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

		await waitFor(() => {
			expect(screen.queryByText("Child Folder")).not.toBeInTheDocument();
		});
	});
});
