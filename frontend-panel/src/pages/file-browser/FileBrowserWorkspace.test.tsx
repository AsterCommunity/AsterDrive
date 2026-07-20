import { fireEvent, render, screen } from "@testing-library/react";
import { createRef } from "react";
import { describe, expect, it, vi } from "vitest";
import type { FileBrowserContextValue } from "@/components/files/FileBrowserContext";
import { FileBrowserWorkspace } from "@/pages/file-browser/FileBrowserWorkspace";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/components/common/EmptyState", () => ({
	EmptyState: (props: { description?: string; title: string }) => (
		<div>
			<div>{props.title}</div>
			<div>{props.description}</div>
		</div>
	),
}));

vi.mock("@/components/common/SkeletonFileGrid", () => ({
	SkeletonFileGrid: () => <div>skeleton-grid</div>,
}));

vi.mock("@/components/common/SkeletonFileTable", () => ({
	SkeletonFileTable: () => <div>skeleton-table</div>,
}));

vi.mock("@/components/files/FileBrowserContext", () => ({
	FileBrowserProvider: (props: {
		children: React.ReactNode;
		value: FileBrowserContextValue;
	}) => (
		<div data-browser-open-mode={props.value.browserOpenMode}>
			{props.children}
		</div>
	),
}));

vi.mock("@/components/files/FileGrid", () => ({
	FileGrid: () => <div>file-grid</div>,
}));

vi.mock("@/components/files/FileTable", () => ({
	FileTable: () => <div>file-table</div>,
}));

vi.mock("@/components/ui/context-menu", () => ({
	ContextMenu: (props: { children: React.ReactNode }) => (
		<div>{props.children}</div>
	),
	ContextMenuContent: (props: { children: React.ReactNode }) => (
		<div>{props.children}</div>
	),
	ContextMenuItem: (props: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
	}) => (
		<button type="button" disabled={props.disabled} onClick={props.onClick}>
			{props.children}
		</button>
	),
	ContextMenuSeparator: () => <hr />,
	ContextMenuTrigger: (props: {
		children: React.ReactNode;
		className?: string;
	}) => <div className={props.className}>{props.children}</div>,
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: () => <span>icon</span>,
}));

vi.mock("@/components/ui/scroll-area", () => ({
	ScrollArea: (props: {
		children: React.ReactNode;
		className?: string;
		viewportProps?: {
			className?: string;
		};
		ref?: (node: HTMLDivElement | null) => void;
	}) => (
		<div
			ref={(node) => {
				props.ref?.(node);
			}}
			className={props.className}
			data-viewport-class={props.viewportProps?.className ?? ""}
		>
			{props.children}
		</div>
	),
}));

vi.mock("@/pages/file-browser/fileBrowserLazy", () => ({
	FileInfoDialog: (props: {
		file?: { id: number; name: string };
		folder?: { id: number; name: string };
		onDownload: (fileId: number, fileName: string) => void;
		onOpenChange: (open: boolean) => void;
		onOpenFolder: (folder: { id: number; name: string }) => void;
		onPreview: (file: { id: number; name: string }) => void;
		onRename: (type: "file" | "folder", id: number, name: string) => void;
		onShare: (target: {
			fileId?: number;
			folderId?: number;
			name: string;
		}) => void;
		onToggleLock: (
			type: "file" | "folder",
			id: number,
			locked: boolean,
		) => Promise<boolean>;
		onVersions: (fileId: number) => void;
		open: boolean;
	}) => {
		const file = props.file ?? { id: 0, name: "" };
		const folder = props.folder ?? { id: 0, name: "" };

		return props.open ? (
			<div>
				<button type="button" onClick={() => props.onPreview(file)}>
					preview
				</button>
				<button type="button" onClick={() => props.onOpenFolder(folder)}>
					open-folder
				</button>
				<button
					type="button"
					onClick={() =>
						props.onShare({
							fileId: props.file?.id,
							folderId: props.folder?.id,
							name: props.file?.name ?? props.folder?.name ?? "",
						})
					}
				>
					share
				</button>
				<button
					type="button"
					onClick={() =>
						props.onDownload(props.file?.id ?? 0, props.file?.name ?? "")
					}
				>
					download
				</button>
				<button
					type="button"
					onClick={() =>
						props.onRename("file", props.file?.id ?? 0, props.file?.name ?? "")
					}
				>
					rename
				</button>
				<button
					type="button"
					onClick={() => props.onVersions(props.file?.id ?? 0)}
				>
					versions
				</button>
				<button
					type="button"
					onClick={() =>
						void props.onToggleLock("file", props.file?.id ?? 0, false)
					}
				>
					lock
				</button>
				<button type="button" onClick={() => props.onOpenChange(false)}>
					close-info
				</button>
			</div>
		) : null;
	},
}));

