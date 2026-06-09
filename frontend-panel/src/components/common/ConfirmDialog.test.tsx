import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => `translated:${key}`,
	}),
}));

vi.mock("@/components/ui/alert-dialog", () => ({
	AlertDialog: ({
		children,
		open,
	}: {
		children: React.ReactNode;
		open: boolean;
	}) => (
		<div data-testid="dialog" data-open={String(open)}>
			{children}
		</div>
	),
	AlertDialogContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	AlertDialogHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	AlertDialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
	AlertDialogDescription: ({ children }: { children: React.ReactNode }) => (
		<p>{children}</p>
	),
	AlertDialogFooter: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	AlertDialogCancel: ({ children }: { children: React.ReactNode }) => (
		<button type="button">{children}</button>
	),
	AlertDialogAction: ({
		children,
		className,
		onClick,
	}: {
		children: React.ReactNode;
		className?: string;
		onClick?: () => void;
	}) => (
		<button
			type="button"
			data-testid="confirm-action"
			className={className}
			onClick={onClick}
		>
			{children}
		</button>
	),
}));

describe("ConfirmDialog", () => {
	it("renders title, description, and default translated actions", () => {
		render(
			<ConfirmDialog
				open
				onOpenChange={vi.fn()}
				title="Delete file"
				description="This cannot be undone."
				onConfirm={vi.fn()}
			/>,
		);

		expect(screen.getByTestId("dialog")).toHaveAttribute("data-open", "true");
		expect(screen.getByText("Delete file")).toBeInTheDocument();
		expect(screen.getByText("This cannot be undone.")).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "translated:cancel" }),
		).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "translated:confirm" }),
		).toBeInTheDocument();
	});

	it("closes first and then confirms with the destructive action class", () => {
		const onOpenChange = vi.fn();
		const onConfirm = vi.fn();

		render(
			<ConfirmDialog
				open
				onOpenChange={onOpenChange}
				title="Delete file"
				confirmLabel="Delete"
				onConfirm={onConfirm}
				variant="destructive"
			/>,
		);

		const action = screen.getByRole("button", { name: "Delete" });
		fireEvent.click(action);

		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(onConfirm).toHaveBeenCalledTimes(1);
		expect(screen.queryByText("translated:confirm")).not.toBeInTheDocument();
		expect(screen.getByTestId("confirm-action")).toHaveClass(
			"bg-destructive",
			"text-white",
		);
	});

	it("keeps the last open content while the close animation is mounted", () => {
		const { rerender } = render(
			<ConfirmDialog
				open
				onOpenChange={vi.fn()}
				title="Delete 5 items"
				description="Selected 5 items"
				confirmLabel="Delete"
				onConfirm={vi.fn()}
				variant="destructive"
			/>,
		);

		rerender(
			<ConfirmDialog
				open={false}
				onOpenChange={vi.fn()}
				title="Delete 0 items"
				description="Selected 0 items"
				confirmLabel="Confirm"
				onConfirm={vi.fn()}
				variant="default"
			/>,
		);

		expect(screen.getByText("Delete 5 items")).toBeInTheDocument();
		expect(screen.getByText("Selected 5 items")).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Delete" })).toBeInTheDocument();
		expect(screen.queryByText("Delete 0 items")).not.toBeInTheDocument();
		expect(screen.queryByText("Selected 0 items")).not.toBeInTheDocument();
	});

	it("updates retained content when reopened with new props", () => {
		const { rerender } = render(
			<ConfirmDialog
				open
				onOpenChange={vi.fn()}
				title="Delete 5 items"
				confirmLabel="Delete"
				onConfirm={vi.fn()}
			/>,
		);

		rerender(
			<ConfirmDialog
				open={false}
				onOpenChange={vi.fn()}
				title="Delete 0 items"
				confirmLabel="Delete"
				onConfirm={vi.fn()}
			/>,
		);
		rerender(
			<ConfirmDialog
				open
				onOpenChange={vi.fn()}
				title="Delete 3 items"
				confirmLabel="Confirm"
				onConfirm={vi.fn()}
			/>,
		);

		expect(screen.getByText("Delete 3 items")).toBeInTheDocument();
		expect(screen.getByRole("button", { name: "Confirm" })).toBeInTheDocument();
		expect(screen.queryByText("Delete 5 items")).not.toBeInTheDocument();
	});
});
