import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
	AdminTable,
	AdminTableBody,
	AdminTableCell,
	AdminTableHead,
	AdminTableHeader,
	AdminTableRow,
	AdminTableShell,
} from "@/components/common/AdminTable";

vi.mock("@/components/layout/AdminSurface", () => ({
	AdminSurface: ({
		children,
		className,
		padded,
	}: {
		children: React.ReactNode;
		className?: string;
		padded?: boolean;
	}) => (
		<div
			data-padded={String(Boolean(padded))}
			data-testid="admin-surface"
			className={className}
		>
			{children}
		</div>
	),
}));

vi.mock("@/components/ui/scroll-area", () => ({
	ScrollArea: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => (
		<div data-testid="scroll-area" className={className}>
			{children}
		</div>
	),
}));

vi.mock("@/components/ui/table", () => ({
	Table: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => (
		<table data-testid="table" className={className}>
			{children}
		</table>
	),
	TableBody: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => (
		<tbody data-testid="table-body" className={className}>
			{children}
		</tbody>
	),
	TableCaption: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => (
		<caption data-testid="table-caption" className={className}>
			{children}
		</caption>
	),
	TableCell: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => (
		<td data-testid="table-cell" className={className}>
			{children}
		</td>
	),
	TableFooter: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => (
		<tfoot data-testid="table-footer" className={className}>
			{children}
		</tfoot>
	),
	TableHead: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => (
		<th data-testid="table-head" className={className}>
			{children}
		</th>
	),
	TableHeader: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => (
		<thead data-testid="table-header" className={className}>
			{children}
		</thead>
	),
	TableRow: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => (
		<tr data-testid="table-row" className={className}>
			{children}
		</tr>
	),
}));

describe("AdminTable", () => {
	it("renders the shared admin table shell with surface and scroll classes", () => {
		render(
			<AdminTableShell
				className="custom-surface"
				padded
				scrollAreaClassName="custom-scroll"
			>
				<div>table content</div>
			</AdminTableShell>,
		);

		expect(screen.getByTestId("admin-surface")).toHaveAttribute(
			"data-padded",
			"true",
		);
		expect(screen.getByTestId("admin-surface")).toHaveClass(
			"rounded-lg",
			"min-h-0",
			"overflow-hidden",
			"custom-surface",
		);
		expect(screen.getByTestId("scroll-area")).toHaveClass(
			"min-h-0",
			"flex-1",
			"custom-scroll",
		);
		expect(screen.getByText("table content")).toBeInTheDocument();
	});

	it("applies compact data-table styling to table primitives", () => {
		render(
			<AdminTable className="custom-table">
				<AdminTableHeader className="custom-header">
					<AdminTableRow className="custom-row">
						<AdminTableHead className="custom-head">Name</AdminTableHead>
					</AdminTableRow>
				</AdminTableHeader>
				<AdminTableBody>
					<AdminTableRow>
						<AdminTableCell className="custom-cell">Alpha</AdminTableCell>
					</AdminTableRow>
				</AdminTableBody>
			</AdminTable>,
		);

		expect(screen.getByTestId("table")).toHaveClass("text-sm", "custom-table");
		expect(screen.getByTestId("table-header")).toHaveClass(
			"[&_tr]:border-border/70",
			"custom-header",
		);
		expect(screen.getAllByTestId("table-row")[0]).toHaveClass(
			"h-11",
			"border-border/60",
			"hover:bg-muted/35",
			"custom-row",
		);
		expect(screen.getByTestId("table-head")).toHaveClass(
			"h-9",
			"bg-muted/35",
			"text-[11px]",
			"font-semibold",
			"uppercase",
			"custom-head",
		);
		expect(screen.getByTestId("table-cell")).toHaveClass(
			"px-3",
			"py-2",
			"align-middle",
			"text-sm",
			"custom-cell",
		);
	});
});
