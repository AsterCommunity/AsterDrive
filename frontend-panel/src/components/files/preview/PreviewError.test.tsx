import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PreviewError } from "@/components/files/preview/PreviewError";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

describe("PreviewError", () => {
	it("renders the translated error message and retry button", () => {
		const onRetry = vi.fn();

		render(<PreviewError onRetry={onRetry} />);

		const alert = screen.getByRole("alert");
		expect(alert).toHaveClass(
			"h-full",
			"min-h-[12rem]",
			"items-center",
			"justify-center",
		);
		expect(screen.getByText("preview_load_failed")).toBeInTheDocument();
		const retryButton = screen.getByRole("button", { name: "preview_retry" });
		fireEvent.click(retryButton);

		expect(onRetry).toHaveBeenCalledTimes(1);
	});

	it("omits the retry button when no retry handler is provided", () => {
		render(<PreviewError />);

		expect(screen.getByRole("alert")).toHaveClass("h-full", "min-h-[12rem]");
		expect(screen.getByText("preview_load_failed")).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "preview_retry" }),
		).not.toBeInTheDocument();
	});

	it("keeps custom messages centered without requiring a retry action", () => {
		render(<PreviewError messageKey="preview_custom_failure" />);

		const alert = screen.getByRole("alert");

		expect(alert).toHaveClass("h-full", "justify-center", "text-center");
		expect(screen.getByText("preview_custom_failure")).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "preview_retry" }),
		).not.toBeInTheDocument();
	});

	it("uses a caller-selected dark appearance that does not depend on theme variables", () => {
		const onRetry = vi.fn();

		render(<PreviewError appearance="dark" onRetry={onRetry} />);

		const alert = screen.getByRole("alert");
		const retryButton = screen.getByRole("button", { name: "preview_retry" });
		const iconFrame = alert.querySelector("svg")?.parentElement;

		expect(alert).toHaveClass(
			"h-full",
			"min-h-[12rem]",
			"bg-zinc-950",
			"text-zinc-400",
			"items-center",
			"justify-center",
		);
		expect(iconFrame).toHaveClass(
			"border-white/16",
			"bg-white/12",
			"text-zinc-300",
		);
		expect(retryButton).toHaveClass(
			"border-white/14",
			"bg-white/10",
			"text-zinc-100",
			"hover:bg-white/16",
			"dark:bg-white/10",
		);

		fireEvent.click(retryButton);
		expect(onRetry).toHaveBeenCalledTimes(1);
	});

	it("allows callers to add layout constraints without removing centering", () => {
		render(<PreviewError className="min-h-0 px-2" />);

		const alert = screen.getByRole("alert");

		expect(alert).toHaveClass("h-full", "min-h-0", "px-2", "justify-center");
		expect(alert).not.toHaveClass("min-h-[12rem]");
	});
});
