import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { StrictMode, useEffect, useState } from "react";
import { describe, expect, it, vi } from "vitest";
import {
	DialogCompletionBoundary,
	DialogLifecycle,
	useDialogActivity,
	useDialogExitComplete,
	useDialogMotion,
} from "./dialog-lifecycle";

function deferred() {
	let resolve = () => {};
	let reject = (_reason: Error) => {};
	const promise = new Promise<void>((yes, no) => {
		resolve = yes;
		reject = no;
	});
	return { promise, resolve, reject };
}

function Content({
	label,
	work,
	committed,
	error,
	onClose,
}: {
	label: string;
	work?: Promise<void>;
	committed?: () => void;
	error?: () => void;
	onClose?: () => void;
}) {
	const [draft, setDraft] = useState(label);
	const current = useDialogActivity();
	return (
		<section data-testid="frame">
			<span>{label}</span>
			<input
				aria-label="draft"
				value={draft}
				onChange={(e) => setDraft(e.target.value)}
			/>
			<button
				type="button"
				onClick={async () => {
					try {
						await work;
						committed?.();
						if (!current()) return;
						setDraft("saved");
						onClose?.();
					} catch {
						if (current()) error?.();
					}
				}}
			>
				mutate
			</button>
		</section>
	);
}

function Nested({
	capture,
}: {
	capture: (value: ReturnType<typeof useDialogExitComplete>) => void;
}) {
	capture(useDialogExitComplete());
	return null;
}

function RejectedCloseHarness({
	allowed,
	requested,
	work,
}: {
	allowed: () => boolean;
	requested: (open: boolean) => void;
	work: Promise<void>;
}) {
	const [open, setOpen] = useState(true);
	return (
		<DialogLifecycle
			open={open}
			target="resource"
			onOpenChange={(next) => {
				requested(next);
				if (allowed()) setOpen(next);
			}}
		>
			{({ target, onOpenChange }) => (
				<Content
					label={target}
					work={work}
					onClose={() => onOpenChange(false)}
				/>
			)}
		</DialogLifecycle>
	);
}

