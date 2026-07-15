import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TopBarBrand } from "./TopBarBrand";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

describe("TopBarBrand", () => {
	it("uses the shared desktop wordmark dimensions and spacing", () => {
		render(<TopBarBrand />);

		expect(screen.getByAltText("app_name")).toHaveClass(
			"h-16",
			"px-6",
			"hidden",
			"md:block",
		);
	});

	it("keeps the wordmark visible on mobile when there is no sidebar toggle", () => {
		render(<TopBarBrand mobileVisible />);

		expect(screen.getByAltText("app_name")).toHaveClass(
			"h-16",
			"px-6",
			"block",
		);
		expect(screen.getByAltText("app_name")).not.toHaveClass("hidden");
	});
});
