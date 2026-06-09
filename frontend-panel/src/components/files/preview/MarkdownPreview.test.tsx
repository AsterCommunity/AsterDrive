import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MarkdownPreview } from "@/components/files/preview/MarkdownPreview";

const mockState = vi.hoisted(() => ({
	reload: vi.fn(),
	useTextContent: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/components/ui/scroll-area", () => ({
	ScrollArea: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={className}>{children}</div>,
}));

vi.mock("@/hooks/useTextContent", () => ({
	useTextContent: (...args: unknown[]) => mockState.useTextContent(...args),
}));

describe("MarkdownPreview", () => {
	beforeEach(() => {
		mockState.reload.mockReset();
		mockState.useTextContent.mockReset();
		mockState.useTextContent.mockReturnValue({
			content: "# Title\n\n**Bold**",
			loading: false,
			error: false,
			reload: mockState.reload,
		});
	});

	it("shows a loading state while markdown content is being fetched", () => {
		mockState.useTextContent.mockReturnValue({
			content: null,
			loading: true,
			error: false,
			reload: mockState.reload,
		});

		render(<MarkdownPreview path="/files/markdown" />);

		expect(mockState.useTextContent).toHaveBeenCalledWith("/files/markdown");
		expect(screen.getByText("loading_preview")).toBeInTheDocument();
	});

	it("shows a retry state when loading fails", () => {
		mockState.useTextContent.mockReturnValue({
			content: null,
			loading: false,
			error: true,
			reload: mockState.reload,
		});

		render(<MarkdownPreview path="/files/markdown" />);

		fireEvent.click(screen.getByRole("button", { name: "preview_retry" }));

		expect(screen.getByText("preview_load_failed")).toBeInTheDocument();
		expect(mockState.reload).toHaveBeenCalledTimes(1);
	});

	it("renders markdown content and strips unsafe markup", () => {
		mockState.useTextContent.mockReturnValue({
			content: "# Title\n\n**Bold**\n\n<script>alert('xss')</script>",
			loading: false,
			error: false,
			reload: mockState.reload,
		});

		const { container } = render(<MarkdownPreview path="/files/markdown" />);

		expect(screen.getByText("preview_mode_markdown")).toBeInTheDocument();
		expect(screen.getByText("preview_mode_rendered")).toBeInTheDocument();
		expect(screen.getByRole("heading", { name: "Title" })).toBeInTheDocument();
		expect(screen.getByText("Bold")).toBeInTheDocument();
		expect(container.querySelector("script")).toBeNull();
		expect(screen.queryByText("alert('xss')")).not.toBeInTheDocument();
	});
});
