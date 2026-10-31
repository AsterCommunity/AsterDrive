import { act, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { UnsavedChangesGuard } from "@/components/files/preview/dialog/UnsavedChangesGuard";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => `translated:${key}`,
	}),
}));

function GuardHarness() {
	const [open, setOpen] = useState(true);

	return (
		<UnsavedChangesGuard
			open={open}
			onOpenChange={setOpen}
			onConfirm={() => setOpen(false)}
		/>
	);
}

describe("UnsavedChangesGuard", () => {
	afterEach(() => {
		vi.useRealTimers();
	});

	it("renders an inline discard guard without opening a nested dialog", () => {
		const onOpenChange = vi.fn();
		const onConfirm = vi.fn();

		render(
			<UnsavedChangesGuard
				open
				onOpenChange={onOpenChange}
				onConfirm={onConfirm}
			/>,
		);

		expect(screen.getByText("translated:are_you_sure")).toBeInTheDocument();
		expect(screen.getByTestId("unsaved-changes-guard")).toHaveClass(
			"animate-in",
			"motion-reduce:animate-none",
		);
		expect(
			screen.getByText("translated:files:unsaved_confirm_desc"),
		).toBeInTheDocument();
		expect(
			screen.getByRole("button", {
				name: "translated:files:discard_changes",
			}),
		).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "translated:cancel" }));
		fireEvent.click(
			screen.getByRole("button", {
				name: "translated:files:discard_changes",
			}),
		);

		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(onConfirm).toHaveBeenCalledTimes(1);
	});

	it("keeps the guard mounted until its actual exit animation completes", async () => {
		let finish = () => {};
		const finished = new Promise<void>((resolve) => {
			finish = resolve;
		});
		const animations = vi
			.spyOn(Element.prototype, "getAnimations")
			.mockReturnValue([{ finished } as Animation]);
		render(<GuardHarness />);

		fireEvent.click(screen.getByRole("button", { name: "translated:cancel" }));

		expect(screen.getByTestId("unsaved-changes-guard")).toHaveClass(
			"animate-out",
			"motion-reduce:animate-none",
		);

		await act(async () => {
			finish();
			await finished;
		});
		animations.mockRestore();

		expect(
			screen.queryByTestId("unsaved-changes-guard"),
		).not.toBeInTheDocument();
	});
});
