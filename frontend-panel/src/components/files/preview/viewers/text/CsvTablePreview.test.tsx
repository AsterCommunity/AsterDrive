import { fireEvent, render, screen } from "@testing-library/react";
import Papa from "papaparse";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CsvTablePreview } from "@/components/files/preview/viewers/text/CsvTablePreview";
import { derivedFileResource } from "@/lib/fileResource";

const mockState = vi.hoisted(() => ({
	reload: vi.fn(),
	useTextContent: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) => {
			if (key === "column") return "column";
			if (key === "files:table_truncated") {
				return `files:table_truncated:${options?.count}`;
			}
			return key;
		},
	}),
}));

vi.mock("@/hooks/useTextContent", () => ({
	useTextContent: (...args: unknown[]) => mockState.useTextContent(...args),
}));

const resource = derivedFileResource("/files/table.csv", {
	deliveryMode: "text",
	scope: "personal",
});

describe("CsvTablePreview", () => {
	beforeEach(() => {
		mockState.reload.mockReset();
		mockState.useTextContent.mockReset();
		mockState.useTextContent.mockReturnValue({
			content: "name,role\nAster,admin",
			loading: false,
			error: false,
			reload: mockState.reload,
		});
	});

	it("shows a loading state while table content is being fetched", () => {
		mockState.useTextContent.mockReturnValue({
			content: null,
			loading: true,
			error: false,
			reload: mockState.reload,
		});

		render(<CsvTablePreview resource={resource} delimiter="," />);

		expect(mockState.useTextContent).toHaveBeenCalledWith(resource);
		expect(screen.getByText("files:loading_preview")).toBeInTheDocument();
	});

	it("renders a retry state when loading fails", () => {
		mockState.useTextContent.mockReturnValue({
			content: null,
			loading: false,
			error: true,
			reload: mockState.reload,
		});

		render(<CsvTablePreview resource={resource} delimiter="," />);

		fireEvent.click(screen.getByRole("button", { name: "preview_retry" }));

		expect(screen.getByText("preview_load_failed")).toBeInTheDocument();
		expect(mockState.reload).toHaveBeenCalledTimes(1);
	});

	it("shows a parse failure message when the content cannot produce rows", () => {
		mockState.useTextContent.mockReturnValue({
			content: "",
			loading: false,
			error: false,
			reload: mockState.reload,
		});

		render(<CsvTablePreview resource={resource} delimiter="," />);

		expect(screen.getByText("files:table_parse_failed")).toBeInTheDocument();
	});

	it("renders parsed rows and falls back to generated column labels", () => {
		mockState.useTextContent.mockReturnValue({
			content: ",Role\nAster,admin",
			loading: false,
			error: false,
			reload: mockState.reload,
		});

		render(<CsvTablePreview resource={resource} delimiter="," />);

		expect(screen.getByText("column 1")).toBeInTheDocument();
		expect(screen.getByText("Role")).toBeInTheDocument();
		expect(screen.getByText("Aster")).toBeInTheDocument();
		expect(screen.getByText("admin")).toBeInTheDocument();
	});

	it("auto-detects delimiters for the default table preview mode", () => {
		mockState.useTextContent.mockReturnValue({
			content: "name;role\nAster;admin",
			loading: false,
			error: false,
			reload: mockState.reload,
		});
		const parseSpy = vi.spyOn(Papa, "parse");

		render(<CsvTablePreview resource={resource} delimiter="auto" />);

		expect(parseSpy).toHaveBeenCalledWith(
			"name;role\nAster;admin",
			expect.objectContaining({
				delimitersToGuess: [",", "\t", ";", "|"],
				skipEmptyLines: true,
			}),
		);
		expect(screen.getByText("Aster")).toBeInTheDocument();
		expect(screen.getByText("admin")).toBeInTheDocument();

		parseSpy.mockRestore();
	});

	it("supports tab-delimited content and truncates very large tables", () => {
		const rows = ["name\trole"];
		rows.push(
			...Array.from({ length: 500 }, (_, index) => `user-${index}\tmember`),
		);
		mockState.useTextContent.mockReturnValue({
			content: rows.join("\n"),
			loading: false,
			error: false,
			reload: mockState.reload,
		});
		const parseSpy = vi.spyOn(Papa, "parse");

		render(<CsvTablePreview resource={resource} delimiter={"\t"} />);

		expect(parseSpy).toHaveBeenCalledWith(
			rows.join("\n"),
			expect.objectContaining({
				delimiter: "\t",
				skipEmptyLines: true,
			}),
		);
		expect(screen.getByText("user-0")).toBeInTheDocument();
		expect(screen.getAllByText("member")[0]).toBeInTheDocument();
		expect(screen.getByText("files:table_truncated:500")).toBeInTheDocument();

		parseSpy.mockRestore();
	});
});
