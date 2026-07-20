import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
	TransferActivitySection,
	TransferTaskItem,
} from "@/components/files/TransferActivitySection";

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ className, name }: { className?: string; name: string }) => (
		<span data-testid={`icon-${name}`} className={className} />
	),
}));

vi.mock("@/components/ui/progress", () => ({
	Progress: ({ className, value }: { className?: string; value: number }) => (
		<div role="progressbar" aria-valuenow={value} className={className} />
	),
}));

describe("TransferActivitySection", () => {
	it("keeps the collapsed body inaccessible while preserving its contents", () => {
		render(
			<TransferActivitySection
				open={false}
				onToggle={vi.fn()}
				title="Downloads"
				summary="2 active"
				icon="Download"
				toggleLabel="Toggle downloads"
				expandedBodyClassName="h-96"
			>
				<div>task list</div>
			</TransferActivitySection>,
		);

		const trigger = screen.getByRole("button", { name: "Toggle downloads" });
		const body = screen.getByText("task list").parentElement;
		expect(trigger).toHaveAttribute("aria-expanded", "false");
		expect(body).toHaveAttribute("aria-hidden", "true");
		expect(body).toHaveAttribute("inert");
		expect(body).toHaveAttribute("data-state", "closed");
		expect(body).toHaveClass("h-0", "opacity-0");
		expect(screen.getByTestId("icon-CaretUp")).toBeInTheDocument();
		expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
	});

	it("renders expanded progress and invokes the summary toggle", () => {
		const onToggle = vi.fn();
		render(
			<TransferActivitySection
				open
				onToggle={onToggle}
				title="Uploads"
				summary="Half way"
				icon="Upload"
				toggleLabel="Toggle uploads"
				expandedBodyClassName="h-80"
				progress={0}
				tone="error"
			>
				<div>upload list</div>
			</TransferActivitySection>,
		);

		const trigger = screen.getByRole("button", { name: "Toggle uploads" });
		fireEvent.click(trigger);
		expect(onToggle).toHaveBeenCalledTimes(1);
		expect(trigger).toHaveAttribute("aria-expanded", "true");
		expect(trigger).toHaveTextContent("0%");
		const body = screen.getByText("upload list").parentElement;
		expect(body).toHaveAttribute("aria-hidden", "false");
		expect(body).not.toHaveAttribute("inert");
		expect(body).toHaveAttribute("data-state", "open");
		expect(body).toHaveClass("h-80", "opacity-100");
		expect(screen.getByRole("progressbar")).toHaveAttribute(
			"aria-valuenow",
			"0",
		);
		expect(screen.getByTestId("icon-CaretDown")).toBeInTheDocument();
	});

	it("does not route action clicks through the section toggle", () => {
		const onToggle = vi.fn();
		const onAction = vi.fn();
		render(
			<TransferActivitySection
				open={false}
				onToggle={onToggle}
				title="Downloads"
				summary="Done"
				icon="Download"
				toggleLabel="Toggle downloads"
				expandedBodyClassName="h-96"
				actions={
					<button type="button" onClick={onAction}>
						Clear
					</button>
				}
			>
				<div>tasks</div>
			</TransferActivitySection>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Clear" }));
		expect(onAction).toHaveBeenCalledTimes(1);
		expect(onToggle).not.toHaveBeenCalled();
	});
});

describe("TransferTaskItem", () => {
	it("renders active progress, actions, warning, error, and rich detail", () => {
		const onCancel = vi.fn();
		render(
			<TransferTaskItem
				title="report.txt"
				detail={<span>2 MB / 4 MB</span>}
				icon="Spinner"
				tone="active"
				progress={0}
				progressLabel="0%"
				warning="Using memory fallback"
				error="Network failed"
				actions={
					<button type="button" onClick={onCancel}>
						Cancel
					</button>
				}
			/>,
		);

		expect(screen.getByText("report.txt")).toBeInTheDocument();
		expect(screen.getByText("2 MB / 4 MB")).toBeInTheDocument();
		expect(screen.getByText("0%")).toBeInTheDocument();
		expect(screen.getByText("Using memory fallback")).toBeInTheDocument();
		expect(screen.getByText("Network failed")).toBeInTheDocument();
		expect(screen.getByTestId("icon-Spinner")).toHaveClass("animate-spin");
		expect(screen.getByRole("progressbar")).toHaveAttribute(
			"aria-valuenow",
			"0",
		);
		fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
		expect(onCancel).toHaveBeenCalledTimes(1);
	});

	it("omits optional task affordances for a completed item", () => {
		render(
			<TransferTaskItem
				title="done.zip"
				detail="Completed"
				icon="Check"
				tone="success"
			/>,
		);

		expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
		expect(screen.queryByRole("button")).not.toBeInTheDocument();
		expect(screen.getByTestId("icon-Check")).not.toHaveClass("animate-spin");
	});
});
