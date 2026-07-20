import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { UploadTaskItem } from "@/components/files/UploadTaskItem";

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		onClick,
		title,
	}: {
		children: React.ReactNode;
		onClick?: () => void;
		title?: string;
	}) => (
		<button type="button" onClick={onClick} aria-label={title}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => (
		<span data-testid="icon" data-name={name} />
	),
}));

vi.mock("@/components/ui/progress", () => ({
	Progress: ({ value, className }: { value: number; className?: string }) => (
		<div data-testid="progress" data-value={value} className={className} />
	),
}));

describe("UploadTaskItem", () => {
	it("renders task metadata and falls back to status when detail is absent", () => {
		const { container } = render(
			<UploadTaskItem
				title="report.pdf"
				status="Uploading"
				mode="Chunked"
				progress={45}
				speed="2.0 MB/s"
			/>,
		);

		expect(screen.getByText("report.pdf")).toBeInTheDocument();
		expect(
			screen.getByText(/Chunked · Uploading · 2\.0 MB\/s/),
		).toBeInTheDocument();
		expect(screen.getByText("45%")).toBeInTheDocument();
		expect(screen.getByTestId("progress")).toHaveAttribute("data-value", "45");
		expect(container.firstChild).toHaveClass("bg-card/45");
		expect(screen.getByTestId("icon")).toHaveAttribute("data-name", "Spinner");
	});

	it("renders completed styling, custom detail text, and action callbacks", () => {
		const onRetry = vi.fn();
		const onCancel = vi.fn();
		const { container } = render(
			<UploadTaskItem
				title="archive.zip"
				status="Failed"
				mode="Presigned"
				progress={100}
				detail="Retry required"
				completed
				actions={[
					{ icon: "ArrowsClockwise", label: "Retry", onClick: onRetry },
					{ icon: "X", label: "Cancel", onClick: onCancel },
				]}
			/>,
		);

		expect(screen.getByText(/Presigned · Retry required/)).toBeInTheDocument();
		expect(screen.queryByText("100%")).not.toBeInTheDocument();
		expect(screen.queryByTestId("progress")).not.toBeInTheDocument();
		expect(container.firstChild).toHaveClass("bg-card/45", "border-b");
		expect(screen.getAllByTestId("icon")[0]).toHaveAttribute(
			"data-name",
			"Check",
		);
		expect(screen.getAllByTestId("icon")[1]).toHaveAttribute(
			"data-name",
			"ArrowsClockwise",
		);
		expect(screen.getAllByTestId("icon")[2]).toHaveAttribute("data-name", "X");

		fireEvent.click(screen.getByRole("button", { name: "Retry" }));
		fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

		expect(onRetry).toHaveBeenCalledTimes(1);
		expect(onCancel).toHaveBeenCalledTimes(1);
	});

	it("renders cancelled uploads with a destructive X instead of progress spinner", () => {
		const { container } = render(
			<UploadTaskItem
				title="video.mov"
				status="Upload canceled"
				mode="Chunked"
				progress={42}
				cancelled
			/>,
		);

		expect(screen.getByText(/Chunked · Upload canceled/)).toBeInTheDocument();
		expect(screen.queryByText("42%")).not.toBeInTheDocument();
		expect(screen.queryByTestId("progress")).not.toBeInTheDocument();
		expect(container.firstChild).toHaveClass("bg-destructive/5");
		expect(screen.getByTestId("icon")).toHaveAttribute("data-name", "X");
	});

	it("keeps the progress bar visible for active retried chunked uploads with a cancel action", () => {
		render(
			<UploadTaskItem
				title="world-1542.tar.gz"
				status="Uploading"
				mode="Chunked"
				progress={37}
				detail="Chunk 66/176"
				actions={[{ icon: "X", label: "Cancel", onClick: vi.fn() }]}
			/>,
		);

		expect(screen.getByText(/Chunked · Chunk 66\/176/)).toBeInTheDocument();
		expect(screen.getByText("37%")).toBeInTheDocument();
		expect(screen.getByTestId("progress")).toHaveAttribute("data-value", "37");
	});
});
