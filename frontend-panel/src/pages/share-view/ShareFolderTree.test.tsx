import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FolderContents, FolderListItem } from "@/types/api";
import { ShareFolderTree } from "./ShareFolderTree";
import type { ShareFolderTreeNode } from "./useShareFolderTree";

const mockState = vi.hoisted(() => ({
	currentFolderId: null as number | null,
	expandedKeys: new Set<string>(),
	loadedKeys: new Set<string>(),
	loadingKeys: new Set<string>(),
	nodeMap: new Map<number, ShareFolderTreeNode>(),
	rootIds: [] as number[],
	toggle: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("@/components/common/SkeletonTree", () => ({
	SkeletonTree: ({ count }: { count: number }) => (
		<div>{`skeleton-tree:${count}`}</div>
	),
}));

vi.mock("@/components/folders/folder-tree/AnimatedTreeGroup", () => ({
	AnimatedTreeGroup: ({
		children,
		open,
	}: {
		children: React.ReactNode;
		open: boolean;
	}) => (open ? <div data-testid="open-tree-group">{children}</div> : null),
}));

vi.mock("./useShareFolderTree", () => ({
	useShareFolderTree: () => mockState,
}));

function folder(id: number, name: string): FolderListItem {
	return {
		id,
		is_locked: false,
		is_shared: false,
		name,
		tags: [],
		updated_at: "2026-07-15T00:00:00Z",
	};
}

function contents(folders: FolderListItem[]): FolderContents {
	return {
		files: [],
		files_total: 0,
		folders,
		folders_total: folders.length,
		next_file_cursor: null,
	} as FolderContents;
}

describe("ShareFolderTree", () => {
	beforeEach(() => {
		mockState.currentFolderId = null;
		mockState.expandedKeys = new Set();
		mockState.loadedKeys = new Set();
		mockState.loadingKeys = new Set();
		mockState.nodeMap = new Map();
		mockState.rootIds = [];
		mockState.toggle.mockReset();
	});

	it("renders a tree skeleton until the canonical breadcrumb is ready", () => {
		render(
			<ShareFolderTree
				breadcrumb={[]}
				folderContents={null}
				rootName="Shared Root"
				token="share-token"
				onNavigate={vi.fn()}
			/>,
		);

		expect(screen.getByText("skeleton-tree:5")).toBeInTheDocument();
	});

	it("navigates and recursively toggles loaded share folders", () => {
		const docs = folder(1, "Docs");
		const deep = folder(2, "Deep");
		mockState.currentFolderId = 2;
		mockState.expandedKeys = new Set(["root", "1"]);
		mockState.loadedKeys = new Set(["root", "1", "2"]);
		mockState.nodeMap = new Map([
			[1, { childIds: [2], folder: docs, parentId: null }],
			[2, { childIds: [], folder: deep, parentId: 1 }],
		]);
		mockState.rootIds = [999, 1];
		const onNavigate = vi.fn();

		render(
			<ShareFolderTree
				breadcrumb={[
					{ id: null, name: "Shared Root" },
					{ id: 1, name: "Docs" },
					{ id: 2, name: "Deep" },
				]}
				folderContents={contents([])}
				rootName="Shared Root"
				token="share-token"
				onNavigate={onNavigate}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Shared Root" }));
		const collapseButtons = screen.getAllByRole("button", {
			name: "collapse_tree",
		});
		expect(collapseButtons[0]).toHaveAttribute("aria-expanded", "true");
		expect(collapseButtons[0]).toHaveClass("size-6");
		fireEvent.click(collapseButtons[0]);
		fireEvent.click(screen.getByRole("button", { name: "Docs" }));
		fireEvent.click(collapseButtons[1]);
		fireEvent.click(screen.getByRole("button", { name: "Deep" }));

		expect(onNavigate).toHaveBeenNthCalledWith(1, null);
		expect(onNavigate).toHaveBeenNthCalledWith(2, 1, "Docs");
		expect(onNavigate).toHaveBeenNthCalledWith(3, 2, "Deep");
		expect(mockState.toggle).toHaveBeenNthCalledWith(1, null);
		expect(mockState.toggle).toHaveBeenNthCalledWith(2, 1);
		expect(screen.getByRole("button", { name: "Deep" })).not.toHaveAttribute(
			"aria-expanded",
		);
		expect(
			document.querySelector('[data-share-folder-tree-row="2"]'),
		).toHaveClass("bg-accent");
	});

	it("shows loading toggles and keeps their keyboard events inside the row", () => {
		const docs = folder(1, "Docs");
		mockState.expandedKeys = new Set(["root"]);
		mockState.loadedKeys = new Set(["root"]);
		mockState.loadingKeys = new Set(["1"]);
		mockState.nodeMap = new Map([
			[1, { childIds: [], folder: docs, parentId: null }],
		]);
		mockState.rootIds = [1];
		const keyDown = vi.fn();

		render(
			<nav onKeyDown={keyDown}>
				<ShareFolderTree
					breadcrumb={[{ id: null, name: "Shared Root" }]}
					folderContents={contents([docs])}
					rootName="Shared Root"
					token="share-token"
					onNavigate={vi.fn()}
				/>
			</nav>,
		);

		const toggle = screen.getByRole("button", { name: "expand_tree" });
		expect(toggle).toBeDisabled();
		fireEvent.keyDown(toggle, { key: "Enter" });
		fireEvent.keyDown(toggle, { key: " " });
		expect(keyDown).not.toHaveBeenCalled();
		fireEvent.keyDown(toggle, { key: "Escape" });
		expect(keyDown).toHaveBeenCalledTimes(1);
	});
});
