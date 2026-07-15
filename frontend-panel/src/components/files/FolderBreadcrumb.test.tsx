import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FolderBreadcrumb } from "./FolderBreadcrumb";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("@/components/ui/breadcrumb", () => ({
	Breadcrumb: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	BreadcrumbEllipsis: () => <span>ellipsis</span>,
	BreadcrumbItem: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	BreadcrumbLink: ({ children, ...props }: React.ComponentProps<"button">) => (
		<button type="button" {...props}>
			{children}
		</button>
	),
	BreadcrumbList: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	BreadcrumbPage: ({ children }: { children: React.ReactNode }) => (
		<span>{children}</span>
	),
	BreadcrumbSeparator: () => <span>/</span>,
}));

vi.mock("@/components/ui/dropdown-menu", () => ({
	DropdownMenu: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DropdownMenuTrigger: ({ render }: { render: React.ReactNode }) => render,
	DropdownMenuContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DropdownMenuItem: ({
		children,
		...props
	}: React.ComponentProps<"button">) => (
		<button type="button" {...props}>
			{children}
		</button>
	),
}));

describe("FolderBreadcrumb", () => {
	it("preserves source indexes and drag targets for compact hidden folders", () => {
		const onDragLeave = vi.fn();
		const onDragOver = vi.fn();
		const onDrop = vi.fn(async () => {});
		const onNavigate = vi.fn();
		render(
			<FolderBreadcrumb
				compact
				items={[
					{ id: null, name: "Root" },
					{ id: 10, name: "Workspace" },
					{ id: 20, name: "Semester" },
					{ id: 30, name: "Current" },
				]}
				onDragLeave={onDragLeave}
				onDragOver={onDragOver}
				onDrop={onDrop}
				onNavigate={onNavigate}
			/>,
		);

		const workspace = screen.getByRole("button", { name: /Workspace/ });
		const semester = screen.getByRole("button", { name: /Semester/ });
		fireEvent.dragOver(workspace);
		fireEvent.dragLeave(workspace);
		fireEvent.drop(semester);
		fireEvent.click(workspace);

		expect(onDragOver).toHaveBeenCalledWith(expect.anything(), 1);
		expect(onDragLeave).toHaveBeenCalledTimes(1);
		expect(onDrop).toHaveBeenCalledWith(expect.anything(), 2, 20);
		expect(onNavigate).toHaveBeenCalledWith(10, "Workspace");
	});
});
