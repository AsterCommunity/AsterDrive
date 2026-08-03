import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
	FileNameCell,
	FileSizeCell,
	FolderNameCell,
	FolderSizeCell,
	UpdatedAtCell,
} from "@/components/files/FileTableCells";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		i18n: {
			language: "zh",
			resolvedLanguage: "zh",
			t: (key: string) => key,
		},
	}),
}));

vi.mock("@/components/files/FileItemStatusIndicators", () => ({
	FileItemStatusIndicators: ({
		isShared,
		isLocked,
		compact,
	}: {
		isShared?: boolean;
		isLocked?: boolean;
		compact?: boolean;
	}) => (
		<span
			data-testid="status"
			data-shared={String(Boolean(isShared))}
			data-locked={String(Boolean(isLocked))}
			data-compact={String(Boolean(compact))}
		/>
	),
}));

vi.mock("@/components/files/FileThumbnail", () => ({
	FileThumbnail: ({
		file,
		size,
		thumbnailPath,
	}: {
		file: { name: string };
		size?: string;
		thumbnailPath?: string;
	}) => (
		<span
			data-testid="thumbnail"
			data-file-name={file.name}
			data-size={size}
			data-thumbnail-path={thumbnailPath ?? ""}
		/>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => (
		<span data-testid="icon" data-name={name} />
	),
}));

vi.mock("@/components/ui/table", () => ({
	TableCell: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => (
		<td data-testid="table-cell" className={className}>
			{children}
		</td>
	),
}));

vi.mock("@/lib/format", () => ({
	formatBytes: (value: number) => `bytes:${value}`,
	formatDate: (value: string) => `date:${value}`,
}));

describe("FileTableCells", () => {
	it("renders file name cells with thumbnails and status indicators", () => {
		render(
			<FileNameCell
				file={
					{
						id: 1,
						name: "report.pdf",
						is_shared: true,
						lock_state: { state: "direct", mode: "exclusive" },
					} as never
				}
				thumbnailPath="/thumb/1"
			/>,
		);

		expect(screen.getByTestId("thumbnail")).toHaveAttribute(
			"data-file-name",
			"report.pdf",
		);
		expect(screen.getByTestId("thumbnail")).toHaveAttribute("data-size", "sm");
		expect(screen.getByTestId("thumbnail")).toHaveAttribute(
			"data-thumbnail-path",
			"/thumb/1",
		);
		expect(screen.getByText("report.pdf")).toBeInTheDocument();
		expect(screen.getByTestId("status")).toHaveAttribute("data-shared", "true");
		expect(screen.getByTestId("status")).toHaveAttribute("data-locked", "true");
		expect(screen.getByTestId("status")).toHaveAttribute(
			"data-compact",
			"true",
		);
	});

	it("renders folder name cells with folder icons", () => {
		render(
			<FolderNameCell
				folder={
					{
						id: 2,
						name: "Docs",
						is_shared: false,
						lock_state: { state: "direct", mode: "exclusive" },
					} as never
				}
			/>,
		);

		expect(screen.getByTestId("icon")).toHaveAttribute("data-name", "Folder");
		expect(screen.getByText("Docs")).toBeInTheDocument();
		expect(screen.getByTestId("status")).toHaveAttribute("data-locked", "true");
	});

	it("formats file sizes, folder sizes, and update dates", () => {
		const { rerender } = render(<FileSizeCell size={123} />);
		expect(screen.getByText("bytes:123")).toBeInTheDocument();

		rerender(<FolderSizeCell />);
		expect(screen.getByText("---")).toBeInTheDocument();

		rerender(<UpdatedAtCell updatedAt="2026-01-01T00:00:00Z" />);
		expect(screen.getByText("date:2026-01-01T00:00:00Z")).toBeInTheDocument();
	});
});
