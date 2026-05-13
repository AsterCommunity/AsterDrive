import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FileContextMenu } from "@/components/files/FileContextMenu";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key.replace(/^core:/, ""),
	}),
}));

vi.mock("@/components/ui/context-menu", async () => {
	const React = await import("react");
	const GroupContext = React.createContext(false);

	return {
		ContextMenu: (props: { children: React.ReactNode }) => (
			<div>{props.children}</div>
		),
		ContextMenuContent: (props: { children: React.ReactNode }) => (
			<div>{props.children}</div>
		),
		ContextMenuGroup: (props: { children: React.ReactNode }) => (
			<GroupContext.Provider value={true}>
				<div>{props.children}</div>
			</GroupContext.Provider>
		),
		ContextMenuLabel: (props: { children: React.ReactNode }) => {
			if (!React.useContext(GroupContext)) {
				throw new Error(
					"Base UI: MenuGroupRootContext is missing. Menu group parts must be used within <Menu.Group>.",
				);
			}

			return <div>{props.children}</div>;
		},
		ContextMenuItem: (props: {
			children: React.ReactNode;
			className?: string;
			disabled?: boolean;
			onClick?: () => void;
		}) => (
			<button
				type="button"
				className={props.className}
				disabled={props.disabled}
				onClick={props.onClick}
			>
				{props.children}
			</button>
		),
		ContextMenuSeparator: () => <hr />,
		ContextMenuTrigger: (props: {
			children?: React.ReactNode;
			className?: string;
			render?: React.ReactNode;
		}) => (
			<div className={props.className} data-testid="context-trigger">
				{props.render ?? props.children}
			</div>
		),
	};
});

vi.mock("@/components/ui/icon", () => ({
	Icon: () => <span>icon</span>,
}));

function renderMenu(
	overrides: Partial<React.ComponentProps<typeof FileContextMenu>> = {},
) {
	const handlers = {
		onArchiveCompress: vi.fn(),
		onChooseOpenMethod: vi.fn(),
		onArchiveDownload: vi.fn(),
		onArchiveExtract: vi.fn(),
		onCopy: vi.fn(),
		onDelete: vi.fn(),
		onDownload: vi.fn(),
		onDirectShare: vi.fn(),
		onInfo: vi.fn(),
		onMove: vi.fn(),
		onOpen: vi.fn(),
		onPageShare: vi.fn(),
		onRename: vi.fn(),
		onToggleLock: vi.fn(),
		onVersions: vi.fn(),
	};

	render(
		<FileContextMenu
			isFolder={false}
			isLocked={false}
			{...handlers}
			{...overrides}
		>
			<div>file-row</div>
		</FileContextMenu>,
	);

	return handlers;
}

