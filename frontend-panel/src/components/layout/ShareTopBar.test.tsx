import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ShareTopBar } from "@/components/layout/ShareTopBar";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => `translated:${key}`,
	}),
}));

vi.mock("@/components/layout/TopBarShell", () => ({
	TopBarShell: ({
		left,
		right,
		heightClassName,
	}: {
		left: React.ReactNode;
		right: React.ReactNode;
		heightClassName?: string;
	}) => (
		<div data-testid="share-topbar-shell" data-height={heightClassName}>
			<div>{left}</div>
			<div>{right}</div>
		</div>
	),
}));

describe("ShareTopBar", () => {
	it("renders a compact public-share top bar", () => {
		render(<ShareTopBar />);

		expect(screen.getByAltText("translated:app_name")).toBeInTheDocument();
		expect(screen.getByTestId("share-topbar-shell")).toHaveAttribute(
			"data-height",
			"h-14",
		);
		expect(screen.getByText("translated:files:share")).toHaveClass("sr-only");
	});
});
