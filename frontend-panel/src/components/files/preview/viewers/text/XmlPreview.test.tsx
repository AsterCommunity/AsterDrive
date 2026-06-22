import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { XmlPreview } from "@/components/files/preview/viewers/text/XmlPreview";
import { derivedFileResource } from "@/lib/fileResource";

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

const resource = derivedFileResource("/files/data.xml", {
	deliveryMode: "text",
	scope: "personal",
});

describe("XmlPreview", () => {
	beforeEach(() => {
		mockState.reload.mockReset();
		mockState.useTextContent.mockReset();
		mockState.useTextContent.mockReturnValue({
			content: "<root><child>value</child></root>",
			loading: false,
			error: false,
			reload: mockState.reload,
		});
	});

	it("shows a loading message while XML content is being fetched", () => {
		mockState.useTextContent.mockReturnValue({
			content: null,
			loading: true,
			error: false,
			reload: mockState.reload,
		});

		render(<XmlPreview resource={resource} mode="formatted" />);

		expect(mockState.useTextContent).toHaveBeenCalledWith(resource);
		expect(screen.getByText("loading_preview")).toBeInTheDocument();
	});

	it("renders a retry state when loading fails", () => {
		mockState.useTextContent.mockReturnValue({
			content: null,
			loading: false,
			error: true,
			reload: mockState.reload,
		});

		render(<XmlPreview resource={resource} mode="formatted" />);

		fireEvent.click(screen.getByRole("button", { name: "preview_retry" }));

		expect(screen.getByText("preview_load_failed")).toBeInTheDocument();
		expect(mockState.reload).toHaveBeenCalledTimes(1);
	});

	it("shows a parse failure message for invalid XML", () => {
		mockState.useTextContent.mockReturnValue({
			content: "<root>",
			loading: false,
			error: false,
			reload: mockState.reload,
		});

		render(<XmlPreview resource={resource} mode="formatted" />);

		expect(screen.getByText("structured_parse_failed")).toBeInTheDocument();
	});

	it("formats valid XML content", () => {
		const { container } = render(
			<XmlPreview resource={resource} mode="formatted" />,
		);

		expect(screen.getByText("preview_mode_xml")).toBeInTheDocument();
		expect(screen.getByText("preview_mode_formatted")).toBeInTheDocument();
		expect(container.querySelector("pre")).toHaveTextContent("<root>");
		expect(container.querySelector("pre")).toHaveTextContent(
			/<child>\s*value\s*<\/child>/,
		);
	});
});
