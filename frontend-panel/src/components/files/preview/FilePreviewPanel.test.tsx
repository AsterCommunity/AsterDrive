import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FilePreviewPanel } from "@/components/files/preview/FilePreviewPanel";

vi.mock("@/components/files/FileThumbnail", () => ({
	FileThumbnail: ({ file }: { file: { name: string } }) => (
		<div data-testid="thumbnail">{file.name}</div>
	),
}));

vi.mock("@/components/ui/dialog", () => ({
	DialogHeader: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => (
		<div data-testid="preview-header" className={className}>
			{children}
		</div>
	),
	DialogTitle: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <h2 className={className}>{children}</h2>,
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		"aria-label": ariaLabel,
		children,
		className,
		disabled,
		onClick,
		title,
	}: {
		"aria-label"?: string;
		children: React.ReactNode;
		className?: string;
		disabled?: boolean;
		onClick?: () => void;
		title?: string;
	}) => (
		<button
			type="button"
			aria-label={ariaLabel}
			className={className}
			disabled={disabled}
			onClick={onClick}
			title={title}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/scroll-area", () => ({
	ScrollArea: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => (
		<div data-testid="preview-scroll-area" className={className}>
			{children}
		</div>
	),
}));

function renderPanel({
	isDirty = false,
	isExpanded = false,
	onChooseOpenMethod = vi.fn(),
	onClose = vi.fn(),
	onToggleExpand = vi.fn(),
}: {
	isDirty?: boolean;
	isExpanded?: boolean;
	onChooseOpenMethod?: () => void;
	onClose?: () => void;
	onToggleExpand?: () => void;
} = {}) {
	return render(
		<FilePreviewPanel
			file={{
				id: 7,
				name: "notes.ts",
				mime_type: "text/typescript",
				size: 128,
			}}
			body={<div>preview body</div>}
			allOptionsCount={2}
			usesInnerScroll={false}
			fillsViewportHeight={false}
			isExpanded={isExpanded}
			isDirty={isDirty}
			onChooseOpenMethod={onChooseOpenMethod}
			onToggleExpand={onToggleExpand}
			onClose={onClose}
			chooseOpenMethodLabel="Choose app"
			enterFullscreenLabel="Enter fullscreen"
			exitFullscreenLabel="Exit fullscreen"
			closeLabel="Close"
		/>,
	);
}

describe("FilePreviewPanel", () => {
	it("renders the file name in the dialog title", () => {
		renderPanel();

		expect(
			screen.getByRole("heading", { name: "notes.ts" }),
		).toBeInTheDocument();
	});

	it("renders the body and routes header controls", () => {
		const onChooseOpenMethod = vi.fn();
		const onClose = vi.fn();
		const onToggleExpand = vi.fn();
		renderPanel({ onChooseOpenMethod, onClose, onToggleExpand });

		expect(screen.getByText("preview body")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "Choose app" }));
		fireEvent.click(screen.getByRole("button", { name: "Enter fullscreen" }));
		fireEvent.click(screen.getByRole("button", { name: "Close" }));

		expect(onChooseOpenMethod).toHaveBeenCalledTimes(1);
		expect(onToggleExpand).toHaveBeenCalledTimes(1);
		expect(onClose).toHaveBeenCalledTimes(1);
	});

	it("switches the expand control label from enter to exit while expanded", () => {
		const { rerender } = renderPanel({ isExpanded: false });

		expect(
			screen.getByRole("button", { name: "Enter fullscreen" }),
		).toBeInTheDocument();

		rerender(
			<FilePreviewPanel
				file={{
					id: 7,
					name: "notes.ts",
					mime_type: "text/typescript",
					size: 128,
				}}
				body={<div>preview body</div>}
				allOptionsCount={2}
				usesInnerScroll={false}
				fillsViewportHeight={false}
				isExpanded
				isDirty={false}
				onChooseOpenMethod={vi.fn()}
				onToggleExpand={vi.fn()}
				onClose={vi.fn()}
				chooseOpenMethodLabel="Choose app"
				enterFullscreenLabel="Enter fullscreen"
				exitFullscreenLabel="Exit fullscreen"
				closeLabel="Close"
			/>,
		);

		expect(
			screen.getByRole("button", { name: "Exit fullscreen" }),
		).toBeInTheDocument();
	});

	it("disables open-method selection while dirty", () => {
		renderPanel({ isDirty: true });

		expect(screen.getByRole("button", { name: "Choose app" })).toBeDisabled();
	});
});
