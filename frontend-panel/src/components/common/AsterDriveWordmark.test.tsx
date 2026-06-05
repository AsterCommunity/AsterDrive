import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { AsterDriveWordmark } from "@/components/common/AsterDriveWordmark";
import { DEFAULT_BRANDING } from "@/lib/branding";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import { useThemeStore } from "@/stores/themeStore";

describe("AsterDriveWordmark", () => {
	beforeEach(() => {
		document.documentElement.classList.remove("dark");
		useFrontendConfigStore.setState((state) => ({
			...state,
			branding: DEFAULT_BRANDING,
		}));
		useThemeStore.setState({ resolvedTheme: "light" });
	});

	it("uses the dark wordmark on light theme", () => {
		render(<AsterDriveWordmark alt="AsterDrive" />);

		expect(screen.getByRole("img", { name: "AsterDrive" })).toHaveAttribute(
			"src",
			"/static/asterdrive/asterdrive-dark.svg",
		);
	});

	it("uses the light wordmark on dark theme", () => {
		useThemeStore.setState({ resolvedTheme: "dark" });

		render(<AsterDriveWordmark alt="AsterDrive" />);

		expect(screen.getByRole("img", { name: "AsterDrive" })).toHaveAttribute(
			"src",
			"/static/asterdrive/asterdrive-light.svg",
		);
	});

	it("uses the theme store as the single source of truth", () => {
		document.documentElement.classList.add("dark");
		useThemeStore.setState({ resolvedTheme: "light" });

		render(<AsterDriveWordmark alt="AsterDrive" />);

		expect(screen.getByRole("img", { name: "AsterDrive" })).toHaveAttribute(
			"src",
			"/static/asterdrive/asterdrive-dark.svg",
		);
	});

	it("allows overriding the surrounding surface theme", () => {
		render(<AsterDriveWordmark alt="AsterDrive" surfaceTheme="dark" />);

		expect(screen.getByRole("img", { name: "AsterDrive" })).toHaveAttribute(
			"src",
			"/static/asterdrive/asterdrive-light.svg",
		);
	});

	it("uses configured branding wordmark URLs", () => {
		useFrontendConfigStore.setState((state) => ({
			...state,
			branding: {
				...state.branding,
				wordmarkDarkUrl: "https://cdn.example.com/brand/dark.svg",
				wordmarkLightUrl: "https://cdn.example.com/brand/light.svg",
			},
		}));

		render(<AsterDriveWordmark alt="AsterDrive" />);

		expect(screen.getByRole("img", { name: "AsterDrive" })).toHaveAttribute(
			"src",
			"https://cdn.example.com/brand/dark.svg",
		);
	});
});