function createContextValue(): FileBrowserContextValue {
	return {
		browserOpenMode: "single_click",
		breadcrumbPathIds: [12],
		files: [],
		folders: [],
		onCopy: vi.fn(),
		onDelete: vi.fn(),
		onDownload: vi.fn(),
		onFileClick: vi.fn(),
		onFolderOpen: vi.fn(),
		onInfo: vi.fn(),
		onShare: vi.fn(),
		onToggleLock: vi.fn(),
	};
}

function renderWorkspace(
	overrides: Partial<React.ComponentProps<typeof FileBrowserWorkspace>> = {},
) {
	const handlers = {
		onContentDragLeave: vi.fn(),
		onContentDragOver: vi.fn(),
		onContentDrop: vi.fn().mockResolvedValue(undefined),
		onCreateFile: vi.fn(),
		onCreateFolder: vi.fn(),
		onDownload: vi.fn(),
		onInfoPanelOpenChange: vi.fn(),
		onOpenInfoFolder: vi.fn(),
		onPreview: vi.fn(),
		onRefresh: vi.fn(),
		onRename: vi.fn(),
		onScrollViewportRef: vi.fn(),
		onShare: vi.fn(),
		onToggleLock: vi.fn().mockResolvedValue(true),
		onTriggerFileUpload: vi.fn(),
		onTriggerFolderUpload: vi.fn(),
		onVersions: vi.fn(),
	};

	const result = render(
		<FileBrowserWorkspace
			breadcrumb={[
				{ id: null, name: "Root" },
				{ id: 12, name: "Workspace" },
			]}
			contentDragOver={false}
			error={null}
			fileBrowserContextValue={createContextValue()}
			hasMoreFiles={false}
			infoPanelOpen={false}
			infoTarget={null}
			isEmpty={false}
			loading={false}
			loadingMore={false}
			scrollViewport={null}
			sentinelRef={createRef<HTMLDivElement>()}
			uploadReady
			viewMode="grid"
			bottomOverlayOffset="none"
			{...handlers}
			{...overrides}
		/>,
	);

	return { ...handlers, ...result };
}

