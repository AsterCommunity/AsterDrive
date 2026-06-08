import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
	FixedDialogFooter,
	InlineConfirm,
	ManagerDialogScrollableList,
	ManagerDialogShell,
} from "@/components/common/ManagerDialogShell";

vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({ children, open }: { children: React.ReactNode; open: boolean }) =>
		open ? <div data-testid="dialog">{children}</div> : null,
	DialogContent: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
	DialogDescription: ({ children }: { children: React.ReactNode }) => (
		<p>{children}</p>
	),
	DialogHeader: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
}));

describe("ManagerDialogShell", () => {
	it("keeps header, controls, scrollable body, and footer as distinct regions", () => {
		render(
			<ManagerDialogShell
				open
				onOpenChange={vi.fn()}
				title="Tags"
				description="Manage tags"
				controls={<label htmlFor="search">Search</label>}
				footer={<FixedDialogFooter>Footer</FixedDialogFooter>}
			>
				<ManagerDialogScrollableList>
					<div>List</div>
				</ManagerDialogScrollableList>
			</ManagerDialogShell>,
		);

		expect(screen.getByRole("heading", { name: "Tags" })).toBeInTheDocument();
		expect(screen.getByText("Manage tags")).toBeInTheDocument();
		expect(screen.getByText("Search")).toBeInTheDocument();
		expect(screen.getByText("List")).toBeInTheDocument();
		expect(screen.getByText("Footer")).toBeInTheDocument();
		expect(screen.getByText("List").parentElement).toHaveClass(
			"overflow-y-auto",
		);
	});

	it("does not render closed dialogs", () => {
		render(
			<ManagerDialogShell open={false} onOpenChange={vi.fn()} title="Hidden">
				Content
			</ManagerDialogShell>,
		);

		expect(screen.queryByText("Hidden")).not.toBeInTheDocument();
	});

	it("renders inline confirmations with destructive emphasis", () => {
		render(<InlineConfirm>Confirm delete</InlineConfirm>);

		expect(screen.getByText("Confirm delete")).toHaveClass(
			"border-destructive/25",
			"bg-destructive/5",
		);
	});
});
