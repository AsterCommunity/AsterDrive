import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { STORAGE_KEYS } from "@/config/app";
import TrashPage from "@/pages/TrashPage";
import { useUploadAreaControlsStore } from "@/stores/uploadAreaControlsStore";

const mockState = vi.hoisted(() => ({
	formatBatchToast: vi.fn((_: unknown, operation: string) => ({
		title: `toast:${operation}`,
		variant: "success",
	})),
	handleApiError: vi.fn(),
	list: vi.fn(),
	listeners: new Set<(event: unknown) => void>(),
	purgeAll: vi.fn(),
	purgeFile: vi.fn(),
	purgeFolder: vi.fn(),
	refreshUser: vi.fn(),
	restoreFile: vi.fn(),
	restoreFolder: vi.fn(),
	selectionShortcuts: vi.fn(),
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
}));

class MockIntersectionObserver {
	static instances: MockIntersectionObserver[] = [];

	disconnect = vi.fn();
	observe = vi.fn();
	root = null;
	rootMargin = "";
	thresholds: number[] = [];
	unobserve = vi.fn();

	private readonly callback: IntersectionObserverCallback;

	constructor(
		callback: IntersectionObserverCallback,
		options: IntersectionObserverInit = {},
	) {
		this.callback = callback;
		this.root = (options.root as Element | Document | null | undefined) ?? null;
		this.rootMargin = options.rootMargin ?? "";
		this.thresholds = Array.isArray(options.threshold)
			? options.threshold
			: options.threshold !== undefined
				? [options.threshold]
				: [];
		MockIntersectionObserver.instances.push(this);
	}

	takeRecords() {
		return [];
	}

	trigger(target: Element, isIntersecting = true) {
		this.callback(
			[
				{
					boundingClientRect: DOMRect.fromRect(),
					intersectionRatio: isIntersecting ? 1 : 0,
					intersectionRect: DOMRect.fromRect(),
					isIntersecting,
					rootBounds: null,
					target,
					time: 0,
				} as IntersectionObserverEntry,
			],
			this as unknown as IntersectionObserver,
		);
	}

	static reset() {
		MockIntersectionObserver.instances = [];
	}
}

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, opts?: Record<string, unknown>) => {
			if (key === "selected_count") return `selected:${opts?.count}`;
			if (key === "items_count") return `items:${opts?.count}`;
			if (key === "files:trash_purge_confirm_title") {
				return `purge-title:${opts?.count}`;
			}
			return key;
		},
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		error: (...args: unknown[]) => mockState.toastError(...args),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/common/ConfirmDialog", () => ({
	ConfirmDialog: ({
		open,
		title,
		description,
		confirmLabel,
		onConfirm,
		onOpenChange,
	}: {
		open: boolean;
		title: string;
		description: string;
		confirmLabel: string;
		onConfirm: () => void;
		onOpenChange: (open: boolean) => void;
	}) =>
		open ? (
			<div data-testid="confirm-dialog">
				<h2>{title}</h2>
				<p>{description}</p>
				<button type="button" onClick={onConfirm}>
					{confirmLabel}
				</button>
				<button type="button" onClick={() => onOpenChange(false)}>
					close-confirm
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/common/EmptyState", () => ({
	EmptyState: ({
		title,
		description,
	}: {
		title: string;
		description: string;
	}) => <div>{`${title}:${description}`}</div>,
}));

vi.mock("@/components/common/SkeletonFileGrid", () => ({
	SkeletonFileGrid: () => <div>skeleton-grid</div>,
}));

vi.mock("@/components/common/SkeletonFileTable", () => ({
	SkeletonFileTable: () => <div>skeleton-table</div>,
}));

vi.mock("@/components/common/ViewToggle", () => ({
	ViewToggle: ({
		value,
		onChange,
	}: {
		value: string;
		onChange: (value: "grid" | "list") => void;
	}) => (
		<div>
			<div>{`view:${value}`}</div>
			<button type="button" onClick={() => onChange("grid")}>
				grid
			</button>
			<button type="button" onClick={() => onChange("list")}>
				list
			</button>
		</div>
	),
}));

