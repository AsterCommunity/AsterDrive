import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { GlobalSearchDialog } from "@/components/layout/GlobalSearchDialog";
import type {
	FileCategory,
	FileListItem,
	FolderListItem,
	TagInfo,
} from "@/types/api";

const mockState = vi.hoisted(() => ({
	getFile: vi.fn(),
	handleApiError: vi.fn(),
	intersectionCallback: null as IntersectionObserverCallback | null,
	listTags: vi.fn(),
	navigate: vi.fn(),
	search: vi.fn(),
	workspace: { kind: "personal" as const },
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) =>
			options ? `${key}:${JSON.stringify(options)}` : key,
	}),
}));

vi.mock("react-router-dom", () => ({
	useNavigate: () => mockState.navigate,
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: mockState.handleApiError,
}));

vi.mock("@/stores/workspaceStore", () => ({
	useWorkspaceStore: (
		selector: (state: { workspace: typeof mockState.workspace }) => unknown,
	) => selector({ workspace: mockState.workspace }),
}));

vi.mock("@/services/searchService", () => ({
	searchService: {
		search: mockState.search,
	},
}));

vi.mock("@/services/fileService", () => ({
	fileService: {
		getFile: mockState.getFile,
	},
}));

vi.mock("@/services/tagService", () => ({
	createTagService: () => ({
		listTags: mockState.listTags,
	}),
}));

vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({
		open,
		children,
	}: {
		open: boolean;
		onOpenChange: (open: boolean) => void;
		children: ReactNode;
	}) => (open ? <div>{children}</div> : null),
	DialogContent: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	DialogHeader: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({ children }: { children: ReactNode }) => <div>{children}</div>,
	DialogDescription: ({ children }: { children: ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => (
		<span data-testid="icon" data-name={name} />
	),
}));

vi.mock("@/components/files/FileThumbnail", () => ({
	FileThumbnail: () => <span data-testid="file-thumbnail">thumb</span>,
}));

vi.mock("@/components/files/TagLibraryManagerDialog", () => ({
	TagLibraryManagerDialog: ({
		onTagDeleted,
		onTagUpdated,
		open,
	}: {
		open: boolean;
		onOpenChange: (open: boolean) => void;
		onTagDeleted?: (tagId: number) => void;
		onTagUpdated?: (tag: TagInfo) => void;
	}) =>
		open ? (
			<div data-testid="tag-library-manager">
				<button
					type="button"
					onClick={() =>
						onTagUpdated?.({
							id: 1,
							name: "Alpha Prime",
							color: "#7c3aed",
							usage_count: 2,
							scope_type: "personal",
							owner_user_id: 1,
							team_id: null,
							normalized_name: "alpha prime",
							sort_order: 0,
							created_at: "2026-06-08T00:00:00Z",
							updated_at: "2026-06-08T00:00:00Z",
						})
					}
				>
					library-update-alpha
				</button>
				<button type="button" onClick={() => onTagDeleted?.(2)}>
					library-delete-beta
				</button>
			</div>
		) : null,
}));

function waitForSearchDebounce() {
	return new Promise((resolve) => window.setTimeout(resolve, 220));
}

function fileItem(
	overrides: Partial<FileListItem> & Pick<FileListItem, "id" | "name">,
): FileListItem {
	const extension =
		overrides.extension ?? overrides.name.split(".").pop() ?? "";
	const category: FileCategory = overrides.file_category ?? "document";

	return {
		compound_extension: null,
		extension,
		file_category: category,
		is_locked: false,
		is_shared: false,
		mime_type: "text/plain",
		size: 2048,
		updated_at: "2026-04-15T12:00:00Z",
		...overrides,
	};
}

function tag(id: number, name: string, color = "#2563eb"): TagInfo {
	return {
		id,
		name,
		color,
		usage_count: 0,
		scope_type: "personal",
		owner_user_id: 1,
		team_id: null,
		normalized_name: name.trim().toLowerCase(),
		sort_order: 0,
		created_at: "2026-06-08T00:00:00Z",
		updated_at: "2026-06-08T00:00:00Z",
	};
}

