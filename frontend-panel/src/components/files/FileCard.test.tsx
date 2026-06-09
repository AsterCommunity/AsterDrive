import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FileCard } from "@/components/files/FileCard";
import { DRAG_SOURCE_MIME } from "@/lib/constants";

const mockState = vi.hoisted(() => ({
	getInvalidInternalDropReason: vi.fn(),
	hasInternalDragData: vi.fn(),
	readInternalDragData: vi.fn(),
	setInternalDragPreview: vi.fn(),
	writeInternalDragData: vi.fn(),
}));

vi.mock("@/components/files/FileItemStatusIndicators", () => ({
	FileItemStatusIndicators: ({
		isShared,
		isLocked,
		compact,
		className,
	}: {
		isShared?: boolean;
		isLocked?: boolean;
		compact?: boolean;
		className?: string;
	}) => (
		<span
			data-testid="status-indicators"
			data-shared={String(Boolean(isShared))}
			data-locked={String(Boolean(isLocked))}
			data-compact={String(Boolean(compact))}
			className={className}
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

vi.mock("@/components/ui/item-checkbox", () => ({
	ItemCheckbox: ({
		checked,
		onChange,
		className,
	}: {
		checked: boolean;
		onChange: () => void;
		className?: string;
	}) => (
		<button
			type="button"
			aria-label="Select item"
			data-checked={String(checked)}
			className={className}
			onClick={(event) => {
				event.stopPropagation();
				onChange();
			}}
		/>
	),
}));

vi.mock("@/lib/dragDrop", () => ({
	getInvalidInternalDropReason: (...args: unknown[]) =>
		mockState.getInvalidInternalDropReason(...args),
	hasInternalDragData: (...args: unknown[]) =>
		mockState.hasInternalDragData(...args),
	readInternalDragData: (...args: unknown[]) =>
		mockState.readInternalDragData(...args),
	setInternalDragPreview: (...args: unknown[]) =>
		mockState.setInternalDragPreview(...args),
	writeInternalDragData: (...args: unknown[]) =>
		mockState.writeInternalDragData(...args),
}));

const folder = {
	id: 7,
	name: "Docs",
	is_shared: false,
	is_locked: false,
};

const file = {
	id: 9,
	name: "report.pdf",
	mime_type: "application/pdf",
	is_shared: true,
	is_locked: true,
};

describe("FileCard", () => {
	beforeEach(() => {
		mockState.getInvalidInternalDropReason.mockReset();
		mockState.hasInternalDragData.mockReset();
		mockState.readInternalDragData.mockReset();
		mockState.setInternalDragPreview.mockReset();
		mockState.writeInternalDragData.mockReset();
		mockState.hasInternalDragData.mockReturnValue(false);
		mockState.readInternalDragData.mockReturnValue(null);
		mockState.getInvalidInternalDropReason.mockReturnValue(null);
	});

	it("renders folder cards with folder icon, selection state, and click handlers", () => {
		const onClick = vi.fn();

		render(
			<FileCard
				item={folder as never}
				isFolder
				selected
				onSelect={vi.fn()}
				onClick={onClick}
				fading
			/>,
		);

		const card = screen.getByRole("button", { name: /Docs/i });
		expect(card).toHaveClass(
			"border-primary",
			"bg-accent",
			"opacity-0",
			"select-none",
		);
		expect(screen.getByTestId("icon")).toHaveAttribute("data-name", "Folder");
		expect(screen.getByText("Docs")).toBeInTheDocument();

		fireEvent.click(card);
		fireEvent.keyDown(card, { key: "Enter" });

		expect(onClick).toHaveBeenCalledTimes(2);
	});

	it("uses the double-click handler for keyboard Enter when provided", () => {
		const onClick = vi.fn();
		const onDoubleClick = vi.fn();

		render(
			<FileCard
				item={folder as never}
				isFolder
				selected={false}
				onSelect={vi.fn()}
				onClick={onClick}
				onDoubleClick={onDoubleClick}
			/>,
		);

		fireEvent.keyDown(screen.getByRole("button", { name: /Docs/i }), {
			key: "Enter",
		});

		expect(onDoubleClick).toHaveBeenCalledTimes(1);
		expect(onClick).not.toHaveBeenCalled();
	});

	it("renders file thumbnails and compact status indicators for files", () => {
		const { container } = render(
			<FileCard
				item={file as never}
				isFolder={false}
				selected={false}
				onSelect={vi.fn()}
				onClick={vi.fn()}
				thumbnailPath="/thumb/9"
			/>,
		);

		expect(screen.getByTestId("thumbnail")).toHaveAttribute(
			"data-file-name",
			"report.pdf",
		);
		expect(screen.getByTestId("thumbnail")).toHaveAttribute(
			"data-thumbnail-path",
			"/thumb/9",
		);
		expect(screen.getByTestId("status-indicators")).toHaveAttribute(
			"data-shared",
			"true",
		);
		expect(screen.getByTestId("status-indicators")).toHaveAttribute(
			"data-locked",
			"true",
		);
		expect(screen.getByTestId("status-indicators")).toHaveAttribute(
			"data-compact",
			"true",
		);
		expect(container.querySelector("[data-drag-preview-media]")).toHaveClass(
			"overflow-hidden",
		);
	});

	it("toggles selection from the checkbox without firing the card click handler", () => {
		const onSelect = vi.fn();
		const onClick = vi.fn();

		render(
			<FileCard
				item={file as never}
				isFolder={false}
				selected={false}
				onSelect={onSelect}
				onClick={onClick}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Select item" }));

		expect(onSelect).toHaveBeenCalledTimes(1);
		expect(onClick).not.toHaveBeenCalled();
	});

	it("keeps the grid card action menu mobile-only and reclaims desktop status space", () => {
		const { container } = render(
			<FileCard
				item={file as never}
				isFolder={false}
				selected={false}
				onSelect={vi.fn()}
				onClick={vi.fn()}
				actionMenu={<button type="button">more</button>}
			/>,
		);

		expect(container.querySelector("[data-file-card-action-menu]")).toHaveClass(
			"sm:hidden",
		);
		expect(screen.getByTestId("status-indicators")).toHaveClass(
			"right-11",
			"sm:right-2",
		);
	});

	it("does not open the card when interacting with the action menu", () => {
		const onClick = vi.fn();
		const onDoubleClick = vi.fn();
		const menuClick = vi.fn();

		const { container } = render(
			<FileCard
				item={file as never}
				isFolder={false}
				selected={false}
				onSelect={vi.fn()}
				onClick={onClick}
				onDoubleClick={onDoubleClick}
				actionMenu={
					<button type="button" onClick={menuClick}>
						more
					</button>
				}
			/>,
		);

		const actionMenu = container.querySelector("[data-file-card-action-menu]");
		expect(actionMenu).not.toBeNull();

		fireEvent.pointerDown(actionMenu as Element);
		fireEvent.click(screen.getByRole("button", { name: "more" }));
		fireEvent.doubleClick(actionMenu as Element);
		fireEvent.keyDown(actionMenu as Element, { key: "Enter" });

		expect(menuClick).toHaveBeenCalledTimes(1);
		expect(onClick).not.toHaveBeenCalled();
		expect(onDoubleClick).not.toHaveBeenCalled();
	});

	it("writes drag data and drag preview metadata on drag start", () => {
		const dataTransfer = { types: [] } as unknown as DataTransfer;

		render(
			<FileCard
				item={file as never}
				isFolder={false}
				selected={false}
				onSelect={vi.fn()}
				onClick={vi.fn()}
				dragData={{ fileIds: [9, 10], folderIds: [2] }}
			/>,
		);

		fireEvent.dragStart(screen.getByRole("button", { name: /report\.pdf/i }), {
			dataTransfer,
		});

		expect(mockState.writeInternalDragData).toHaveBeenCalledWith(dataTransfer, {
			fileIds: [9, 10],
			folderIds: [2],
		});
		expect(mockState.setInternalDragPreview).toHaveBeenCalledWith(
			expect.anything(),
			{
				variant: "grid-card",
				itemCount: 3,
			},
		);
	});

	it("accepts valid folder drops and blocks invalid or source-marker drops", () => {
		const onDrop = vi.fn();
		const dataTransfer = {
			types: ["application/x-asterdrive-move"],
			dropEffect: "copy",
		} as unknown as DataTransfer;
		mockState.hasInternalDragData.mockReturnValue(true);
		mockState.readInternalDragData.mockReturnValue({
			fileIds: [9],
			folderIds: [3],
		});

		render(
			<FileCard
				item={folder as never}
				isFolder
				selected={false}
				onSelect={vi.fn()}
				onClick={vi.fn()}
				onDrop={onDrop}
				targetPathIds={[1, 2, 7]}
			/>,
		);

		const card = screen.getByRole("button", { name: /Docs/i });

		fireEvent.dragOver(card, { dataTransfer });
		expect(dataTransfer.dropEffect).toBe("move");
		expect(card).toHaveClass("ring-2", "ring-primary");

		fireEvent.drop(card, { dataTransfer });
		expect(mockState.getInvalidInternalDropReason).toHaveBeenCalledWith(
			{ fileIds: [9], folderIds: [3] },
			7,
			[1, 2, 7],
		);
		expect(onDrop).toHaveBeenCalledWith([9], [3], 7, [1, 2, 7]);

		mockState.getInvalidInternalDropReason.mockReturnValueOnce("descendant");
		fireEvent.drop(card, { dataTransfer });
		expect(onDrop).toHaveBeenCalledTimes(1);

		const sourceDataTransfer = {
			types: [DRAG_SOURCE_MIME],
		} as unknown as DataTransfer;
		fireEvent.dragOver(card, { dataTransfer: sourceDataTransfer });
		fireEvent.drop(card, { dataTransfer: sourceDataTransfer });
		expect(onDrop).toHaveBeenCalledTimes(1);
	});
});
