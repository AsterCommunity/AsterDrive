import { fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import { TopBarBrand } from "./TopBarBrand";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

describe("TopBarBrand", () => {
	it("uses the shared desktop wordmark dimensions and spacing", () => {
		render(
			<MemoryRouter>
				<TopBarBrand />
			</MemoryRouter>,
		);

		expect(screen.getByAltText("app_name")).toHaveClass("h-16", "px-6");
		expect(screen.getByRole("link", { name: "auth:go_home" })).toHaveClass(
			"hidden",
			"md:block",
		);
	});

	it("keeps the wordmark visible on mobile when there is no sidebar toggle", () => {
		render(
			<MemoryRouter>
				<TopBarBrand mobileVisible />
			</MemoryRouter>,
		);

		expect(screen.getByAltText("app_name")).toHaveClass("h-16", "px-6");
		expect(screen.getByRole("link", { name: "auth:go_home" })).toHaveClass(
			"block",
		);
	});

	it("navigates to the home route without a full page reload", () => {
		render(
			<MemoryRouter initialEntries={["/settings/profile"]}>
				<Routes>
					<Route path="/" element={<div>home destination</div>} />
					<Route path="*" element={<TopBarBrand mobileVisible />} />
				</Routes>
			</MemoryRouter>,
		);

		const brandLink = screen.getByRole("link", { name: "auth:go_home" });
		expect(brandLink).toHaveAttribute("href", "/");
		brandLink.focus();
		expect(brandLink).toHaveFocus();

		fireEvent.click(brandLink);

		expect(screen.getByText("home destination")).toBeInTheDocument();
	});
});
