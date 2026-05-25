import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Sidebar } from "@/components/layout/Sidebar";
import { STORAGE_KEYS } from "@/config/app";

const mockState = vi.hoisted(() => ({
	scrollAreaTestIds: [] as Array<string | undefined>,
	pathname: "/",
	auth: {
		user: {
			storage_quota: 100,
			storage_used: 25,
		},
	},
	teams: [],
	workspace: {
		kind: "personal" as const,
	},
	hasInternalDragData: vi.fn(),
	readInternalDragData: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, string>) => {
			if (key === "files:storage_quota") {
				return `${key}:${options?.used}/${options?.quota}`;
			}
			if (key === "files:storage_used") {
				return `${key}:${options?.used}`;
			}
			return `translated:${key}`;
		},
	}),
}));

vi.mock("react-router-dom", () => ({
	Link: ({
		children,
		onClick,
		onDragOver,
		onDragLeave,
		onDrop,
		className,
	}: {
		children: React.ReactNode;
		onClick?: () => void;
		onDragOver?: (event: React.DragEvent<HTMLButtonElement>) => void;
		onDragLeave?: (event: React.DragEvent<HTMLButtonElement>) => void;
		onDrop?: (event: React.DragEvent<HTMLButtonElement>) => void;
		className?: string;
	}) => (
		<button
			type="button"
			className={className}
			onClick={onClick}
			onDragOver={onDragOver}
			onDragLeave={onDragLeave}
			onDrop={onDrop}
		>
			{children}
		</button>
	),
	useLocation: () => ({
		pathname: mockState.pathname,
	}),
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: (selector: (state: typeof mockState.auth) => unknown) =>
		selector(mockState.auth),
}));

vi.mock("@/stores/teamStore", () => ({
	useTeamStore: (
		selector: (state: { teams: typeof mockState.teams }) => unknown,
	) =>
		selector({
			teams: mockState.teams,
		}),
}));

vi.mock("@/stores/workspaceStore", () => ({
	useWorkspaceStore: (
		selector: (state: { workspace: typeof mockState.workspace }) => unknown,
	) =>
		selector({
			workspace: mockState.workspace,
		}),
}));

vi.mock("@/components/folders/FolderTree", () => ({
	FolderTree: ({ onMoveToFolder }: { onMoveToFolder?: unknown }) => (
		<div
			data-testid="folder-tree"
			data-has-move={String(Boolean(onMoveToFolder))}
		>
			FolderTree
		</div>
	),
}));

