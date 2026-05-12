import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
	AdminSortableTableHead,
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
		...props
	}: {
		children: React.ReactNode;
		className?: string;
	} & React.TableHTMLAttributes<HTMLTableElement>) => (
		<table data-testid="table" className={className} {...props}>
			{children}
		</table>
	),
	TableBody: ({
		children,
		className,
		...props
	}: {
		children: React.ReactNode;
		className?: string;
	} & React.HTMLAttributes<HTMLTableSectionElement>) => (
		<tbody data-testid="table-body" className={className} {...props}>
			{children}
		</tbody>
	),
	TableCaption: ({
		children,
		className,
		...props
	}: {
		children: React.ReactNode;
		className?: string;
	} & React.HTMLAttributes<HTMLTableCaptionElement>) => (
		<caption data-testid="table-caption" className={className} {...props}>
			{children}
		</caption>
	),
	TableCell: ({
		children,
		className,
		...props
	}: {
		children: React.ReactNode;
		className?: string;
	} & React.TdHTMLAttributes<HTMLTableCellElement>) => (
		<td data-testid="table-cell" className={className} {...props}>
			{children}
		</td>
	),
	TableFooter: ({
		children,
		className,
		...props
	}: {
		children: React.ReactNode;
		className?: string;
	} & React.HTMLAttributes<HTMLTableSectionElement>) => (
		<tfoot data-testid="table-footer" className={className} {...props}>
			{children}
		</tfoot>
	),
	TableHead: ({
		children,
		className,
		...props
	}: {
		children: React.ReactNode;
		className?: string;
	} & React.ThHTMLAttributes<HTMLTableCellElement>) => (
		<th data-testid="table-head" className={className} {...props}>
			{children}
		</th>
	),
	TableHeader: ({
		children,
		className,
		...props
	}: {
		children: React.ReactNode;
		className?: string;
	} & React.HTMLAttributes<HTMLTableSectionElement>) => (
		<thead data-testid="table-header" className={className} {...props}>
			{children}
		</thead>
	),
	TableRow: ({
		children,
		className,
		...props
	}: {
		children: React.ReactNode;
		className?: string;
	} & React.HTMLAttributes<HTMLTableRowElement>) => (
		<tr data-testid="table-row" className={className} {...props}>
			{children}
		</tr>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		className,
		onClick,
		type = "button",
	}: {
		children: React.ReactNode;
		className?: string;
		onClick?: () => void;
		type?: "button" | "submit" | "reset";
	}) => (
		<button className={className} onClick={onClick} type={type}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span data-testid={name} />,
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

	it("renders sortable headers with aria-sort and toggles the next sort order", () => {
		const onSortChange = vi.fn();

		const { rerender } = render(
			<table>
				<thead>
					<tr>
						<AdminSortableTableHead
							sortKey="username"
							sortBy="created_at"
							sortOrder="desc"
							onSortChange={onSortChange}
						>
							Username
						</AdminSortableTableHead>
					</tr>
				</thead>
			</table>,
		);

		const head = screen.getByTestId("table-head");
		expect(head).toHaveAttribute("aria-sort", "none");

		fireEvent.click(screen.getByRole("button", { name: /username/i }));

		expect(onSortChange).toHaveBeenCalledWith("username", "asc");

		rerender(
			<table>
				<thead>
					<tr>
						<AdminSortableTableHead
							sortKey="username"
							sortBy="username"
							sortOrder="asc"
							onSortChange={onSortChange}
						>
							Username
						</AdminSortableTableHead>
					</tr>
				</thead>
			</table>,
		);

		expect(screen.getByTestId("table-head")).toHaveAttribute(
			"aria-sort",
			"ascending",
		);
		fireEvent.click(screen.getByRole("button", { name: /username/i }));

		expect(onSortChange).toHaveBeenLastCalledWith("username", "desc");
		expect(screen.getByTestId("SortAscending")).toBeInTheDocument();
	});
});
