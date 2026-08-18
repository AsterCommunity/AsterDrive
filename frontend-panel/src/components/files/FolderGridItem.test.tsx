import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FolderGridItem } from "@/components/files/FolderGridItem";
import { DRAG_SOURCE_MIME } from "@/lib/constants";

const mockState = vi.hoisted(() => ({
	getInvalidInternalDropReason: vi.fn(),
	hasInternalDragData: vi.fn(),
	readInternalDragData: vi.fn(),
	setInternalDragPreview: vi.fn(),
	writeInternalDragData: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ t: (key: string) => key }),
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

vi.mock("@/components/files/FolderGlyph", () => ({
	FolderGlyph: ({ className }: { className?: string }) => (
		<span data-testid="folder-glyph" className={className} />
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
	lock_state: { state: "unlocked" },
};

describe("FolderGridItem", () => {
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

	it("renders a borderless folder glyph with centered name and selection state", () => {
		const onClick = vi.fn();

		const { container } = render(
			<FolderGridItem
				item={folder as never}
				selected
				onSelect={vi.fn()}
				onClick={onClick}
				fading
			/>,
		);

		const item = screen.getByRole("button", { name: /Docs/i });
		expect(item).toHaveClass("bg-accent/60", "opacity-0", "select-none");
		expect(item).not.toHaveClass("border");
		expect(item).toHaveAttribute("data-folder-drop-target", "true");
		expect(screen.getByTestId("folder-glyph")).toBeInTheDocument();
		// 媒体区不带底色与边框——去卡片化的关键
		const media = container.querySelector("[data-drag-preview-media]");
		expect(media).not.toHaveClass("border");
		expect(media).not.toHaveClass("bg-muted/25");

		const name = screen.getByText("Docs");
		expect(name.parentElement).toHaveClass("text-center");

		fireEvent.click(item);
		fireEvent.keyDown(item, { key: "Enter" });
		expect(onClick).toHaveBeenCalledTimes(2);
	});

	it("uses the double-click handler for keyboard Enter when provided", () => {
		const onClick = vi.fn();
		const onDoubleClick = vi.fn();

		render(
			<FolderGridItem
				item={folder as never}
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

	it("does not open the item when interacting with the action menu", () => {
		const onClick = vi.fn();
		const menuClick = vi.fn();

		const { container } = render(
			<FolderGridItem
				item={folder as never}
				selected={false}
				onSelect={vi.fn()}
				onClick={onClick}
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

		expect(menuClick).toHaveBeenCalledTimes(1);
		expect(onClick).not.toHaveBeenCalled();
	});

	it("accepts valid drops and blocks invalid or source-marker drops", () => {
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
			<FolderGridItem
				item={folder as never}
				selected={false}
				onSelect={vi.fn()}
				onClick={vi.fn()}
				onDrop={onDrop}
				targetPathIds={[1, 2, 7]}
			/>,
		);

		const item = screen.getByRole("button", { name: /Docs/i });

		fireEvent.dragOver(item, { dataTransfer });
		expect(dataTransfer.dropEffect).toBe("move");
		expect(item).toHaveClass("ring-2", "ring-primary");

		fireEvent.drop(item, { dataTransfer });
		expect(mockState.getInvalidInternalDropReason).toHaveBeenCalledWith(
			{ fileIds: [9], folderIds: [3] },
			7,
			[1, 2, 7],
		);
		expect(onDrop).toHaveBeenCalledWith([9], [3], 7, [1, 2, 7]);

		mockState.getInvalidInternalDropReason.mockReturnValueOnce("descendant");
		fireEvent.drop(item, { dataTransfer });
		expect(onDrop).toHaveBeenCalledTimes(1);

		const sourceDataTransfer = {
			types: [DRAG_SOURCE_MIME],
		} as unknown as DataTransfer;
		fireEvent.dragOver(item, { dataTransfer: sourceDataTransfer });
		fireEvent.drop(item, { dataTransfer: sourceDataTransfer });
		expect(onDrop).toHaveBeenCalledTimes(1);
	});
});
