import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FileCard } from "@/components/files/FileCard";

const mockState = vi.hoisted(() => ({
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
	setInternalDragPreview: (...args: unknown[]) =>
		mockState.setInternalDragPreview(...args),
	writeInternalDragData: (...args: unknown[]) =>
		mockState.writeInternalDragData(...args),
}));

const file = {
	id: 9,
	name: "report.pdf",
	mime_type: "application/pdf",
	size: 2048,
	is_shared: true,
	lock_state: { state: "direct", mode: "exclusive" },
};

describe("FileCard", () => {
	beforeEach(() => {
		mockState.setInternalDragPreview.mockReset();
		mockState.writeInternalDragData.mockReset();
	});

	it("renders file thumbnails and compact status indicators for files", () => {
		const { container } = render(
			<FileCard
				item={file as never}
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
		expect(screen.getByText("report.pdf").parentElement).not.toHaveClass(
			"text-center",
		);
	});

	it("toggles selection from the checkbox without firing the card click handler", () => {
		const onSelect = vi.fn();
		const onClick = vi.fn();

		render(
			<FileCard
				item={file as never}
				selected={false}
				onSelect={onSelect}
				onClick={onClick}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Select item" }));

		expect(onSelect).toHaveBeenCalledTimes(1);
		expect(onClick).not.toHaveBeenCalled();
	});

	it("marks selected cards with a ring and keeps long unbreakable names wrapped", () => {
		const longNameFile = {
			...file,
			name: "458123c16ba00578ae49b09a70f8b66d.png",
		};
		render(
			<FileCard
				item={longNameFile as never}
				selected={true}
				onSelect={vi.fn()}
				onClick={vi.fn()}
			/>,
		);

		const card = screen.getByRole("button", { name: /458123c16ba/ });
		// 选中态：背景 + 主色 ring 常驻指示（单选多选一致）
		expect(card).toHaveClass("bg-accent/60", "ring-2", "ring-primary/60");
		// 鼠标点击不出 UA 焦点框；键盘焦点经 focus-visible 保留
		expect(card).toHaveClass("outline-none", "focus-visible:ring-2");
		// 无空格长文件名（hash 名）允许硬断折行，不横向溢出
		expect(screen.getByText(longNameFile.name)).toHaveClass(
			"line-clamp-2",
			"break-words",
		);
	});

	it("keeps the grid card action menu mobile-only and reclaims desktop status space", () => {
		const { container } = render(
			<FileCard
				item={file as never}
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

	it("keeps the action menu visible when selection is disabled", () => {
		const { container } = render(
			<FileCard
				item={file as never}
				selected={false}
				onSelect={vi.fn()}
				onClick={vi.fn()}
				selectable={false}
				actionMenu={<button type="button">download</button>}
			/>,
		);

		expect(
			container.querySelector("[data-file-card-action-menu]"),
		).not.toHaveClass("sm:hidden");
		expect(
			screen.queryByRole("button", { name: "Select item" }),
		).not.toBeInTheDocument();
	});

	it("can keep the action menu visible while selection remains enabled", () => {
		const { container } = render(
			<FileCard
				item={file as never}
				selected={false}
				onSelect={vi.fn()}
				onClick={vi.fn()}
				selectable
				alwaysShowActionMenu
				actionMenu={<button type="button">download</button>}
			/>,
		);

		expect(
			container.querySelector("[data-file-card-action-menu]"),
		).not.toHaveClass("sm:hidden");
		expect(
			screen.getByRole("button", { name: "Select item" }),
		).toBeInTheDocument();
	});

	it("does not open the card when interacting with the action menu", () => {
		const onClick = vi.fn();
		const onDoubleClick = vi.fn();
		const menuClick = vi.fn();

		const { container } = render(
			<FileCard
				item={file as never}
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
		fireEvent.keyDown(actionMenu as Element, { key: "Escape" });

		expect(menuClick).toHaveBeenCalledTimes(1);
		expect(onClick).not.toHaveBeenCalled();
		expect(onDoubleClick).not.toHaveBeenCalled();
	});

	it("writes drag data and drag preview metadata on drag start", () => {
		const dataTransfer = { types: [] } as unknown as DataTransfer;

		render(
			<FileCard
				item={file as never}
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
});
