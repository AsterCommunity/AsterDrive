import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import {
	AlertDialog,
	AlertDialogCancel,
	AlertDialogContent,
	AlertDialogTitle,
} from "./alert-dialog";
import { Dialog, DialogClose, DialogContent, DialogTitle } from "./dialog";
import { DialogLifecycle } from "./dialog-lifecycle";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

function ConfirmHarness({ confirmed }: { confirmed: () => void }) {
	const [open, setOpen] = useState(true);
	return (
		<ConfirmDialog
			open={open}
			onOpenChange={setOpen}
			title="Remove selected file"
			description="Removal is permanent"
			onConfirm={confirmed}
		/>
	);
}

function FunctionChildrenHarness({ kind }: { kind: "dialog" | "alert" }) {
	const [open, setOpen] = useState(true);
	return (
		<DialogLifecycle open={open} target="Retained title" onOpenChange={setOpen}>
			{({ target, active, onOpenChange }) =>
				kind === "dialog" ? (
					<Dialog open={active} onOpenChange={onOpenChange}>
						{() => (
							<DialogContent>
								<DialogTitle>{target}</DialogTitle>
								<DialogClose>Dismiss</DialogClose>
							</DialogContent>
						)}
					</Dialog>
				) : (
					<AlertDialog open={active} onOpenChange={onOpenChange}>
						{() => (
							<AlertDialogContent>
								<AlertDialogTitle>{target}</AlertDialogTitle>
								<AlertDialogCancel>Dismiss</AlertDialogCancel>
							</AlertDialogContent>
						)}
					</AlertDialog>
				)
			}
		</DialogLifecycle>
	);
}

describe("Base UI lifecycle integration", () => {
	it("disposes real AlertDialog content after confirming", async () => {
		const confirmed = vi.fn();
		render(<ConfirmHarness confirmed={confirmed} />);
		await screen.findByRole("alertdialog", { name: "Remove selected file" });
		fireEvent.click(screen.getByRole("button", { name: "confirm" }));
		expect(confirmed).toHaveBeenCalledTimes(1);
		await waitFor(() =>
			expect(screen.queryByText("Remove selected file")).toBeNull(),
		);
	});

	it.each(["dialog", "alert"] as const)(
		"connects function children to the %s completion boundary",
		async (kind) => {
			render(<FunctionChildrenHarness kind={kind} />);
			await screen.findByRole(kind === "dialog" ? "dialog" : "alertdialog", {
				name: "Retained title",
			});
			fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
			await waitFor(() =>
				expect(screen.queryByText("Retained title")).toBeNull(),
			);
		},
	);
});