vi.mock("@/components/layout/WorkspaceSwitcher", () => ({
	WorkspaceSwitcher: ({ variant }: { variant?: string }) => (
		<div data-testid="workspace-switcher" data-variant={variant}>
			WorkspaceSwitcher
		</div>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => (
		<span data-testid="icon" data-name={name} />
	),
}));

vi.mock("@/components/ui/progress", () => ({
	Progress: ({ value }: { value: number }) => (
		<div data-testid="progress" data-value={String(value)} />
	),
}));

vi.mock("@/components/ui/scroll-area", () => ({
	ScrollArea: ({
		children,
		className,
		"data-testid": testId,
	}: {
		children: React.ReactNode;
		className?: string;
		"data-testid"?: string;
	}) => (
		<div
			data-testid={testId ?? "scroll-area"}
			className={className}
			ref={() => {
				mockState.scrollAreaTestIds.push(testId);
			}}
		>
			{children}
		</div>
	),
}));

vi.mock("@/components/ui/separator", () => ({
	Separator: () => <hr data-testid="separator" />,
}));

vi.mock("@/lib/format", () => ({
	formatBytes: (value: number) => `formatted:${value}`,
}));

vi.mock("@/lib/dragDrop", () => ({
	hasInternalDragData: (dataTransfer: DataTransfer | null) =>
		mockState.hasInternalDragData(dataTransfer),
	readInternalDragData: (dataTransfer: DataTransfer | null) =>
		mockState.readInternalDragData(dataTransfer),
}));

describe("Sidebar", () => {
	beforeEach(() => {
		localStorage.clear();
		mockState.pathname = "/";
		mockState.auth.user = {
			storage_quota: 100,
			storage_used: 25,
		};
		mockState.teams = [];
		mockState.workspace = {
			kind: "personal",
		};
		mockState.hasInternalDragData.mockReset();
		mockState.readInternalDragData.mockReset();
		mockState.hasInternalDragData.mockReturnValue(false);
		mockState.readInternalDragData.mockReturnValue(null);
		mockState.scrollAreaTestIds = [];
	});

	it("renders navigation, folder tree, and storage quota usage", () => {
		render(<Sidebar mobileOpen={false} onMobileClose={vi.fn()} />);

		expect(screen.getByTestId("folder-tree")).toHaveAttribute(
			"data-has-move",
			"false",
		);
		expect(mockState.scrollAreaTestIds).toEqual(["user-sidebar-scroll"]);
		const scrollArea = screen.getByTestId("user-sidebar-scroll");
		expect(scrollArea).toHaveClass("min-h-0", "flex-1");
		expect(scrollArea).toContainElement(screen.getByTestId("folder-tree"));
		expect(scrollArea.firstElementChild).toHaveClass("flex", "min-h-full");
		expect(
			screen
				.getByText("translated:search:quick_categories")
				.closest(".mt-auto"),
		).not.toBeNull();
		expect(screen.getByTestId("workspace-switcher")).toHaveAttribute(
			"data-variant",
			"sidebar",
		);
		expect(scrollArea).toContainElement(
			screen.getByRole("button", {
				name: /translated:search:category_image/i,
			}),
		);
		expect(
			screen.getByRole("button", { name: /translated:share:my_shares_title/i }),
		).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: /translated:tasks:title/i }),
		).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: /translated:trash/i }),
		).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: /translated:webdav/i }),
		).toBeInTheDocument();
		expect(
			screen.getByText("translated:files:storage_space"),
		).toBeInTheDocument();
		expect(
			screen.getByText("translated:files:storage_space").parentElement,
		).toHaveClass("pb-[calc(0.75rem+env(safe-area-inset-bottom))]", "md:pb-3");
		expect(scrollArea).not.toContainElement(
			screen.getByText("translated:files:storage_space"),
		);
		expect(screen.getByTestId("progress")).toHaveAttribute("data-value", "25");
		expect(
			screen.getByText("files:storage_quota:formatted:25/formatted:100"),
		).toBeInTheDocument();
	});

	it("renders storage used copy when no quota is configured", () => {
		mockState.auth.user = {
			storage_quota: 0,
			storage_used: 25,
		};

		render(<Sidebar mobileOpen={false} onMobileClose={vi.fn()} />);

		expect(screen.getByTestId("progress")).toHaveAttribute("data-value", "0");
		expect(
			screen.getByText("files:storage_used:formatted:25"),
		).toBeInTheDocument();
	});

	it("renders and closes the mobile overlay", () => {
		const onMobileClose = vi.fn();

		const { container, rerender } = render(
			<Sidebar mobileOpen={false} onMobileClose={onMobileClose} />,
		);

		expect(container.querySelector("aside")?.className).toContain(
			"-translate-x-full",
		);

		rerender(<Sidebar mobileOpen onMobileClose={onMobileClose} />);

		expect(container.querySelector("aside")?.className).toContain(
			"translate-x-0",
		);

		fireEvent.click(
			screen.getByRole("button", { name: "translated:close_sidebar" }),
		);

		expect(onMobileClose).toHaveBeenCalledTimes(1);
	});

	it("handles trash drag and drop for internal move payloads", () => {
		const onTrashDrop = vi.fn();
		const dataTransfer = { dropEffect: "copy" } as DataTransfer;
		mockState.hasInternalDragData.mockReturnValue(true);
		mockState.readInternalDragData.mockReturnValue({
			fileIds: [1],
			folderIds: [2],
		});

		render(
			<Sidebar
				mobileOpen={false}
				onMobileClose={vi.fn()}
				onTrashDrop={onTrashDrop}
			/>,
		);

		const trashButton = screen.getByRole("button", {
			name: /translated:trash/i,
		});

		fireEvent.dragOver(trashButton, { dataTransfer });

		expect(dataTransfer.dropEffect).toBe("move");
		expect(trashButton.className).toContain("bg-destructive/10");

		fireEvent.drop(trashButton, { dataTransfer });

		expect(onTrashDrop).toHaveBeenCalledWith({
			fileIds: [1],
			folderIds: [2],
		});
		expect(trashButton.className).not.toContain("bg-destructive/10");
	});

	it("uses dynamic viewport mobile overlay positioning below the top bar", () => {
		const { container } = render(
			<Sidebar mobileOpen onMobileClose={vi.fn()} />,
		);

		expect(
			screen.getByRole("button", { name: "translated:close_sidebar" })
				.className,
		).toContain("h-[calc(100dvh-4rem)]");
		expect(
			screen.getByRole("button", { name: "translated:close_sidebar" })
				.className,
		).toContain("top-16");
		expect(container.querySelector("aside")?.className).toContain(
			"h-[calc(100dvh-4rem)]",
		);
	});

	it("resizes the desktop sidebar by dragging the divider and persists the width", () => {
		const { container } = render(
			<Sidebar mobileOpen={false} onMobileClose={vi.fn()} />,
		);

		const aside = container.querySelector("aside");
		const resizer = screen.getByRole("slider", {
			name: "translated:resize_sidebar",
		});

		expect(aside).toHaveStyle("--user-sidebar-width: 240px");
		expect(resizer).toHaveValue("240");

		fireEvent.pointerDown(resizer, { button: 0, clientX: 240 });
		fireEvent.pointerMove(window, { clientX: 320 });

		expect(aside).toHaveStyle("--user-sidebar-width: 320px");
		expect(resizer).toHaveValue("320");
		expect(document.body.style.cursor).toBe("col-resize");

		fireEvent.pointerUp(window);

		expect(localStorage.getItem(STORAGE_KEYS.userSidebarWidth)).toBe("320");
		expect(document.body.style.cursor).toBe("");
	});

	it("supports keyboard resizing for the desktop sidebar divider", () => {
		const { container } = render(
			<Sidebar mobileOpen={false} onMobileClose={vi.fn()} />,
		);

		const aside = container.querySelector("aside");
		const resizer = screen.getByRole("slider", {
			name: "translated:resize_sidebar",
		});

		fireEvent.keyDown(resizer, { key: "End" });
		expect(aside).toHaveStyle("--user-sidebar-width: 420px");
		expect(resizer).toHaveValue("420");
		expect(localStorage.getItem(STORAGE_KEYS.userSidebarWidth)).toBe("420");

		fireEvent.keyDown(resizer, { key: "Home" });
		expect(aside).toHaveStyle("--user-sidebar-width: 220px");
		expect(resizer).toHaveValue("220");
		expect(localStorage.getItem(STORAGE_KEYS.userSidebarWidth)).toBe("220");
	});

	it("opens quick category search links and closes the mobile sidebar", () => {
		const onMobileClose = vi.fn();
		const onSearchCategoryOpen = vi.fn();

		render(
			<Sidebar
				mobileOpen
				onMobileClose={onMobileClose}
				onSearchCategoryOpen={onSearchCategoryOpen}
			/>,
		);

		fireEvent.click(
			screen.getByRole("button", {
				name: /translated:search:category_image/i,
			}),
		);

		expect(onSearchCategoryOpen).toHaveBeenCalledWith("image");
		expect(onMobileClose).toHaveBeenCalledTimes(1);
	});
});
