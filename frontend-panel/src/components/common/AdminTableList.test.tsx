import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AdminTableList } from "@/components/common/AdminTableList";

vi.mock("@/components/common/EmptyState", () => ({
	EmptyState: ({
		action,
		description,
		icon,
		title,
	}: {
		action?: React.ReactNode;
		description?: string;
		icon?: React.ReactNode;
		title: string;
	}) => (
		<div>
			<div>{title}</div>
			<div>{description}</div>
			<div>{icon}</div>
			<div>{action}</div>
		</div>
	),
}));

vi.mock("@/components/common/SkeletonTable", () => ({
	SkeletonTable: ({
		columns,
		rows,
		frameless,
	}: {
		columns: number;
		rows: number;
		frameless?: boolean;
	}) => (
		<div>{`skeleton:${columns}:${rows}${frameless ? ":frameless" : ""}`}</div>
	),
}));

vi.mock("@/components/layout/AdminSurface", () => ({
	AdminSurface: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => (
		<div data-testid="admin-surface" className={className}>
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
		frameless,
	}: {
		children: React.ReactNode;
		frameless?: boolean;
	}) => (
		<div data-testid="table" data-frameless={frameless ? "true" : "false"}>
			{children}
		</div>
	),
	TableBody: ({ children }: { children: React.ReactNode }) => (
		<div data-testid="table-body">{children}</div>
	),
}));

describe("AdminTableList", () => {
	it("renders a skeleton table while loading", () => {
		render(
			<AdminTableList
				loading
				items={[]}
				columns={4}
				emptyTitle="empty"
				headerRow={<div>header</div>}
				renderRow={() => <div>row</div>}
			/>,
		);

		expect(screen.getByTestId("admin-surface")).toBeInTheDocument();
		expect(screen.getByText("skeleton:4:5")).toBeInTheDocument();
	});

	it("renders the empty state when there are no items", () => {
		render(
			<AdminTableList
				loading={false}
				items={[]}
				columns={3}
				emptyIcon={<span>icon</span>}
				emptyTitle="No accounts"
				emptyDescription="Create one first"
				headerRow={<div>header</div>}
				renderRow={() => <div>row</div>}
			/>,
		);

		expect(screen.getByText("No accounts")).toBeInTheDocument();
		expect(screen.getByText("Create one first")).toBeInTheDocument();
		expect(screen.getByText("icon")).toBeInTheDocument();
	});

	it("renders toolbar, filtered empty state, and pagination slots", () => {
		render(
			<AdminTableList
				loading={false}
				items={[]}
				columns={3}
				emptyTitle="No accounts"
				emptyDescription="Create one first"
				emptyAction={<button type="button">Create</button>}
				filtered
				filteredEmptyTitle="No matching accounts"
				filteredEmptyDescription="Clear filters to see all accounts"
				filteredEmptyAction={<button type="button">Clear filters</button>}
				headerRow={<div>header</div>}
				pagination={<div>pagination</div>}
				renderRow={() => <div>row</div>}
				toolbar={<div>filters</div>}
			/>,
		);

		expect(screen.getByText("filters")).toBeInTheDocument();
		expect(screen.getByText("No matching accounts")).toBeInTheDocument();
		expect(
			screen.getByText("Clear filters to see all accounts"),
		).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "Clear filters" }),
		).toBeInTheDocument();
		expect(screen.queryByRole("button", { name: "Create" })).toBeNull();
		expect(screen.getByText("pagination")).toBeInTheDocument();
	});

	it("renders the table surface, header, and each row", () => {
		const renderRow = vi.fn((item: { id: number; name: string }) => (
			<div key={item.id}>{`row:${item.name}`}</div>
		));

		render(
			<AdminTableList
				loading={false}
				items={[
					{ id: 1, name: "Alpha" },
					{ id: 2, name: "Beta" },
				]}
				columns={2}
				rows={7}
				emptyTitle="empty"
				headerRow={<div>header-row</div>}
				renderRow={renderRow}
			/>,
		);

		expect(screen.getByTestId("admin-surface")).toBeInTheDocument();
		expect(screen.getByTestId("scroll-area")).toHaveClass("min-h-0", "flex-1");
		expect(screen.getByTestId("table")).toBeInTheDocument();
		expect(screen.getByText("header-row")).toBeInTheDocument();
		expect(screen.getByText("row:Alpha")).toBeInTheDocument();
		expect(screen.getByText("row:Beta")).toBeInTheDocument();
		expect(renderRow).toHaveBeenCalledTimes(2);
		expect(renderRow).toHaveBeenNthCalledWith(
			1,
			{ id: 1, name: "Alpha" },
			0,
			expect.any(Array),
		);
		expect(renderRow).toHaveBeenNthCalledWith(
			2,
			{ id: 2, name: "Beta" },
			1,
			expect.any(Array),
		);
	});

	it("renders frameless loading, empty, toolbar, and table without AdminSurface", () => {
		const { rerender } = render(
			<AdminTableList
				frameless
				loading
				items={[]}
				columns={4}
				rows={6}
				emptyTitle="empty"
				headerRow={<div>header</div>}
				renderRow={() => <div>row</div>}
			/>,
		);

		expect(screen.getByText("skeleton:4:6:frameless")).toBeInTheDocument();
		expect(screen.queryByTestId("admin-surface")).toBeNull();

		rerender(
			<AdminTableList
				frameless
				loading={false}
				items={[]}
				columns={4}
				emptyTitle="nothing here"
				headerRow={<div>header</div>}
				renderRow={() => <div>row</div>}
				toolbar={<div>filters</div>}
			/>,
		);

		expect(screen.getByText("nothing here")).toBeInTheDocument();
		expect(screen.getByText("filters")).toBeInTheDocument();
		expect(screen.queryByTestId("admin-surface")).toBeNull();

		rerender(
			<AdminTableList
				frameless
				loading={false}
				items={[{ id: 1 }]}
				columns={4}
				emptyTitle="empty"
				headerRow={<div>header</div>}
				renderRow={() => <div>row</div>}
			/>,
		);

		expect(screen.getByTestId("table")).toHaveAttribute(
			"data-frameless",
			"true",
		);
		expect(screen.queryByTestId("admin-surface")).toBeNull();
	});
});
