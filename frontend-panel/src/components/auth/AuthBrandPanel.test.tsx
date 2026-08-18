import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_BRANDING } from "@/lib/branding";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import { useThemeStore } from "@/stores/themeStore";
import { AuthBrandPanel } from "./AuthBrandPanel";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
}));

describe("AuthBrandPanel", () => {
	beforeEach(() => {
		useFrontendConfigStore.setState((state) => ({
			...state,
			branding: DEFAULT_BRANDING,
		}));
		useThemeStore.setState({ resolvedTheme: "light" });
	});

	it("falls back to the i18n slogan with default branding", () => {
		render(<AuthBrandPanel />);

		expect(screen.getByText("slogan")).toBeInTheDocument();
	});

	it("shows the custom site description as slogan when configured", () => {
		useFrontendConfigStore.setState((state) => ({
			...state,
			branding: {
				...state.branding,
				description: "Team ACME drive",
			},
		}));

		render(<AuthBrandPanel />);

		expect(screen.getByText("Team ACME drive")).toBeInTheDocument();
		expect(screen.queryByText("slogan")).not.toBeInTheDocument();
	});

	it("renders the instance host instead of a version line", () => {
		render(<AuthBrandPanel />);

		expect(screen.getByText(window.location.host)).toBeInTheDocument();
	});

	it("uses the configured favicon as the brand mark", () => {
		useFrontendConfigStore.setState((state) => ({
			...state,
			branding: {
				...state.branding,
				faviconUrl: "https://cdn.example.com/mark.svg",
			},
		}));

		const { container } = render(<AuthBrandPanel />);

		const mark = container.querySelector(
			'img[src="https://cdn.example.com/mark.svg"]',
		);
		expect(mark).not.toBeNull();
	});

	it("lets the wordmark follow the active theme", () => {
		useThemeStore.setState({ resolvedTheme: "dark" });

		render(<AuthBrandPanel />);

		expect(
			screen.getByRole("img", { name: DEFAULT_BRANDING.title }),
		).toHaveAttribute("src", "/static/asterdrive/asterdrive-light.svg");
	});
});