describe("FileContextMenu", () => {
	it("renders file actions and invokes the matching callbacks", () => {
		const handlers = renderMenu();

		fireEvent.click(screen.getByText("open"));
		fireEvent.click(screen.getByText("open_with_action"));
		fireEvent.click(screen.getByText("download"));
		fireEvent.click(screen.getByText("tasks:archive_extract_action"));
		fireEvent.click(screen.getByText("tasks:archive_compress_action"));
		fireEvent.click(screen.getByText("share"));
		fireEvent.click(screen.getByText("share:share_direct_link_action"));
		fireEvent.click(screen.getByText("copy"));
		fireEvent.click(screen.getByText("move"));
		fireEvent.click(screen.getByText("rename"));
		fireEvent.click(screen.getByText("versions"));
		fireEvent.click(screen.getByText("info"));
		fireEvent.click(screen.getByText("lock"));
		fireEvent.click(screen.getByText("delete"));

		expect(handlers.onOpen).toHaveBeenCalledTimes(1);
		expect(handlers.onChooseOpenMethod).toHaveBeenCalledTimes(1);
		expect(handlers.onDownload).toHaveBeenCalledTimes(1);
		expect(handlers.onArchiveExtract).toHaveBeenCalledTimes(1);
		expect(handlers.onArchiveCompress).toHaveBeenCalledTimes(1);
		expect(handlers.onPageShare).toHaveBeenCalledTimes(1);
		expect(handlers.onDirectShare).toHaveBeenCalledTimes(1);
		expect(handlers.onCopy).toHaveBeenCalledTimes(1);
		expect(handlers.onMove).toHaveBeenCalledTimes(1);
		expect(handlers.onRename).toHaveBeenCalledTimes(1);
		expect(handlers.onVersions).toHaveBeenCalledTimes(1);
		expect(handlers.onInfo).toHaveBeenCalledTimes(1);
		expect(handlers.onToggleLock).toHaveBeenCalledTimes(1);
		expect(handlers.onDelete).toHaveBeenCalledTimes(1);
	});

	it("hides file-only actions for folders and disables delete when locked", () => {
		renderMenu({
			isFolder: true,
			isLocked: true,
		});

		expect(screen.queryByText("download")).not.toBeInTheDocument();
		expect(screen.getByText("open")).toBeInTheDocument();
		expect(
			screen.getByText("tasks:archive_compress_action"),
		).toBeInTheDocument();
		expect(
			screen.getByText("tasks:archive_download_action"),
		).toBeInTheDocument();
		expect(screen.getByText("share")).toBeInTheDocument();
		expect(
			screen.queryByText("share:share_direct_link_action"),
		).not.toBeInTheDocument();
		expect(screen.queryByText("open_with_action")).not.toBeInTheDocument();
		expect(screen.queryByText("versions")).not.toBeInTheDocument();
		expect(screen.getByText("unlock")).toBeInTheDocument();
		expect(screen.getByText("delete")).toBeDisabled();
	});

	it("invokes the archive download handler for folders", () => {
		const handlers = renderMenu({
			isFolder: true,
		});

		fireEvent.click(screen.getByText("tasks:archive_download_action"));

		expect(handlers.onArchiveDownload).toHaveBeenCalledTimes(1);
	});

	it("invokes archive extract and compress handlers for files", () => {
		const handlers = renderMenu();

		fireEvent.click(screen.getByText("tasks:archive_extract_action"));
		fireEvent.click(screen.getByText("tasks:archive_compress_action"));

		expect(handlers.onArchiveExtract).toHaveBeenCalledTimes(1);
		expect(handlers.onArchiveCompress).toHaveBeenCalledTimes(1);
	});

	it("uses renderTrigger when requested", () => {
		render(
			<FileContextMenu
				isFolder={false}
				isLocked={false}
				onPageShare={vi.fn()}
				onDirectShare={vi.fn()}
				onCopy={vi.fn()}
				onToggleLock={vi.fn()}
				onDelete={vi.fn()}
				onInfo={vi.fn()}
				renderTrigger
			>
				<button type="button">custom trigger</button>
			</FileContextMenu>,
		);

		expect(screen.getByText("custom trigger")).toBeInTheDocument();
		expect(screen.getByTestId("context-trigger")).toContainElement(
			screen.getByText("custom trigger"),
		);
	});

	it("renders batch actions when selectionCount is provided", () => {
		const handlers = {
			onArchiveCompress: vi.fn(),
			onDownload: vi.fn(),
			onCopy: vi.fn(),
			onDelete: vi.fn(),
			onMove: vi.fn(),
		};

		render(
			<FileContextMenu
				isFolder={false}
				isLocked={false}
				selectionCount={3}
				downloadAction={{
					kind: "archive",
					onClick: handlers.onDownload,
				}}
				{...handlers}
			>
				<div>selected-row</div>
			</FileContextMenu>,
		);

		expect(screen.getByText("selected_count")).toBeInTheDocument();
		expect(screen.queryByText("open")).not.toBeInTheDocument();
		expect(screen.queryByText("share")).not.toBeInTheDocument();
		fireEvent.click(screen.getByText("tasks:archive_download_action"));
		fireEvent.click(screen.getByText("tasks:archive_compress_action"));
		fireEvent.click(screen.getByText("copy"));
		fireEvent.click(screen.getByText("move"));
		fireEvent.click(screen.getByText("delete"));

		expect(handlers.onDownload).toHaveBeenCalledTimes(1);
		expect(handlers.onArchiveCompress).toHaveBeenCalledTimes(1);
		expect(handlers.onCopy).toHaveBeenCalledTimes(1);
		expect(handlers.onMove).toHaveBeenCalledTimes(1);
		expect(handlers.onDelete).toHaveBeenCalledTimes(1);
	});

	it("labels batch download as a regular download for a single file action", () => {
		const onDownload = vi.fn();

		render(
			<FileContextMenu
				isFolder={false}
				isLocked={false}
				selectionCount={2}
				downloadAction={{
					kind: "file",
					onClick: onDownload,
				}}
				onCopy={vi.fn()}
				onDelete={vi.fn()}
				onMove={vi.fn()}
			>
				<div>selected-row</div>
			</FileContextMenu>,
		);

		fireEvent.click(screen.getByText("download"));

		expect(onDownload).toHaveBeenCalledTimes(1);
		expect(
			screen.queryByText("tasks:archive_download_action"),
		).not.toBeInTheDocument();
	});
});