describe("DialogLifecycle", () => {
	it("restores activity when a parent rejects close, then accepts a later close", async () => {
		const work = deferred();
		let allowed = false;
		const requested = vi.fn();
		// The content's mutation completion checks useDialogActivity before requesting close.
		render(
			<RejectedCloseHarness
				allowed={() => allowed}
				requested={requested}
				work={work.promise}
			/>,
		);
		fireEvent.click(screen.getByRole("button", { name: "mutate" }));
		await act(async () => {
			work.resolve();
			await work.promise;
		});
		expect(requested).toHaveBeenCalledTimes(1);
		allowed = true;
		fireEvent.click(screen.getByRole("button", { name: "mutate" }));
		await waitFor(() => expect(requested).toHaveBeenCalledTimes(2));
	});

	it("never revalidates callbacks when a target changes away and back", async () => {
		const work = deferred();
		const close = vi.fn();
		const view = (target: string) => (
			<DialogLifecycle
				open
				target={target}
				identity={target}
				onOpenChange={close}
			>
				{({ target, onOpenChange }) => (
					<Content
						label={target}
						work={work.promise}
						onClose={() => onOpenChange(false)}
					/>
				)}
			</DialogLifecycle>
		);
		const { rerender } = render(view("first"));
		const frame = screen.getByTestId("frame");
		fireEvent.click(screen.getByRole("button", { name: "mutate" }));
		rerender(view("second"));
		rerender(view("first"));
		await act(async () => {
			work.resolve();
			await work.promise;
		});
		expect(screen.getByTestId("frame")).toBe(frame);
		expect(screen.getByLabelText("draft")).not.toHaveValue("saved");
		expect(close).not.toHaveBeenCalled();
	});

	it("stops nested work when the parent begins exiting", async () => {
		const work = deferred();
		const close = vi.fn();
		const view = (open: boolean) => (
			<DialogLifecycle open={open} target={null}>
				{() => (
					<DialogLifecycle open target={null} onOpenChange={close}>
						{({ active, onOpenChange }) => (
							<div data-active={active} data-testid="nested">
								<Content
									label="nested"
									work={work.promise}
									onClose={() => onOpenChange(false)}
								/>
							</div>
						)}
					</DialogLifecycle>
				)}
			</DialogLifecycle>
		);
		const { rerender } = render(view(true));
		fireEvent.click(screen.getByRole("button", { name: "mutate" }));
		rerender(view(false));
		expect(screen.getByTestId("nested")).toHaveAttribute(
			"data-active",
			"false",
		);
		await act(async () => {
			work.resolve();
			await work.promise;
		});
		expect(close).not.toHaveBeenCalled();
		expect(screen.getByLabelText("draft")).not.toHaveValue("saved");
	});

	it("owns drafts through exit and starts a fresh content instance on reopen", () => {
		let complete = (_open: boolean) => {};
		const view = (open: boolean, label: string) => (
			<DialogLifecycle open={open} target={label}>
				{({ target, complete: finish }) => {
					complete = finish;
					return <Content label={target} />;
				}}
			</DialogLifecycle>
		);
		const { rerender } = render(view(false, "unused"));
		expect(screen.queryByTestId("frame")).toBeNull();
		rerender(view(true, "first"));
		fireEvent.change(screen.getByLabelText("draft"), {
			target: { value: "edited" },
		});
		rerender(view(false, "cleared by parent"));
		expect(screen.getByText("first")).toBeInTheDocument();
		expect(screen.getByLabelText("draft")).toHaveValue("edited");
		act(() => complete(false));
		expect(screen.queryByTestId("frame")).toBeNull();
		rerender(view(true, "next"));
		expect(screen.getByLabelText("draft")).toHaveValue("next");
	});

	it("rejects an old exit notification after reopening and after a second close", () => {
		let complete = (_open: boolean) => {};
		const view = (open: boolean) => (
			<DialogLifecycle open={open} target="content">
				{({ complete: finish }) => {
					complete = finish;
					return <Content label="content" />;
				}}
			</DialogLifecycle>
		);
		const { rerender } = render(view(true));
		rerender(view(false));
		const oldComplete = complete;
		rerender(view(true));
		act(() => oldComplete(false));
		expect(screen.getByTestId("frame")).toBeInTheDocument();
		rerender(view(false));
		act(() => oldComplete(false));
		expect(screen.getByTestId("frame")).toBeInTheDocument();
		act(() => complete(false));
		expect(screen.queryByTestId("frame")).toBeNull();
	});

	it.each(["reopen", "switch", "unmount"])(
		"preserves committed side effects while rejecting stale UI after %s",
		async (transition) => {
			const work = deferred();
			const committed = vi.fn();
			const close = vi.fn();
			const view = (open: boolean, label = "first") => (
				<StrictMode>
					<DialogLifecycle
						open={open}
						target={label}
						identity={label}
						onOpenChange={close}
					>
						{({ target, onOpenChange }) => (
							<Content
								label={target}
								work={work.promise}
								committed={committed}
								onClose={() => onOpenChange(false)}
							/>
						)}
					</DialogLifecycle>
				</StrictMode>
			);
			const { rerender, unmount } = render(view(true));
			fireEvent.click(screen.getByRole("button", { name: "mutate" }));
			if (transition === "unmount") unmount();
			else if (transition === "switch") rerender(view(true, "second"));
			else {
				rerender(view(false));
				rerender(view(true, "second"));
			}
			await act(async () => {
				work.resolve();
				await work.promise;
			});
			expect(committed).toHaveBeenCalledTimes(1);
			expect(close).not.toHaveBeenCalled();
			if (transition !== "unmount")
				expect(screen.getByLabelText("draft")).not.toHaveValue("saved");
		},
	);

	it("suppresses stale errors while a new session is active", async () => {
		const work = deferred();
		const error = vi.fn();
		const view = (open: boolean) => (
			<DialogLifecycle open={open} target={null}>
				{() => <Content label="content" work={work.promise} error={error} />}
			</DialogLifecycle>
		);
		const { rerender } = render(view(true));
		fireEvent.click(screen.getByRole("button", { name: "mutate" }));
		rerender(view(false));
		rerender(view(true));
		await act(async () => {
			work.reject(new Error("old request"));
			await work.promise.catch(() => {});
		});
		expect(error).not.toHaveBeenCalled();
	});

	it("does not let a nested primitive complete its parent's presentation", () => {
		let innerComplete: ReturnType<typeof useDialogExitComplete>;
		const view = (open: boolean) => (
			<DialogLifecycle open={open} target={null}>
				{() => (
					<DialogCompletionBoundary>
						<Nested
							capture={(value) => {
								innerComplete = value;
							}}
						/>
						<Content label="parent" />
					</DialogCompletionBoundary>
				)}
			</DialogLifecycle>
		);
		const { rerender } = render(view(true));
		rerender(view(false));
		expect(innerComplete).toBeUndefined();
		expect(screen.getByText("parent")).toBeInTheDocument();
	});
});

function MotionContent({
	open,
	disposed,
}: {
	open: boolean;
	disposed: () => void;
}) {
	const { ref, visible } = useDialogMotion(open);
	useEffect(() => disposed, [disposed]);
	return <div ref={ref} data-testid="motion" data-visible={visible} />;
}

describe("dialog motion", () => {
	it.each(["finished", "cancelled", "none"])(
		"disposes using %s browser animations",
		async (outcome) => {
			const work = deferred();
			vi.spyOn(Element.prototype, "getAnimations").mockReturnValue(
				outcome === "none" ? [] : [{ finished: work.promise } as Animation],
			);
			const disposed = vi.fn();
			const view = (open: boolean) => (
				<DialogLifecycle open={open} target={null}>
					{({ active }) => <MotionContent open={active} disposed={disposed} />}
				</DialogLifecycle>
			);
			const { rerender } = render(view(true));
			rerender(view(false));
			expect(screen.getByTestId("motion")).toHaveAttribute(
				"data-visible",
				"false",
			);
			if (outcome !== "none") {
				expect(disposed).not.toHaveBeenCalled();
				await act(async () => {
					if (outcome === "cancelled") work.reject(new Error("cancel"));
					else work.resolve();
					await work.promise.catch(() => {});
				});
			}
			await waitFor(() => expect(screen.queryByTestId("motion")).toBeNull());
			expect(disposed).toHaveBeenCalledTimes(1);
		},
	);
});