describe("GlobalSearchDialog", () => {
	beforeEach(() => {
		mockState.getFile.mockReset();
		mockState.handleApiError.mockReset();
		mockState.intersectionCallback = null;
		mockState.listTags.mockReset();
		mockState.listTags.mockResolvedValue({
			items: [],
			limit: 100,
			offset: 0,
			total: 0,
		});
		mockState.navigate.mockReset();
		mockState.search.mockReset();

		window.IntersectionObserver = class MockIntersectionObserver {
			constructor(callback: IntersectionObserverCallback) {
				mockState.intersectionCallback = callback;
			}

			observe() {}

			disconnect() {}

			unobserve() {}

			takeRecords() {
				return [];
			}

			root = null;
			rootMargin = "";
			thresholds = [];
		} as typeof IntersectionObserver;
	});

	it("debounces searches and renders grouped results", async () => {
		const folder: FolderListItem = {
			id: 3,
			is_locked: false,
			is_shared: false,
			name: "Reports",
			updated_at: "2026-04-15T12:00:00Z",
		};
		const file = fileItem({
			id: 7,
			name: "report.txt",
		});
		mockState.search.mockResolvedValue({
			files: [file],
			folders: [folder],
			total_files: 1,
			total_folders: 1,
		});

		render(<GlobalSearchDialog open onOpenChange={vi.fn()} />);

		fireEvent.change(screen.getByPlaceholderText("search:placeholder"), {
			target: { value: "report" },
		});
		await waitForSearchDebounce();

		await waitFor(() => {
			expect(mockState.search).toHaveBeenCalledWith(
				{
					q: "report",
					type: "all",
					limit: 10,
				},
				{ signal: expect.any(AbortSignal) },
			);
		});

		expect(await screen.findByText("Reports")).toBeInTheDocument();
		expect(screen.getByText("report.txt")).toBeInTheDocument();
	});

	it("opens file results in their parent folder with preview state", async () => {
		const onOpenChange = vi.fn();
		const file = fileItem({
			id: 7,
			name: "report.txt",
		});
		mockState.search.mockResolvedValue({
			files: [file],
			folders: [],
			total_files: 1,
			total_folders: 0,
		});
		mockState.getFile.mockResolvedValue({
			folder_id: 42,
		});

		render(<GlobalSearchDialog open onOpenChange={onOpenChange} />);

		fireEvent.change(screen.getByPlaceholderText("search:placeholder"), {
			target: { value: "report" },
		});
		await waitForSearchDebounce();
		expect(await screen.findByText("report.txt")).toBeInTheDocument();

		fireEvent.click(screen.getByText("report.txt"));

		await waitFor(() => {
			expect(mockState.getFile).toHaveBeenCalledWith(7);
		});
		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(mockState.navigate).toHaveBeenCalledWith("/folder/42", {
			state: {
				searchPreviewFile: file,
			},
			viewTransition: false,
		});
	});

	it("loads more results when the sentinel enters view", async () => {
		const firstPageFile = fileItem({
			id: 7,
			name: "report-1.txt",
		});
		const secondPageFile = fileItem({
			id: 8,
			name: "report-2.txt",
			size: 1024,
		});

		mockState.search
			.mockResolvedValueOnce({
				files: [firstPageFile],
				folders: [],
				total_files: 2,
				total_folders: 0,
			})
			.mockResolvedValueOnce({
				files: [secondPageFile],
				folders: [],
				total_files: 2,
				total_folders: 0,
			});

		render(<GlobalSearchDialog open onOpenChange={vi.fn()} />);

		fireEvent.change(screen.getByPlaceholderText("search:placeholder"), {
			target: { value: "report" },
		});
		await waitForSearchDebounce();
		expect(await screen.findByText("report-1.txt")).toBeInTheDocument();
		expect(screen.getByText("report-1.txt")).toBeInTheDocument();

		const loadMoreTarget = document.querySelector("[data-search-load-more]");
		expect(loadMoreTarget).not.toBeNull();
		expect(mockState.intersectionCallback).not.toBeNull();

		mockState.intersectionCallback?.(
			[
				{
					isIntersecting: true,
					target: loadMoreTarget as Element,
				} as IntersectionObserverEntry,
			],
			{} as IntersectionObserver,
		);

		await waitFor(() => {
			expect(mockState.search).toHaveBeenNthCalledWith(
				2,
				{
					q: "report",
					type: "all",
					limit: 10,
					offset: 1,
				},
				{ signal: expect.any(AbortSignal) },
			);
		});
		expect(await screen.findByText("report-2.txt")).toBeInTheDocument();
	});

	it("ignores load-more observers that are not intersecting and swallows load-more failures", async () => {
		const firstPageFile = fileItem({
			id: 7,
			name: "report-1.txt",
		});
		mockState.search
			.mockResolvedValueOnce({
				files: [firstPageFile],
				folders: [],
				total_files: 2,
				total_folders: 0,
			})
			.mockRejectedValueOnce(new Error("load more failed"));

		render(<GlobalSearchDialog open onOpenChange={vi.fn()} />);

		fireEvent.change(screen.getByPlaceholderText("search:placeholder"), {
			target: { value: "report" },
		});
		await waitForSearchDebounce();
		expect(await screen.findByText("report-1.txt")).toBeInTheDocument();

		const loadMoreTarget = document.querySelector("[data-search-load-more]");
		expect(loadMoreTarget).not.toBeNull();

		mockState.intersectionCallback?.(
			[
				{
					isIntersecting: false,
					target: loadMoreTarget as Element,
				} as IntersectionObserverEntry,
			],
			{} as IntersectionObserver,
		);
		expect(mockState.search).toHaveBeenCalledTimes(1);

		mockState.intersectionCallback?.(
			[
				{
					isIntersecting: true,
					target: loadMoreTarget as Element,
				} as IntersectionObserverEntry,
			],
			{} as IntersectionObserver,
		);

		await waitFor(() => {
			expect(mockState.search).toHaveBeenCalledTimes(2);
		});
		expect(screen.getByText("report-1.txt")).toBeInTheDocument();
		expect(screen.queryByText("search:search_error")).not.toBeInTheDocument();
	});

	it("searches by category without requiring a keyword", async () => {
		const image = fileItem({
			id: 9,
			name: "cover.jpg",
			extension: "jpg",
			file_category: "image",
			mime_type: "image/jpeg",
		});
		mockState.search.mockResolvedValue({
			files: [image],
			folders: [],
			total_files: 1,
			total_folders: 0,
		});

		render(<GlobalSearchDialog open onOpenChange={vi.fn()} />);

		fireEvent.click(
			screen.getByRole("button", { name: "search:category_image" }),
		);
		await waitForSearchDebounce();

		await waitFor(() => {
			expect(mockState.search).toHaveBeenCalledWith(
				{
					type: "file",
					category: "image",
					limit: 10,
				},
				{ signal: expect.any(AbortSignal) },
			);
		});
		expect(await screen.findByText("cover.jpg")).toBeInTheDocument();
	});

	it("combines category filters with keyword searches", async () => {
		mockState.search.mockResolvedValue({
			files: [],
			folders: [],
			total_files: 0,
			total_folders: 0,
		});

		render(<GlobalSearchDialog open onOpenChange={vi.fn()} />);

		fireEvent.click(
			screen.getByRole("button", { name: "search:category_video" }),
		);
		fireEvent.change(screen.getByPlaceholderText("search:placeholder"), {
			target: { value: "clip" },
		});
		await waitForSearchDebounce();

		await waitFor(() => {
			expect(mockState.search).toHaveBeenLastCalledWith(
				{
					q: "clip",
					type: "file",
					category: "video",
					limit: 10,
				},
				{ signal: expect.any(AbortSignal) },
			);
		});
	});

	it("clears category-only searches when switching to folder results", async () => {
		mockState.search.mockResolvedValue({
			files: [],
			folders: [],
			total_files: 0,
			total_folders: 0,
		});

		render(<GlobalSearchDialog open onOpenChange={vi.fn()} />);

		fireEvent.click(
			screen.getByRole("button", { name: "search:category_image" }),
		);
		await waitForSearchDebounce();

		await waitFor(() => {
			expect(mockState.search).toHaveBeenCalledWith(
				{
					type: "file",
					category: "image",
					limit: 10,
				},
				{ signal: expect.any(AbortSignal) },
			);
		});

		fireEvent.click(
			screen.getByRole("button", { name: "search:folders_only" }),
		);

		await waitFor(() => {
			expect(
				screen.queryByRole("button", { name: "search:category_image" }),
			).not.toBeInTheDocument();
		});
		expect(screen.getByText("search:start_typing_desc")).toBeInTheDocument();
		expect(mockState.search).toHaveBeenCalledTimes(1);
	});

	it("keeps category filters when loading more results", async () => {
		const firstPageFile = fileItem({
			id: 7,
			name: "cover-1.jpg",
			extension: "jpg",
			file_category: "image",
			mime_type: "image/jpeg",
		});
		const secondPageFile = fileItem({
			id: 8,
			name: "cover-2.jpg",
			extension: "jpg",
			file_category: "image",
			mime_type: "image/jpeg",
		});

		mockState.search
			.mockResolvedValueOnce({
				files: [firstPageFile],
				folders: [],
				total_files: 2,
				total_folders: 0,
			})
			.mockResolvedValueOnce({
				files: [secondPageFile],
				folders: [],
				total_files: 2,
				total_folders: 0,
			});

		render(<GlobalSearchDialog open onOpenChange={vi.fn()} />);

		fireEvent.click(
			screen.getByRole("button", { name: "search:category_image" }),
		);
		await waitForSearchDebounce();
		expect(await screen.findByText("cover-1.jpg")).toBeInTheDocument();

		const loadMoreTarget = document.querySelector("[data-search-load-more]");
		expect(loadMoreTarget).not.toBeNull();
		expect(mockState.intersectionCallback).not.toBeNull();

		mockState.intersectionCallback?.(
			[
				{
					isIntersecting: true,
					target: loadMoreTarget as Element,
				} as IntersectionObserverEntry,
			],
			{} as IntersectionObserver,
		);

		await waitFor(() => {
			expect(mockState.search).toHaveBeenNthCalledWith(
				2,
				{
					type: "file",
					category: "image",
					limit: 10,
					offset: 1,
				},
				{ signal: expect.any(AbortSignal) },
			);
		});
		expect(await screen.findByText("cover-2.jpg")).toBeInTheDocument();
	});

	it("searches with tag filters and syncs tag library updates", async () => {
		mockState.listTags.mockResolvedValueOnce({
			items: [tag(1, "Alpha"), tag(2, "Beta", "#16a34a")],
			limit: 100,
			offset: 0,
			total: 2,
		});
		mockState.search.mockResolvedValue({
			files: [],
			folders: [],
			total_files: 0,
			total_folders: 0,
		});

		render(<GlobalSearchDialog open onOpenChange={vi.fn()} />);

		expect(
			await screen.findByRole("button", { name: /Alpha/ }),
		).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: /Alpha/ }));
		await waitForSearchDebounce();

		await waitFor(() => {
			expect(mockState.search).toHaveBeenLastCalledWith(
				{
					type: "all",
					tag_ids: "1",
					tag_match: "any",
					limit: 10,
				},
				{ signal: expect.any(AbortSignal) },
			);
		});

		fireEvent.click(screen.getByRole("button", { name: /Beta/ }));
		await waitForSearchDebounce();
		fireEvent.click(
			screen.getByRole("button", { name: "search:tag_match_any" }),
		);
		await waitForSearchDebounce();

		await waitFor(() => {
			expect(mockState.search).toHaveBeenLastCalledWith(
				{
					type: "all",
					tag_ids: "1,2",
					tag_match: "all",
					limit: 10,
				},
				{ signal: expect.any(AbortSignal) },
			);
		});

		fireEvent.click(
			screen.getByRole("button", { name: /files:tag_library_manage/ }),
		);
		fireEvent.click(screen.getByText("library-update-alpha"));
		expect(
			screen.getByRole("button", { name: /Alpha Prime/ }),
		).toBeInTheDocument();

		fireEvent.click(screen.getByText("library-delete-beta"));
		await waitForSearchDebounce();

		expect(
			screen.queryByRole("button", { name: /Beta/ }),
		).not.toBeInTheDocument();
		await waitFor(() => {
			expect(mockState.search).toHaveBeenLastCalledWith(
				{
					type: "all",
					tag_ids: "1",
					tag_match: "all",
					limit: 10,
				},
				{ signal: expect.any(AbortSignal) },
			);
		});
	});

	it("keeps an empty tag filter list when tag loading fails or is canceled", async () => {
		mockState.listTags.mockRejectedValueOnce(new Error("tag load failed"));

		const { rerender } = render(
			<GlobalSearchDialog open onOpenChange={vi.fn()} />,
		);

		await waitFor(() => {
			expect(screen.queryByText("search:tag_loading")).not.toBeInTheDocument();
		});
		expect(
			screen.queryByRole("button", { name: /Alpha/ }),
		).not.toBeInTheDocument();

		const aborted = new DOMException("aborted", "AbortError");
		mockState.listTags.mockRejectedValueOnce(aborted);
		rerender(<GlobalSearchDialog open={false} onOpenChange={vi.fn()} />);
		rerender(<GlobalSearchDialog open onOpenChange={vi.fn()} />);

		await waitFor(() => {
			expect(screen.queryByText("search:tag_loading")).not.toBeInTheDocument();
		});
	});

	it("applies an initial category preset when opened from quick views", async () => {
		mockState.search.mockResolvedValue({
			files: [],
			folders: [],
			total_files: 0,
			total_folders: 0,
		});

		render(
			<GlobalSearchDialog
				initialCategory="audio"
				open
				onOpenChange={vi.fn()}
			/>,
		);

		await waitForSearchDebounce();

		await waitFor(() => {
			expect(mockState.search).toHaveBeenCalledWith(
				{
					type: "file",
					category: "audio",
					limit: 10,
				},
				{ signal: expect.any(AbortSignal) },
			);
		});
	});

	it("opens folder results directly with a view transition", async () => {
		const onOpenChange = vi.fn();
		const folder: FolderListItem = {
			id: 3,
			is_locked: false,
			is_shared: false,
			name: "Reports",
			updated_at: "2026-04-15T12:00:00Z",
		};
		mockState.search.mockResolvedValue({
			files: [],
			folders: [folder],
			total_files: 0,
			total_folders: 1,
		});

		render(<GlobalSearchDialog open onOpenChange={onOpenChange} />);

		fireEvent.change(screen.getByPlaceholderText("search:placeholder"), {
			target: { value: "reports" },
		});
		await waitForSearchDebounce();
		expect(await screen.findByText("Reports")).toBeInTheDocument();

		fireEvent.click(screen.getByText("Reports"));

		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(mockState.navigate).toHaveBeenCalledWith("/folder/3?name=Reports", {
			viewTransition: false,
		});
	});

	it("updates the filter and opens the active file from keyboard navigation", async () => {
		const onOpenChange = vi.fn();
		const folder: FolderListItem = {
			id: 3,
			is_locked: false,
			is_shared: false,
			name: "Reports",
			updated_at: "2026-04-15T12:00:00Z",
		};
		const file = fileItem({
			id: 7,
			is_locked: true,
			name: "report.txt",
		});
		mockState.search.mockResolvedValue({
			files: [file],
			folders: [folder],
			total_files: 1,
			total_folders: 1,
		});
		mockState.getFile.mockResolvedValue({
			folder_id: 42,
		});

		render(<GlobalSearchDialog open onOpenChange={onOpenChange} />);

		const input = screen.getByPlaceholderText("search:placeholder");
		fireEvent.change(input, {
			target: { value: "report" },
		});
		await waitForSearchDebounce();
		expect(await screen.findByText("report.txt")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "search:files_only" }));
		await waitForSearchDebounce();

		await waitFor(() => {
			expect(mockState.search).toHaveBeenLastCalledWith(
				{
					q: "report",
					type: "file",
					limit: 10,
				},
				{ signal: expect.any(AbortSignal) },
			);
		});

		fireEvent.keyDown(input, { key: "ArrowUp" });
		fireEvent.keyDown(input, { key: "Enter" });

		await waitFor(() => {
			expect(mockState.getFile).toHaveBeenCalledWith(7);
		});
		expect(onOpenChange).toHaveBeenCalledWith(false);
		expect(mockState.navigate).toHaveBeenCalledWith("/folder/42", {
			state: {
				searchPreviewFile: file,
			},
			viewTransition: false,
		});
	});

	it("leaves keyboard navigation to the IME while the search input is composing", async () => {
		const onOpenChange = vi.fn();
		const folder: FolderListItem = {
			id: 3,
			is_locked: false,
			is_shared: false,
			name: "Reports",
			updated_at: "2026-04-15T12:00:00Z",
		};
		mockState.search.mockResolvedValue({
			files: [],
			folders: [folder],
			total_files: 0,
			total_folders: 1,
		});

		render(<GlobalSearchDialog open onOpenChange={onOpenChange} />);

		const input = screen.getByPlaceholderText("search:placeholder");
		fireEvent.change(input, {
			target: { value: "bao" },
		});
		await waitForSearchDebounce();
		expect(await screen.findByText("Reports")).toBeInTheDocument();

		fireEvent.compositionStart(input);
		fireEvent.keyDown(input, { key: "ArrowDown" });
		fireEvent.keyDown(input, { key: "Enter" });
		fireEvent.keyDown(input, { key: "Escape" });

		expect(onOpenChange).not.toHaveBeenCalled();
		expect(mockState.navigate).not.toHaveBeenCalled();
	});

	it("handles header close, input blur, and composition end events", async () => {
		const onOpenChange = vi.fn();
		mockState.search.mockResolvedValue({
			files: [],
			folders: [],
			total_files: 0,
			total_folders: 0,
		});

		render(<GlobalSearchDialog open onOpenChange={onOpenChange} />);

		const input = screen.getByPlaceholderText("search:placeholder");
		fireEvent.compositionStart(input);
		fireEvent.change(input, {
			target: { value: "report" },
		});
		fireEvent.compositionEnd(input);
		fireEvent.blur(input);
		await waitForSearchDebounce();

		await waitFor(() => {
			expect(mockState.search).toHaveBeenCalledWith(
				{
					q: "report",
					type: "all",
					limit: 10,
				},
				{ signal: expect.any(AbortSignal) },
			);
		});

		const closeIcon = screen
			.getAllByTestId("icon")
			.find((icon) => icon.getAttribute("data-name") === "X");
		expect(closeIcon).toBeDefined();
		fireEvent.click(closeIcon?.closest("button") as HTMLButtonElement);

		expect(onOpenChange).toHaveBeenCalledWith(false);
	});

	it("does not trap arrow keys when there are no results to navigate", () => {
		render(<GlobalSearchDialog open onOpenChange={vi.fn()} />);

		const allowed = fireEvent.keyDown(
			screen.getByPlaceholderText("search:placeholder"),
			{
				cancelable: true,
				key: "ArrowDown",
			},
		);

		expect(allowed).toBe(true);
	});

	it("shows an empty state when a query has no matches", async () => {
		mockState.search.mockResolvedValue({
			files: [],
			folders: [],
			total_files: 0,
			total_folders: 0,
		});

		render(<GlobalSearchDialog open onOpenChange={vi.fn()} />);

		fireEvent.change(screen.getByPlaceholderText("search:placeholder"), {
			target: { value: "unknown" },
		});
		await waitForSearchDebounce();
		await waitFor(() => {
			expect(mockState.search).toHaveBeenCalledTimes(1);
		});

		await waitFor(() => {
			expect(screen.queryByText("search:searching")).not.toBeInTheDocument();
			expect(screen.getByText("search:no_results")).toBeInTheDocument();
		});
	});

	it("shows a search error message when the request fails", async () => {
		mockState.search.mockRejectedValue(new Error("search exploded"));

		render(<GlobalSearchDialog open onOpenChange={vi.fn()} />);

		fireEvent.change(screen.getByPlaceholderText("search:placeholder"), {
			target: { value: "broken" },
		});
		await waitForSearchDebounce();
		await waitFor(() => {
			expect(mockState.search).toHaveBeenCalledTimes(1);
		});

		await waitFor(() => {
			expect(screen.getByText("search:search_error")).toBeInTheDocument();
		});
	});

	it("closes on escape and resets stale results when reopened", async () => {
		const onOpenChange = vi.fn();
		const file = fileItem({
			id: 7,
			name: "report.txt",
		});
		mockState.search.mockResolvedValue({
			files: [file],
			folders: [],
			total_files: 1,
			total_folders: 0,
		});

		const { rerender } = render(
			<GlobalSearchDialog open onOpenChange={onOpenChange} />,
		);

		const input = screen.getByPlaceholderText("search:placeholder");
		fireEvent.change(input, {
			target: { value: "report" },
		});
		await waitForSearchDebounce();
		expect(await screen.findByText("report.txt")).toBeInTheDocument();

		fireEvent.keyDown(input, { key: "Escape" });
		expect(onOpenChange).toHaveBeenCalledWith(false);

		rerender(<GlobalSearchDialog open={false} onOpenChange={onOpenChange} />);
		rerender(<GlobalSearchDialog open onOpenChange={onOpenChange} />);

		expect(screen.getByPlaceholderText("search:placeholder")).toHaveValue("");
		expect(screen.queryByText("report.txt")).not.toBeInTheDocument();
		expect(screen.getByText("search:start_typing_desc")).toBeInTheDocument();
	});

	it("ignores duplicate file opens while a result is already opening", async () => {
		const file = fileItem({
			id: 7,
			name: "report.txt",
		});
		let resolveFile: ((value: { folder_id: number }) => void) | undefined;
		mockState.search.mockResolvedValue({
			files: [file],
			folders: [],
			total_files: 1,
			total_folders: 0,
		});
		mockState.getFile.mockReturnValue(
			new Promise((resolve: (value: { folder_id: number }) => void) => {
				resolveFile = resolve;
			}),
		);

		render(<GlobalSearchDialog open onOpenChange={vi.fn()} />);

		fireEvent.change(screen.getByPlaceholderText("search:placeholder"), {
			target: { value: "report" },
		});
		await waitForSearchDebounce();
		expect(await screen.findByText("report.txt")).toBeInTheDocument();

		fireEvent.click(screen.getByText("report.txt"));

		await waitFor(() => {
			expect(mockState.getFile).toHaveBeenCalledTimes(1);
		});
		await waitFor(() => {
			expect(
				screen
					.getAllByTestId("icon")
					.some((icon) => icon.getAttribute("data-name") === "Spinner"),
			).toBe(true);
		});

		fireEvent.click(screen.getByText("report.txt"));
		expect(mockState.getFile).toHaveBeenCalledTimes(1);

		resolveFile?.({ folder_id: 42 });
		await waitFor(() => {
			expect(mockState.navigate).toHaveBeenCalledWith("/folder/42", {
				state: {
					searchPreviewFile: file,
				},
				viewTransition: false,
			});
		});
	});
});
