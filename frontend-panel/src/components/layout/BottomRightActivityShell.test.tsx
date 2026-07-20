import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
	BottomRightActivityPortal,
	BottomRightActivityShell,
} from "@/components/layout/BottomRightActivityShell";
import { BOTTOM_RIGHT_ACTIVITY_SHELL_HEIGHT_PROPERTY } from "@/lib/constants";

describe("BottomRightActivityShell", () => {
	afterEach(() => {
		vi.restoreAllMocks();
		document.documentElement.style.removeProperty(
			BOTTOM_RIGHT_ACTIVITY_SHELL_HEIGHT_PROPERTY,
		);
	});

	it("stacks activity surfaces inside one fixed bottom-right shell", async () => {
		vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
			bottom: 164,
			height: 164,
			left: 0,
			right: 448,
			top: 0,
			width: 448,
			x: 0,
			y: 0,
			toJSON: () => ({}),
		});
		const view = render(
			<BottomRightActivityShell>
				<BottomRightActivityPortal>
					<div>upload-surface</div>
				</BottomRightActivityPortal>
				<BottomRightActivityPortal>
					<div>download-surface</div>
				</BottomRightActivityPortal>
			</BottomRightActivityShell>,
		);

		const shell = screen.getByTestId("bottom-right-activity-shell");
		expect(shell).toHaveClass(
			"fixed",
			"right-4",
			"bottom-4",
			"flex-col",
			"w-[28rem]",
			"overflow-hidden",
			"rounded-lg",
		);
		expect((await screen.findByText("upload-surface")).parentElement).toBe(
			shell,
		);
		expect(screen.getByText("download-surface").parentElement).toBe(shell);
		expect(
			document.documentElement.style.getPropertyValue(
				BOTTOM_RIGHT_ACTIVITY_SHELL_HEIGHT_PROPERTY,
			),
		).toBe("164px");

		view.unmount();
		expect(
			document.documentElement.style.getPropertyValue(
				BOTTOM_RIGHT_ACTIVITY_SHELL_HEIGHT_PROPERTY,
			),
		).toBe("");
	});
});
