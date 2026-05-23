import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

async function loadModule() {
	vi.resetModules();

	const mapMocks = {
		checkHasIcon: vi.fn((name: string) => name.endsWith(".ts")),
		resolveIcon: vi.fn((name: string) =>
			name.endsWith(".ts")
				? ({ size }: { size?: string }) => (
						<svg data-testid="dev-icon" data-size={size} />
					)
				: null,
		),
	};

	vi.doMock("./language-icons-map", () => mapMocks);

	const mod = await import("./language-icon");
	return { ...mod, mapMocks };
}

afterEach(() => {
	vi.resetModules();
	vi.clearAllMocks();
});

describe("language-icon", () => {
	it("loads the icon map on demand and updates the availability helpers", async () => {
		const { LanguageIcon, hasLanguageIcon, isIconMapLoaded, mapMocks } =
			await loadModule();

		expect(isIconMapLoaded()).toBe(false);
		expect(hasLanguageIcon("main.ts")).toBe(false);

		render(<LanguageIcon name="main.ts" />);
		await vi.dynamicImportSettled();
		await waitFor(() => {
			expect(screen.getByTestId("dev-icon")).toBeInTheDocument();
		});

		expect(isIconMapLoaded()).toBe(true);
		expect(hasLanguageIcon("main.ts")).toBe(true);
		expect(hasLanguageIcon("README")).toBe(false);
		expect(mapMocks.checkHasIcon).toHaveBeenCalledWith("main.ts");
		expect(mapMocks.checkHasIcon).toHaveBeenCalledWith("README");
	});

	it("renders cached icons synchronously after the map has been loaded once", async () => {
		const { LanguageIcon } = await loadModule();

		const firstRender = render(
			<LanguageIcon name="main.ts" size="2rem" className="lang-icon" />,
		);

		expect(screen.queryByTestId("dev-icon")).not.toBeInTheDocument();

		await vi.dynamicImportSettled();
		await waitFor(() => {
			expect(screen.getByTestId("dev-icon")).toBeInTheDocument();
		});

		firstRender.unmount();

		const { container } = render(
			<LanguageIcon name="main.ts" size="3rem" className="cached-icon" />,
		);

		expect(screen.getByTestId("dev-icon")).toHaveAttribute("data-size", "3rem");
		expect(container.firstChild).toHaveClass("inline-flex", "cached-icon");
	});

	it("fills the wrapper when sizing comes from className utilities", async () => {
		const { LanguageIcon } = await loadModule();

		const firstRender = render(<LanguageIcon name="main.ts" />);
		await vi.dynamicImportSettled();
		await waitFor(() => {
			expect(screen.getByTestId("dev-icon")).toBeInTheDocument();
		});
		firstRender.unmount();

		const { container } = render(
			<LanguageIcon name="main.ts" className="size-12 sized-icon" />,
		);

		expect(screen.getByTestId("dev-icon")).toHaveAttribute("data-size", "100%");
		expect(container.firstChild).toHaveClass(
			"inline-flex",
			"size-12",
			"sized-icon",
		);
	});

	it("returns null when no language icon exists", async () => {
		const { LanguageIcon } = await loadModule();

		const { container } = render(<LanguageIcon name="README" />);

		await vi.dynamicImportSettled();
		await waitFor(() => {
			expect(container).toBeEmptyDOMElement();
		});
	});
});