vi.mock("@/components/layout/AppLayout", () => ({
	AppLayout: ({ children }: { children: React.ReactNode }) => (
		<div data-testid="app-layout">{children}</div>
	),
}));

vi.mock("@/components/trash/TrashBatchActionBar", () => ({
	TrashBatchActionBar: ({
		count,
		onRestore,
		onPurge,
		onClearSelection,
	}: {
		count: number;
		onRestore: () => void;
		onPurge: () => void;
		onClearSelection: () => void;
	}) =>
		count > 0 ? (
			<div>
				<div>{`batch-count:${count}`}</div>
				<button type="button" onClick={onRestore}>
					restore-selected
				</button>
				<button type="button" onClick={onPurge}>
					purge-selected
				</button>
				<button type="button" onClick={onClearSelection}>
					clear-selection
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/trash/TrashGrid", () => ({
	TrashGrid: ({
		items,
		onToggleSelect,
		onRestore,
		onPurge,
	}: {
		items: Array<{ id: number; name: string }>;
		onToggleSelect: (item: never) => void;
		onRestore: (item: never) => void;
		onPurge: (item: never) => void;
	}) => (
		<div>
			{items.map((item) => (
				<div key={item.id}>
					<button type="button" onClick={() => onToggleSelect(item as never)}>
						{`select:${item.name}`}
					</button>
					<button type="button" onClick={() => onRestore(item as never)}>
						{`restore:${item.name}`}
					</button>
					<button type="button" onClick={() => onPurge(item as never)}>
						{`purge:${item.name}`}
					</button>
				</div>
			))}
		</div>
	),
}));

vi.mock("@/components/trash/TrashTable", () => ({
	TrashTable: ({
		items,
		onToggleSelectAll,
		onToggleSelect,
		onRestore,
		onPurge,
	}: {
		items: Array<{ id: number; name: string }>;
		onToggleSelectAll: () => void;
		onToggleSelect: (item: never) => void;
		onRestore: (item: never) => void;
		onPurge: (item: never) => void;
	}) => (
		<div>
			<button type="button" onClick={onToggleSelectAll}>
				toggle-all
			</button>
			{items.map((item) => (
				<div key={item.id}>
					<button type="button" onClick={() => onToggleSelect(item as never)}>
						{`select:${item.name}`}
					</button>
					<button type="button" onClick={() => onRestore(item as never)}>
						{`restore:${item.name}`}
					</button>
					<button type="button" onClick={() => onPurge(item as never)}>
						{`purge:${item.name}`}
					</button>
				</div>
			))}
		</div>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		type,
		disabled,
		onClick,
		className,
		title,
	}: {
		children: React.ReactNode;
		type?: "button" | "submit";
		disabled?: boolean;
		onClick?: () => void;
		className?: string;
		title?: string;
	}) => (
		<button
			type={type ?? "button"}
			disabled={disabled}
			onClick={onClick}
			className={className}
			title={title}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/item-checkbox", () => ({
	ItemCheckbox: ({
		checked,
		onChange,
	}: {
		checked: boolean;
		onChange: () => void;
	}) => (
		<button
			type="button"
			aria-label={`checkbox:${checked ? "checked" : "unchecked"}`}
			onClick={onChange}
		/>
	),
}));

vi.mock("@/components/ui/scroll-area", () => ({
	ScrollArea: ({
		children,
		className,
		viewportProps,
	}: {
		children: React.ReactNode;
		className?: string;
		viewportProps?: {
			className?: string;
		};
	}) => (
		<div
			className={className}
			data-viewport-class={viewportProps?.className ?? ""}
		>
			{children}
		</div>
	),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/hooks/useSelectionShortcuts", () => ({
	useSelectionShortcuts: (...args: unknown[]) =>
		mockState.selectionShortcuts(...args),
}));

vi.mock("@/lib/formatBatchToast", () => ({
	formatBatchToast: (...args: unknown[]) => mockState.formatBatchToast(...args),
}));

vi.mock("@/lib/storageChangeBus", () => ({
	subscribeStorageChange: (listener: (event: unknown) => void) => {
		mockState.listeners.add(listener);
		return () => {
			mockState.listeners.delete(listener);
		};
	},
}));

vi.mock("@/services/trashService", () => ({
	trashService: {
		list: (...args: unknown[]) => mockState.list(...args),
		purgeAll: (...args: unknown[]) => mockState.purgeAll(...args),
		purgeFile: (...args: unknown[]) => mockState.purgeFile(...args),
		purgeFolder: (...args: unknown[]) => mockState.purgeFolder(...args),
		restoreFile: (...args: unknown[]) => mockState.restoreFile(...args),
		restoreFolder: (...args: unknown[]) => mockState.restoreFolder(...args),
	},
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: (
		selector: (state: { refreshUser: typeof mockState.refreshUser }) => unknown,
	) => selector({ refreshUser: mockState.refreshUser }),
}));

const fileItem = {
	entity_type: "file",
	expires_at: "2026-04-08T00:00:00Z",
	id: 1,
	name: "report.pdf",
	original_path: "/Docs",
	size: 12,
} as never;

function emptyTrashContents() {
	return {
		files: [],
		files_total: 0,
		folders: [],
		folders_total: 0,
		next_file_cursor: null,
	} as never;
}

describe("TrashPage", () => {
	beforeEach(() => {
		localStorage.clear();
		mockState.formatBatchToast.mockClear();
		mockState.handleApiError.mockReset();
		mockState.list.mockReset();
		mockState.listeners.clear();
		mockState.purgeAll.mockReset();
		mockState.purgeFile.mockReset();
		mockState.purgeFolder.mockReset();
		mockState.refreshUser.mockReset();
		mockState.restoreFile.mockReset();
		mockState.restoreFolder.mockReset();
		mockState.selectionShortcuts.mockReset();
		mockState.toastError.mockReset();
		mockState.toastSuccess.mockReset();
		MockIntersectionObserver.reset();
		useUploadAreaControlsStore.getState().setUploadPanelPresence({
			open: false,
			visible: false,
		});

		mockState.list.mockResolvedValue(emptyTrashContents());
		mockState.purgeAll.mockResolvedValue({
			display_name: "Empty trash",
		});
		mockState.purgeFile.mockResolvedValue(undefined);
		mockState.purgeFolder.mockResolvedValue(undefined);
		mockState.refreshUser.mockResolvedValue(undefined);
		mockState.restoreFile.mockResolvedValue(undefined);
		mockState.restoreFolder.mockResolvedValue(undefined);
	});

	it("uses the stored grid preference and persists view mode changes", async () => {
		localStorage.setItem(STORAGE_KEYS.trashViewMode, "grid");

		render(<TrashPage />);

		expect(
			await screen.findByText("files:trash_empty_title:files:trash_empty_desc"),
		).toBeInTheDocument();
		expect(screen.getByText("view:grid")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "list" }));

		expect(localStorage.getItem(STORAGE_KEYS.trashViewMode)).toBe("list");
		expect(screen.getByText("view:list")).toBeInTheDocument();
	});

	it("restores selected items through the batch action bar and reloads the list", async () => {
		mockState.list
			.mockResolvedValueOnce({
				files: [fileItem],
				files_total: 1,
				folders: [],
				folders_total: 0,
				next_file_cursor: null,
			} as never)
			.mockResolvedValueOnce(emptyTrashContents());

		render(<TrashPage />);

		await screen.findByText("select:report.pdf");
		fireEvent.click(screen.getByRole("button", { name: "select:report.pdf" }));

		expect(screen.getByText("batch-count:1")).toBeInTheDocument();
		expect(
			screen.getByText("select:report.pdf").closest(".min-h-0"),
		).toHaveAttribute(
			"data-viewport-class",
			expect.stringContaining("pb-[calc(5.5rem"),
		);
		fireEvent.click(screen.getByRole("button", { name: "restore-selected" }));

		await waitFor(() => {
			expect(mockState.restoreFile).toHaveBeenCalledWith(1);
		});
		expect(mockState.formatBatchToast).toHaveBeenCalledWith(
			expect.any(Function),
			"restore",
			{
				errors: [],
				failed: 0,
				succeeded: 1,
			},
		);
		expect(mockState.toastSuccess).toHaveBeenCalledWith("toast:restore");
		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledTimes(2);
		});
		expect(mockState.refreshUser).not.toHaveBeenCalled();
	});

	it("reserves bottom space when the collapsed upload panel is visible", async () => {
		useUploadAreaControlsStore.getState().setUploadPanelPresence({
			open: false,
			visible: true,
		});
		mockState.list.mockResolvedValueOnce({
			files: [fileItem],
			files_total: 1,
			folders: [],
			folders_total: 0,
			next_file_cursor: null,
		} as never);

		render(<TrashPage />);

		await screen.findByText("select:report.pdf");

		expect(
			screen.getByText("select:report.pdf").closest(".min-h-0"),
		).toHaveAttribute(
			"data-viewport-class",
			expect.stringContaining("pb-[calc(7rem"),
		);
	});

	it("reserves expanded bottom space when the upload panel is open", async () => {
		useUploadAreaControlsStore.getState().setUploadPanelPresence({
			open: true,
			visible: true,
		});
		mockState.list.mockResolvedValueOnce({
			files: [fileItem],
			files_total: 1,
			folders: [],
			folders_total: 0,
			next_file_cursor: null,
		} as never);

		render(<TrashPage />);

		await screen.findByText("select:report.pdf");

		expect(
			screen.getByText("select:report.pdf").closest(".min-h-0"),
		).toHaveAttribute(
			"data-viewport-class",
			expect.stringContaining("pb-[calc(18rem"),
		);
	});

	it("confirms and schedules trash purge without reloading before completion", async () => {
		mockState.list
			.mockResolvedValueOnce({
				files: [fileItem],
				files_total: 1,
				folders: [],
				folders_total: 0,
				next_file_cursor: null,
			} as never)
			.mockResolvedValueOnce(emptyTrashContents());

		render(<TrashPage />);

		await screen.findByRole("button", { name: "select:report.pdf" });
		fireEvent.click(screen.getByRole("button", { name: /admin:empty_trash/i }));

		expect(await screen.findByText("are_you_sure")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "admin:empty_trash" }));

		await waitFor(() => {
			expect(mockState.purgeAll).toHaveBeenCalledTimes(1);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"tasks:task_created_success",
			{
				description: "Empty trash",
			},
		);
		expect(mockState.list).toHaveBeenCalledTimes(1);
		await waitFor(() => {
			expect(mockState.refreshUser).not.toHaveBeenCalled();
		});
	});

	it("reloads trash contents and quota when a sync.required event arrives", async () => {
		mockState.list
			.mockResolvedValueOnce({
				files: [fileItem],
				files_total: 1,
				folders: [],
				folders_total: 0,
				next_file_cursor: null,
			} as never)
			.mockResolvedValueOnce(emptyTrashContents());

		render(<TrashPage />);

		await screen.findByText("select:report.pdf");

		for (const listener of mockState.listeners) {
			listener({
				kind: "sync.required",
				workspace: { kind: "personal" },
				file_ids: [],
				folder_ids: [],
				affected_parent_ids: [],
				root_affected: false,
				affects_quota: true,
				storage_delta: null,
				at: "2026-05-19T00:00:00Z",
			});
		}

		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledTimes(2);
		});
		expect(mockState.refreshUser).toHaveBeenCalledWith({ fields: ["quota"] });
	});

	it("ignores storage events that do not require a full sync", async () => {
		mockState.list.mockResolvedValueOnce({
			files: [fileItem],
			files_total: 1,
			folders: [],
			folders_total: 0,
			next_file_cursor: null,
		} as never);

		render(<TrashPage />);

		await screen.findByText("select:report.pdf");

		for (const listener of mockState.listeners) {
			listener({
				kind: "file.trashed",
				workspace: { kind: "personal" },
				file_ids: [1],
				folder_ids: [],
				affected_parent_ids: [],
				root_affected: false,
				affects_quota: true,
				storage_delta: null,
				at: "2026-05-19T00:00:00Z",
			});
		}

		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledTimes(1);
		});
		expect(mockState.refreshUser).not.toHaveBeenCalled();
	});

	it("skips concurrent sync.required reloads while one is in flight", async () => {
		let resolveReload:
			| ((value: ReturnType<typeof emptyTrashContents>) => void)
			| null = null;
		mockState.list
			.mockResolvedValueOnce({
				files: [fileItem],
				files_total: 1,
				folders: [],
				folders_total: 0,
				next_file_cursor: null,
			} as never)
			.mockImplementationOnce(
				() =>
					new Promise((resolve) => {
						resolveReload = resolve;
					}) as never,
			);

		render(<TrashPage />);

		await screen.findByText("select:report.pdf");

		const event = {
			kind: "sync.required",
			workspace: { kind: "personal" },
			file_ids: [],
			folder_ids: [],
			affected_parent_ids: [],
			root_affected: false,
			affects_quota: true,
			storage_delta: null,
			at: "2026-05-19T00:00:00Z",
		};
		for (const listener of mockState.listeners) {
			listener(event);
			listener(event);
		}

		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledTimes(2);
		});
		expect(mockState.refreshUser).toHaveBeenCalledTimes(1);

		resolveReload?.(emptyTrashContents());

		await waitFor(() => {
			expect(screen.queryByText("select:report.pdf")).not.toBeInTheDocument();
		});
	});

	it("shows the server total even when only the first trash page is loaded", async () => {
		mockState.list.mockResolvedValueOnce({
			files: [fileItem],
			files_total: 150,
			folders: [],
			folders_total: 2,
			next_file_cursor: {
				expires_at: "2026-04-08T00:00:00Z",
				id: 1,
			},
		} as never);

		render(<TrashPage />);

		expect(await screen.findByText("items:152")).toBeInTheDocument();
		expect(screen.queryByText("items:1")).not.toBeInTheDocument();
	});

	it("loads additional folders when the first trash page has more folders but no more files", async () => {
		const originalIntersectionObserver = window.IntersectionObserver;
		Object.defineProperty(window, "IntersectionObserver", {
			writable: true,
			value: MockIntersectionObserver,
		});

		try {
			mockState.list
				.mockResolvedValueOnce({
					files: [],
					files_total: 0,
					folders: [
						{
							entity_type: "folder",
							expires_at: "2026-04-08T00:00:00Z",
							id: 11,
							name: "folder-a",
							original_path: "/",
						},
					],
					folders_total: 2,
					next_file_cursor: null,
				} as never)
				.mockResolvedValueOnce({
					files: [],
					files_total: 0,
					folders: [
						{
							entity_type: "folder",
							expires_at: "2026-04-07T00:00:00Z",
							id: 12,
							name: "folder-b",
							original_path: "/",
						},
					],
					folders_total: 2,
					next_file_cursor: null,
				} as never);

			render(<TrashPage />);

			await screen.findByText("select:folder-a");
			await waitFor(() => {
				expect(MockIntersectionObserver.instances).toHaveLength(1);
			});

			const observer = MockIntersectionObserver.instances[0];
			const target = observer?.observe.mock.calls[0]?.[0] as
				| Element
				| undefined;
			expect(target).toBeInstanceOf(HTMLElement);

			if (observer && target) {
				observer.trigger(target);
			}

			await screen.findByText("select:folder-b");
			expect(mockState.list).toHaveBeenLastCalledWith({
				file_after_expires_at: undefined,
				file_after_id: undefined,
				file_limit: 0,
				folder_limit: 1000,
				folder_offset: 1,
			});
		} finally {
			Object.defineProperty(window, "IntersectionObserver", {
				writable: true,
				value: originalIntersectionObserver,
			});
		}
	});
});
