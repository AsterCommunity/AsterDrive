import { act, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { useDesktopInfoPanelMount } from "./useDesktopInfoPanelMount";

function Presence({
	open,
	desktop = true,
	onExit,
}: {
	open: boolean;
	desktop?: boolean;
	onExit: (open: boolean) => void;
}) {
	const { desktopMounted, desktopVisible, panelRef } = useDesktopInfoPanelMount(
		open,
		desktop,
		onExit,
	);
	return desktopMounted ? (
		<div ref={panelRef} data-testid="panel" data-visible={desktopVisible} />
	) : null;
}
it("disposes only after the actual exit animations finish, without a timer", async () => {
	let finish: () => void = () => {};
	const finished = new Promise<void>((resolve) => {
		finish = resolve;
	});
	const spy = vi
		.spyOn(Element.prototype, "getAnimations")
		.mockReturnValue([{ finished } as Animation]);
	const exit = vi.fn();
	const view = render(<Presence open onExit={exit} />);
	view.rerender(<Presence open={false} onExit={exit} />);
	expect(screen.getByTestId("panel")).toHaveAttribute("data-visible", "false");
	expect(exit).not.toHaveBeenCalled();
	await act(async () => {
		finish();
		await finished;
	});
	expect(screen.queryByTestId("panel")).not.toBeInTheDocument();
	expect(exit).toHaveBeenCalledExactlyOnceWith(false);
	spy.mockRestore();
});
it("ignores an old animation completion after reopening and switching content", async () => {
	let finish: () => void = () => {};
	const finished = new Promise<void>((resolve) => {
		finish = resolve;
	});
	const spy = vi
		.spyOn(Element.prototype, "getAnimations")
		.mockReturnValue([{ finished } as Animation]);
	const exit = vi.fn();
	const view = render(<Presence open onExit={exit} />);
	const panel = screen.getByTestId("panel");
	view.rerender(<Presence open={false} onExit={exit} />);
	view.rerender(<Presence open onExit={exit} />);
	await act(async () => {
		finish();
		await finished;
	});
	expect(screen.getByTestId("panel")).toBe(panel);
	expect(panel).toHaveAttribute("data-visible", "true");
	expect(exit).not.toHaveBeenCalled();
	spy.mockRestore();
});
it("finishes immediately when reduced motion produces no animations", async () => {
	const exit = vi.fn();
	const spy = vi.spyOn(Element.prototype, "getAnimations").mockReturnValue([]);
	const view = render(<Presence open onExit={exit} />);
	view.rerender(<Presence open={false} onExit={exit} />);
	await waitFor(() =>
		expect(screen.queryByTestId("panel")).not.toBeInTheDocument(),
	);
	expect(exit).toHaveBeenCalledExactlyOnceWith(false);
	spy.mockRestore();
});
it("does not deliver stale completion after unmount", async () => {
	let finish: () => void = () => {};
	const finished = new Promise<void>((resolve) => {
		finish = resolve;
	});
	const spy = vi
		.spyOn(Element.prototype, "getAnimations")
		.mockReturnValue([{ finished } as Animation]);
	const exit = vi.fn();
	const view = render(<Presence open onExit={exit} />);
	view.rerender(<Presence open={false} onExit={exit} />);
	view.unmount();
	await act(async () => {
		finish();
		await finished;
	});
	expect(exit).not.toHaveBeenCalled();
	spy.mockRestore();
});