describe("FileBrowserWorkspace", () => {
	it("renders the empty drop target state and current folder overlay", () => {
		renderWorkspace({
			contentDragOver: true,
			isEmpty: true,
		});

		expect(screen.getByText("move_to_current_folder")).toBeInTheDocument();
		expect(screen.getByText("Workspace")).toBeInTheDocument();
		expect(screen.getByText("folder_empty")).toBeInTheDocument();
		expect(screen.getByText("folder_empty_desc")).toBeInTheDocument();
	});

	it("allows read-only views to override the empty state copy", () => {
		renderWorkspace({
			emptyDescription: "category_empty_desc",
			emptyTitle: "category_empty",
			isEmpty: true,
			uploadReady: false,
		});

		expect(screen.getByText("category_empty")).toBeInTheDocument();
		expect(screen.getByText("category_empty_desc")).toBeInTheDocument();
		expect(screen.queryByText("folder_empty_desc")).not.toBeInTheDocument();
	});

	it("wires context menu actions and forwarded info dialog callbacks", () => {
		const handlers = renderWorkspace({
			infoPanelOpen: true,
			infoTarget: {
				file: { id: 7, name: "demo.txt" } as never,
				folder: { id: 12, name: "Workspace" } as never,
			},
		});

		fireEvent.click(screen.getByText("upload_file"));
		fireEvent.click(screen.getByText("upload_folder"));
		fireEvent.click(screen.getByText("new_folder"));
		fireEvent.click(screen.getByText("new_file"));
		fireEvent.click(screen.getByText("core:refresh"));
		fireEvent.click(screen.getByText("preview"));
		fireEvent.click(screen.getByText("open-folder"));
		fireEvent.click(screen.getByText("share"));
		fireEvent.click(screen.getByText("download"));
		fireEvent.click(screen.getByText("rename"));
		fireEvent.click(screen.getByText("versions"));
		fireEvent.click(screen.getByText("lock"));
		fireEvent.click(screen.getByText("close-info"));

		expect(screen.getByText("file-grid")).toBeInTheDocument();
		expect(handlers.onTriggerFileUpload).toHaveBeenCalledTimes(1);
		expect(handlers.onTriggerFolderUpload).toHaveBeenCalledTimes(1);
		expect(handlers.onCreateFolder).toHaveBeenCalledTimes(1);
		expect(handlers.onCreateFile).toHaveBeenCalledTimes(1);
		expect(handlers.onRefresh).toHaveBeenCalledTimes(1);
		expect(handlers.onPreview).toHaveBeenCalledWith({
			id: 7,
			name: "demo.txt",
		});
		expect(handlers.onOpenInfoFolder).toHaveBeenCalledWith({
			id: 12,
			name: "Workspace",
		});
		expect(handlers.onShare).toHaveBeenCalledWith({
			fileId: 7,
			folderId: 12,
			name: "demo.txt",
		});
		expect(handlers.onDownload).toHaveBeenCalledWith(7, "demo.txt");
		expect(handlers.onRename).toHaveBeenCalledWith("file", 7, "demo.txt");
		expect(handlers.onVersions).toHaveBeenCalledWith(7);
		expect(handlers.onToggleLock).toHaveBeenCalledWith("file", 7, false);
		expect(handlers.onInfoPanelOpenChange).toHaveBeenCalledWith(false);
	});

	it("reserves bottom space for fixed bottom overlays", () => {
		const view = renderWorkspace({
			bottomOverlayOffset: "selection-compact",
		});

		expect(screen.getByText("file-grid").closest(".min-h-0")).toHaveAttribute(
			"data-viewport-class",
			expect.stringContaining("pb-[calc(5.5rem"),
		);

		view.rerender(
			<FileBrowserWorkspace
				breadcrumb={[
					{ id: null, name: "Root" },
					{ id: 12, name: "Workspace" },
				]}
				contentDragOver={false}
				error={null}
				fileBrowserContextValue={createContextValue()}
				hasMoreFiles={false}
				infoPanelOpen={false}
				infoTarget={null}
				isEmpty={false}
				loading={false}
				loadingMore={false}
				scrollViewport={null}
				sentinelRef={createRef<HTMLDivElement>()}
				uploadReady
				viewMode="grid"
				bottomOverlayOffset="upload-compact"
				onContentDragLeave={vi.fn()}
				onContentDragOver={vi.fn()}
				onContentDrop={vi.fn().mockResolvedValue(undefined)}
				onCreateFile={vi.fn()}
				onCreateFolder={vi.fn()}
				onDownload={vi.fn()}
				onInfoPanelOpenChange={vi.fn()}
				onOpenInfoFolder={vi.fn()}
				onOfflineDownload={vi.fn()}
				onPreview={vi.fn()}
				onRefresh={vi.fn()}
				onRename={vi.fn()}
				onScrollViewportRef={vi.fn()}
				onShare={vi.fn()}
				onToggleLock={vi.fn().mockResolvedValue(true)}
				onTriggerFileUpload={vi.fn()}
				onTriggerFolderUpload={vi.fn()}
				onVersions={vi.fn()}
			/>,
		);

		expect(screen.getByText("file-grid").closest(".min-h-0")).toHaveAttribute(
			"data-viewport-class",
			expect.stringContaining(
				"pb-[calc(var(--bottom-right-activity-shell-height,0px)+2rem",
			),
		);

		view.rerender(
			<FileBrowserWorkspace
				breadcrumb={[
					{ id: null, name: "Root" },
					{ id: 12, name: "Workspace" },
				]}
				contentDragOver={false}
				error={null}
				fileBrowserContextValue={createContextValue()}
				hasMoreFiles={false}
				infoPanelOpen={false}
				infoTarget={null}
				isEmpty={false}
				loading={false}
				loadingMore={false}
				scrollViewport={null}
				sentinelRef={createRef<HTMLDivElement>()}
				uploadReady
				viewMode="grid"
				bottomOverlayOffset="expanded"
				onContentDragLeave={vi.fn()}
				onContentDragOver={vi.fn()}
				onContentDrop={vi.fn().mockResolvedValue(undefined)}
				onCreateFile={vi.fn()}
				onCreateFolder={vi.fn()}
				onDownload={vi.fn()}
				onInfoPanelOpenChange={vi.fn()}
				onOpenInfoFolder={vi.fn()}
				onOfflineDownload={vi.fn()}
				onPreview={vi.fn()}
				onRefresh={vi.fn()}
				onRename={vi.fn()}
				onScrollViewportRef={vi.fn()}
				onShare={vi.fn()}
				onToggleLock={vi.fn().mockResolvedValue(true)}
				onTriggerFileUpload={vi.fn()}
				onTriggerFolderUpload={vi.fn()}
				onVersions={vi.fn()}
			/>,
		);

		expect(screen.getByText("file-grid").closest(".min-h-0")).toHaveAttribute(
			"data-viewport-class",
			expect.stringContaining(
				"pb-[calc(var(--bottom-right-activity-shell-height,0px)+2rem",
			),
		);
	});
});
