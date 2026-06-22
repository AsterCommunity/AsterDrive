import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { JsonPreview } from "@/components/files/preview/viewers/text/JsonPreview";
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

vi.mock("prism-react-renderer", () => ({
	Highlight: ({
		code,
		children,
	}: {
		code: string;
		children: (props: {
			className: string;
			style: Record<string, unknown>;
			tokens: Array<Array<{ content: string; types: string[] }>>;
			getLineProps: (args: { key: string }) => { key: string };
			getTokenProps: (args: {
				token: { content: string; types: string[] };
			}) => { children: string };
		}) => React.ReactNode;
	}) =>
		children({
			className: "highlight",
			style: {},
			tokens: code
				.split("\n")
				.map((line) => [{ content: line, types: ["plain"] }]),
			getLineProps: () => ({}),
			getTokenProps: ({ token }) => ({ children: token.content }),
		}),
	themes: { github: {} },
}));

vi.mock("@/hooks/useTextContent", () => ({
	useTextContent: (...args: unknown[]) => mockState.useTextContent(...args),
}));

const resource = derivedFileResource("/files/data.json", {
	deliveryMode: "text",
	scope: "personal",
});

describe("JsonPreview", () => {
	beforeEach(() => {
		mockState.reload.mockReset();
		mockState.useTextContent.mockReset();
		mockState.useTextContent.mockReturnValue({
			content: '{"name":"AsterDrive"}',
			loading: false,
			error: false,
			reload: mockState.reload,
		});
	});

	it("shows a loading message while JSON content is being fetched", () => {
		mockState.useTextContent.mockReturnValue({
			content: null,
			loading: true,
			error: false,
			reload: mockState.reload,
		});

		render(<JsonPreview resource={resource} />);

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

		render(<JsonPreview resource={resource} />);

		fireEvent.click(screen.getByRole("button", { name: "preview_retry" }));

		expect(screen.getByText("preview_load_failed")).toBeInTheDocument();
		expect(mockState.reload).toHaveBeenCalledTimes(1);
	});

	it("shows a parse failure message for invalid JSON", () => {
		mockState.useTextContent.mockReturnValue({
			content: "{invalid json}",
			loading: false,
			error: false,
			reload: mockState.reload,
		});

		render(<JsonPreview resource={resource} />);

		expect(screen.getByText("structured_parse_failed")).toBeInTheDocument();
	});

	it("formats and renders valid JSON content", () => {
		mockState.useTextContent.mockReturnValue({
			content: '{"name":"AsterDrive","count":2}',
			loading: false,
			error: false,
			reload: mockState.reload,
		});

		const { container } = render(<JsonPreview resource={resource} />);

		expect(screen.getByText("preview_mode_json")).toBeInTheDocument();
		expect(screen.getByText("preview_mode_formatted")).toBeInTheDocument();
		expect(container.querySelector("pre")).toHaveTextContent(
			/"name": "AsterDrive",/,
		);
		expect(container.querySelector("pre")).toHaveTextContent(/"count": 2/);
	});
});
