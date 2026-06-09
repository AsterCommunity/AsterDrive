import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AdminFilterToolbar } from "@/components/common/AdminFilterToolbar";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

describe("AdminFilterToolbar", () => {
	it("keeps filter controls collapsed until the filter button is clicked", () => {
		render(
			<AdminFilterToolbar activeFilterCount={0}>
				<input aria-label="keyword" />
			</AdminFilterToolbar>,
		);

		expect(screen.queryByLabelText("keyword")).not.toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: /show_filters/ }));

		expect(
			screen.getByRole("button", { name: /hide_filters/ }),
		).toHaveAttribute("aria-expanded", "true");
		expect(screen.getByLabelText("keyword")).toBeInTheDocument();
	});

	it("shows active filter state and clears filters without expanding controls", () => {
		const onResetFilters = vi.fn();

		render(
			<AdminFilterToolbar activeFilterCount={2} onResetFilters={onResetFilters}>
				<input aria-label="keyword" />
			</AdminFilterToolbar>,
		);

		expect(screen.queryByLabelText("keyword")).not.toBeInTheDocument();
		expect(screen.getByText("filters_active")).toBeInTheDocument();
		expect(screen.getByText("2")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "clear_filters" }));

		expect(onResetFilters).toHaveBeenCalledTimes(1);
	});

	it("can render controls expanded by default", () => {
		render(
			<AdminFilterToolbar activeFilterCount={0} defaultOpen>
				<input aria-label="keyword" />
			</AdminFilterToolbar>,
		);

		expect(
			screen.getByRole("button", { name: /hide_filters/ }),
		).toHaveAttribute("aria-expanded", "true");
		expect(screen.getByLabelText("keyword")).toBeInTheDocument();
	});

	it("applies inline layout and custom content classes", () => {
		render(
			<AdminFilterToolbar
				activeFilterCount={0}
				inline
				contentClassName="my-content"
			>
				<input aria-label="keyword" />
			</AdminFilterToolbar>,
		);

		fireEvent.click(screen.getByRole("button", { name: /show_filters/ }));

		expect(screen.getByLabelText("keyword").parentElement).toHaveClass(
			"my-content",
		);
		expect(
			screen.getByLabelText("keyword").parentElement?.parentElement
				?.parentElement,
		).toHaveClass("basis-full");
	});
});
