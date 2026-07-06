import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useEffect, useState } from "react";
import { MemoryRouter, useLocation } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AdminFilesPage from "@/pages/admin/AdminFilesPage";
import type {
	AdminFileBlobDetail,
	AdminFileBlobInfo,
	AdminFileDetail,
	AdminFileInfo,
	UserSummary,
} from "@/types/api";

const mockState = vi.hoisted(() => ({
	createBlobMaintenanceTask: vi.fn(),
	getBlob: vi.fn(),
	getFile: vi.fn(),
	handleApiError: vi.fn(),
	listBlobs: vi.fn(),
	listFiles: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) => {
			if (key === "page_size_option") return `size:${options?.count}`;
			return key;
		},
	}),
}));

vi.mock("@/components/admin/AdminOffsetPagination", () => ({
	AdminOffsetPagination: ({
		currentPage,
		nextDisabled,
		onNext,
		onPageSizeChange,
		onPrevious,
		pageSizeOptions,
		prevDisabled,
		total,
		totalPages,
	}: {
		currentPage: number;
		nextDisabled: boolean;
		onNext: () => void;
		onPageSizeChange: (value: string | null) => void;
		onPrevious: () => void;
		pageSizeOptions: Array<{ label: string; value: string }>;
		prevDisabled: boolean;
		total: number;
		totalPages: number;
	}) => (
		<div>
			<div>{`pagination:${currentPage}/${totalPages}/${total}`}</div>
			<button type="button" disabled={prevDisabled} onClick={onPrevious}>
				prev
			</button>
			<button type="button" disabled={nextDisabled} onClick={onNext}>
				next
			</button>
			{pageSizeOptions.map((option) => (
				<button
					key={option.value}
					type="button"
					onClick={() => onPageSizeChange(option.value)}
				>
					{option.label}
				</button>
			))}
		</div>
	),
}));

vi.mock("@/components/common/AdminTable", () => ({
	ADMIN_INTERACTIVE_TABLE_ROW_CLASS: "interactive-row",
	ADMIN_TABLE_BADGE_CELL_CLASS: "badge-cell",
	ADMIN_TABLE_MONO_TEXT_CLASS: "mono-cell",
	ADMIN_TABLE_STACKED_CELL_CLASS: "stacked-cell",
	ADMIN_TABLE_TEXT_CELL_CLASS: "text-cell",
	AdminSortableTableHead: ({
		children,
		onSortChange,
		sortKey,
	}: {
		children: React.ReactNode;
		onSortChange: (sortBy: string, sortOrder: "asc" | "desc") => void;
		sortKey: string;
	}) => (
		<th>
			<button type="button" onClick={() => onSortChange(sortKey, "asc")}>
				{children}
			</button>
		</th>
	),
	AdminTableCell: ({
		children,
		onClick,
	}: {
		children: React.ReactNode;
		onClick?: (event: { stopPropagation: () => void }) => void;
	}) => (
		<td
			onClick={() => onClick?.({ stopPropagation: vi.fn() })}
			onKeyDown={() => undefined}
		>
			{children}
		</td>
	),
	AdminTableHead: ({ children }: { children: React.ReactNode }) => (
		<th>{children}</th>
	),
	AdminTableHeader: ({ children }: { children: React.ReactNode }) => (
		<thead>{children}</thead>
	),
	AdminTableRow: ({
		children,
		onClick,
		onKeyDown,
		tabIndex,
	}: {
		children: React.ReactNode;
		onClick?: () => void;
		onKeyDown?: (event: {
			key: string;
			preventDefault: () => void;
			stopPropagation: () => void;
		}) => void;
		tabIndex?: number;
	}) => (
		<tr
			onClick={onClick}
			onKeyDown={(event) =>
				onKeyDown?.({
					key: event.key,
					preventDefault: vi.fn(),
					stopPropagation: vi.fn(),
				})
			}
			tabIndex={tabIndex}
		>
			{children}
		</tr>
	),
}));

vi.mock("@/components/common/AdminTableList", () => ({
	AdminTableList: ({
		emptyDescription,
		emptyTitle,
		headerRow,
		items,
		loading,
		renderRow,
	}: {
		emptyDescription: string;
		emptyTitle: string;
		headerRow: React.ReactNode;
		items: unknown[];
		loading: boolean;
		renderRow: (item: never) => React.ReactNode;
	}) =>
		loading ? (
			<div>loading</div>
		) : items.length === 0 ? (
			<div>{`${emptyTitle}:${emptyDescription}`}</div>
		) : (
			<table>
				{headerRow}
				<tbody>{items.map((item) => renderRow(item as never))}</tbody>
			</table>
		),
}));

vi.mock("@/components/common/ConfirmDialog", () => ({
	ConfirmDialog: ({
		confirmLabel,
		description,
		onConfirm,
		onOpenChange,
		open,
		title,
	}: {
		confirmLabel?: string;
		description?: string;
		onConfirm: () => void;
		onOpenChange?: (open: boolean) => void;
		open: boolean;
		title: string;
	}) =>
		open ? (
			<div>
				<h2>{title}</h2>
				{description ? <p>{description}</p> : null}
				<button type="button" onClick={() => onOpenChange?.(false)}>
					close-confirm
				</button>
				<button type="button" onClick={onConfirm}>
					{confirmLabel ?? "confirm"}
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/layout/AdminLayout", () => ({
	AdminLayout: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/layout/AdminPageHeader", () => ({
	AdminPageHeader: ({
		actions,
		description,
		title,
		toolbar,
	}: {
		actions?: React.ReactNode;
		description: string;
		title: string;
		toolbar?: React.ReactNode;
	}) => (
		<header>
			<h1>{title}</h1>
			<p>{description}</p>
			<div>{actions}</div>
			<div>{toolbar}</div>
		</header>
	),
}));

vi.mock("@/components/layout/AdminPageShell", () => ({
	AdminPageShell: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({ children }: { children: React.ReactNode; variant?: string }) => (
		<span>{children}</span>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
		type,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
		type?: "button" | "submit";
	}) => (
		<button type={type ?? "button"} disabled={disabled} onClick={onClick}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({
		children,
		onOpenChange,
		open,
	}: {
		children: React.ReactNode;
		onOpenChange?: (open: boolean) => void;
		open: boolean;
	}) =>
		open ? (
			<div>
				{children}
				<button type="button" onClick={() => onOpenChange?.(false)}>
					close
				</button>
			</div>
		) : null,
	DialogContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
}));

vi.mock("@/components/ui/dropdown-menu", () => ({
	DropdownMenu: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DropdownMenuContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DropdownMenuItem: ({
		children,
		onClick,
	}: {
		children: React.ReactNode;
		onClick?: () => void;
	}) => (
		<button type="button" onClick={onClick}>
			{children}
		</button>
	),
	DropdownMenuTrigger: ({ render }: { render: React.ReactNode }) => render,
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({
		className,
		onChange,
		placeholder,
		value,
	}: {
		className?: string;
		onChange?: (event: { target: { value: string } }) => void;
		placeholder?: string;
		value?: string;
	}) => (
		<input
			className={className}
			placeholder={placeholder}
			value={value}
			onChange={(event) =>
				onChange?.({ target: { value: event.target.value } })
			}
		/>
	),
}));

vi.mock("@/components/ui/select", () => {
	const { createContext, useContext } =
		require("react") as typeof import("react");

	const SelectContext = createContext<{
		onValueChange?: (value: string) => void;
	}>({});

	return {
		Select: ({
			children,
			onValueChange,
			value,
		}: {
			children: React.ReactNode;
			onValueChange?: (value: string) => void;
			value?: string;
		}) => (
			<SelectContext.Provider value={{ onValueChange }}>
				<div>
					<div>{`select:${value}`}</div>
					{children}
				</div>
			</SelectContext.Provider>
		),
		SelectContent: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
		SelectItem: ({
			children,
			value,
		}: {
			children: React.ReactNode;
			value: string;
		}) => {
			const context = useContext(SelectContext);

			return (
				<button
					type="button"
					aria-label={`select:${value}`}
					onClick={() => context.onValueChange?.(value)}
				>
					{children}
				</button>
			);
		},
		SelectTrigger: ({ children }: { children: React.ReactNode }) => (
			<div>{children}</div>
		),
		SelectValue: ({ children }: { children?: React.ReactNode }) => (
			<span>{children ?? "select-value"}</span>
		),
	};
});

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/hooks/useApiList", () => ({
	useApiList: (loader: () => Promise<{ items: unknown[]; total: number }>) => {
		const [items, setItems] = useState<unknown[]>([]);
		const [total, setTotal] = useState(0);
		const [loading, setLoading] = useState(true);

		useEffect(() => {
			let active = true;
			setLoading(true);
			void loader().then((page) => {
				if (!active) return;
				setItems(page.items);
				setTotal(page.total);
				setLoading(false);
			});
			return () => {
				active = false;
			};
		}, [loader]);

		return {
			items,
			loading,
			reload: async () => {
				const page = await loader();
				setItems(page.items);
				setTotal(page.total);
			},
			total,
		};
	},
}));

vi.mock("@/hooks/usePageTitle", () => ({
	usePageTitle: () => undefined,
}));

vi.mock("@/lib/format", () => ({
	formatBytes: (value: number) => `bytes:${value}`,
	formatDateAbsoluteWithOffset: (value: string) => `date:${value}`,
}));

vi.mock("@/services/adminService", () => ({
	adminFileService: {
		createBlobMaintenanceTask: (...args: unknown[]) =>
			mockState.createBlobMaintenanceTask(...args),
		getBlob: (...args: unknown[]) => mockState.getBlob(...args),
		getFile: (...args: unknown[]) => mockState.getFile(...args),
		listBlobs: (...args: unknown[]) => mockState.listBlobs(...args),
		listFiles: (...args: unknown[]) => mockState.listFiles(...args),
	},
}));

function createUserSummary(overrides: Partial<UserSummary> = {}): UserSummary {
	return {
		id: 1,
		profile: {
			avatar: {
				source: "none",
				url_1024: null,
				url_512: null,
				version: 0,
			},
			display_name: "Root User",
		},
		username: "root",
		...overrides,
	};
}

function createFile(overrides: Partial<AdminFileInfo> = {}): AdminFileInfo {
	return {
		blob: {
			hash: "abcdef1234567890abcdef",
			id: 9,
			policy_id: 3,
			size: 4096,
			storage_path: "ab/cd/abcdef",
		},
		blob_id: 9,
		compound_extension: null,
		created_at: "2026-05-01T00:00:00Z",
		created_by: createUserSummary(),
		created_by_user_id: 1,
		created_by_username: "root",
		deleted_at: null,
		extension: "txt",
		file_category: "text",
		folder_id: 2,
		id: 21,
		is_locked: false,
		mime_type: "text/plain",
		name: "report.txt",
		owner_user_id: 1,
		size: 4096,
		team_id: null,
		updated_at: "2026-05-01T01:00:00Z",
		...overrides,
	};
}

function createFileDetail(
	overrides: Partial<AdminFileDetail> = {},
): AdminFileDetail {
	return {
		...createFile(),
		versions: [
			{
				blob: {
					hash: "version-hash",
					id: 10,
					policy_id: 3,
					size: 1024,
					storage_path: "ve/rs/version-hash",
				},
				blob_id: 10,
				created_at: "2026-05-01T00:30:00Z",
				file_id: 21,
				id: 44,
				size: 1024,
				version: 2,
			},
		],
		...overrides,
	};
}

function createBlob(
	overrides: Partial<AdminFileBlobInfo> = {},
): AdminFileBlobInfo {
	return {
		actual_ref_count: 2,
		created_at: "2026-05-01T00:00:00Z",
		file_ref_count: 1,
		hash: "fedcba9876543210fedcba",
		hash_kind: "content_sha256",
		health: "healthy",
		id: 31,
		policy_id: 4,
		ref_count: 2,
		size: 8192,
		storage_path: "fe/dc/fedcba",
		thumbnail_path: null,
		thumbnail_processor: null,
		thumbnail_version: null,
		updated_at: "2026-05-01T01:00:00Z",
		uploader_count: 1,
		uploaders: [createUserSummary()],
		version_ref_count: 1,
		...overrides,
	};
}

function createBlobDetail(
	overrides: Partial<AdminFileBlobDetail> = {},
): AdminFileBlobDetail {
	return {
		...createBlob(),
		file_versions: [
			{
				created_at: "2026-05-01T00:20:00Z",
				file_id: 21,
				id: 45,
				size: 2048,
				version: 3,
			},
		],
		files: [
			{
				created_at: "2026-05-01T00:10:00Z",
				created_by: createUserSummary(),
				created_by_user_id: 1,
				created_by_username: "root",
				deleted_at: null,
				folder_id: 2,
				id: 21,
				mime_type: "text/plain",
				name: "report.txt",
				owner_user_id: 1,
				size: 4096,
				team_id: null,
				updated_at: "2026-05-01T01:00:00Z",
			},
		],
		...overrides,
	};
}

function LocationProbe() {
	const location = useLocation();

	return <div data-testid="location-search">{location.search}</div>;
}

function renderPage(kind: "files" | "blobs", initialEntry = "/admin/files") {
	return render(
		<MemoryRouter initialEntries={[initialEntry]}>
			<LocationProbe />
			<AdminFilesPage kind={kind} />
		</MemoryRouter>,
	);
}

describe("AdminFilesPage", () => {
	beforeEach(() => {
		mockState.createBlobMaintenanceTask.mockReset();
		mockState.getBlob.mockReset();
		mockState.getFile.mockReset();
		mockState.handleApiError.mockReset();
		mockState.listBlobs.mockReset();
		mockState.listFiles.mockReset();

		mockState.createBlobMaintenanceTask.mockResolvedValue({ id: 9001 });
		mockState.getBlob.mockResolvedValue(createBlobDetail());
		mockState.getFile.mockResolvedValue(createFileDetail());
		mockState.listBlobs.mockResolvedValue({
			items: [createBlob()],
			total: 1,
		});
		mockState.listFiles.mockResolvedValue({
			items: [createFile()],
			total: 1,
		});
	});

	it("lists files, syncs filters to the url, and opens file details", async () => {
		renderPage("files", "/admin/files?name=report&deleted=live");

		await waitFor(() => {
			expect(mockState.listFiles).toHaveBeenCalledWith({
				blob_id: undefined,
				deleted: false,
				limit: 20,
				name: "report",
				offset: 0,
				owner_user_id: undefined,
				policy_id: undefined,
				sort_by: "created_at",
				sort_order: "desc",
				team_id: undefined,
			});
		});
		expect(screen.getByText("report.txt")).toBeInTheDocument();
		expect(screen.getByText("bytes:4096")).toBeInTheDocument();
		expect(screen.getByText("abcdef1234...abcdef")).toBeInTheDocument();
		expect(screen.getByText("Root User")).toBeInTheDocument();
		expect(screen.getByText("@root")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: /show_filters/ }));
		expect(screen.getByText("select:live")).toBeInTheDocument();
		expect(screen.getAllByText("admin_deleted_live").length).toBeGreaterThan(0);
		expect(screen.queryByText("__all__")).not.toBeInTheDocument();

		fireEvent.change(screen.getByPlaceholderText("admin_policy_id"), {
			target: { value: "3" },
		});
		fireEvent.click(screen.getByRole("button", { name: "select:deleted" }));

		await waitFor(() => {
			expect(mockState.listFiles).toHaveBeenLastCalledWith({
				blob_id: undefined,
				deleted: true,
				limit: 20,
				name: "report",
				offset: 0,
				owner_user_id: undefined,
				policy_id: 3,
				sort_by: "created_at",
				sort_order: "desc",
				team_id: undefined,
			});
		});
		await waitFor(() => {
			expect(screen.getByTestId("location-search").textContent).toContain(
				"policyId=3",
			);
			expect(screen.getByTestId("location-search").textContent).toContain(
				"deleted=deleted",
			);
		});

		fireEvent.click(screen.getByText("report.txt"));

		await waitFor(() => {
			expect(mockState.getFile).toHaveBeenCalledWith(21);
		});
		expect(await screen.findByText("admin_file_versions")).toBeInTheDocument();
		expect(screen.getByText(/v2/)).toBeInTheDocument();
		expect(screen.getByText(/version-hash/)).toBeInTheDocument();
		expect(screen.getAllByText("Root User").length).toBeGreaterThan(1);
	});

	it("opens file details from keyboard activation", async () => {
		renderPage("files");

		const fileCell = await screen.findByText("report.txt");
		const row = fileCell.closest("tr");
		if (!row) {
			throw new Error("Expected file row");
		}
		fireEvent.keyDown(row, { key: "Enter" });

		await waitFor(() => {
			expect(mockState.getFile).toHaveBeenCalledWith(21);
		});
	});

	it("lists blobs with numeric filters and opens blob details", async () => {
		renderPage(
			"blobs",
			"/admin/file-blobs?hash=fedcba&refCountMin=1&sizeMax=9000",
		);

		await waitFor(() => {
			expect(mockState.listBlobs).toHaveBeenCalledWith({
				hash: "fedcba",
				limit: 20,
				offset: 0,
				policy_id: undefined,
				ref_count_max: undefined,
				ref_count_min: 1,
				size_max: 9000,
				size_min: undefined,
				sort_by: "created_at",
				sort_order: "desc",
				storage_path: undefined,
			});
		});
		expect(screen.getByText("fedcba9876...fedcba")).toBeInTheDocument();
		expect(screen.getByText("admin_hash_kind_content")).toBeInTheDocument();
		expect(screen.getByText("admin_blob_health_healthy")).toBeInTheDocument();
		expect(screen.getByText("Root User")).toBeInTheDocument();
		expect(
			screen.getByText("admin_blob_actual_ref_count_short"),
		).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: /show_filters/ }));
		fireEvent.change(screen.getByPlaceholderText("admin_storage_path"), {
			target: { value: "fe/dc" },
		});
		fireEvent.click(screen.getByText("clear_filters"));

		await waitFor(() => {
			expect(mockState.listBlobs).toHaveBeenLastCalledWith({
				hash: undefined,
				limit: 20,
				offset: 0,
				policy_id: undefined,
				ref_count_max: undefined,
				ref_count_min: undefined,
				size_max: undefined,
				size_min: undefined,
				sort_by: "created_at",
				sort_order: "desc",
				storage_path: undefined,
			});
		});

		fireEvent.click(screen.getByText("fedcba9876...fedcba"));

		await waitFor(() => {
			expect(mockState.getBlob).toHaveBeenCalledWith(31);
		});
		expect(await screen.findByText("Blob #31")).toBeInTheDocument();
		expect(screen.getByText("admin_blob_files")).toBeInTheDocument();
		expect(screen.getByText("admin_blob_versions")).toBeInTheDocument();
		expect(screen.getAllByText("Root User").length).toBeGreaterThan(1);
		expect(
			screen.getAllByText("admin_blob_health_healthy").length,
		).toBeGreaterThan(0);
		expect(screen.getByText("admin_actual_ref_count")).toBeInTheDocument();
		expect(screen.getAllByText(/#45/).length).toBeGreaterThan(0);
	});

	it("updates blob range filters, pagination, and detail dialog visibility", async () => {
		mockState.listBlobs
			.mockResolvedValueOnce({
				items: [createBlob()],
				total: 45,
			})
			.mockResolvedValue({
				items: [createBlob()],
				total: 45,
			});

		renderPage("blobs", "/admin/file-blobs?offset=20");

		await waitFor(() => {
			expect(mockState.listBlobs).toHaveBeenCalledWith({
				hash: undefined,
				limit: 20,
				offset: 20,
				policy_id: undefined,
				ref_count_max: undefined,
				ref_count_min: undefined,
				size_max: undefined,
				size_min: undefined,
				sort_by: "created_at",
				sort_order: "desc",
				storage_path: undefined,
			});
		});

		fireEvent.click(screen.getByRole("button", { name: /show_filters/ }));
		fireEvent.click(screen.getByText("next"));
		await waitFor(() => {
			expect(screen.getByTestId("location-search").textContent).toContain(
				"offset=40",
			);
		});

		fireEvent.change(screen.getByPlaceholderText("admin_ref_count_max"), {
			target: { value: "5" },
		});
		fireEvent.change(screen.getByPlaceholderText("admin_size_min"), {
			target: { value: "1024" },
		});

		await waitFor(() => {
			expect(mockState.listBlobs).toHaveBeenLastCalledWith({
				hash: undefined,
				limit: 20,
				offset: 0,
				policy_id: undefined,
				ref_count_max: 5,
				ref_count_min: undefined,
				size_max: undefined,
				size_min: 1024,
				sort_by: "created_at",
				sort_order: "desc",
				storage_path: undefined,
			});
		});

		await waitFor(() => {
			expect(screen.getByTestId("location-search").textContent).not.toContain(
				"offset=40",
			);
		});

		fireEvent.click(await screen.findByText("fedcba9876...fedcba"));
		expect(await screen.findByText("Blob #31")).toBeInTheDocument();
		fireEvent.click(screen.getByText("close"));
		await waitFor(() => {
			expect(screen.queryByText("Blob #31")).not.toBeInTheDocument();
		});
	});

	it("falls back from an empty page to the last available blob page", async () => {
		mockState.listBlobs
			.mockResolvedValueOnce({
				items: [],
				total: 21,
			})
			.mockResolvedValue({
				items: [createBlob()],
				total: 21,
			});

		renderPage("blobs", "/admin/file-blobs?offset=40");

		await waitFor(() => {
			expect(mockState.listBlobs).toHaveBeenCalledWith({
				hash: undefined,
				limit: 20,
				offset: 40,
				policy_id: undefined,
				ref_count_max: undefined,
				ref_count_min: undefined,
				size_max: undefined,
				size_min: undefined,
				sort_by: "created_at",
				sort_order: "desc",
				storage_path: undefined,
			});
		});

		await waitFor(() => {
			expect(mockState.listBlobs).toHaveBeenLastCalledWith({
				hash: undefined,
				limit: 20,
				offset: 20,
				policy_id: undefined,
				ref_count_max: undefined,
				ref_count_min: undefined,
				size_max: undefined,
				size_min: undefined,
				sort_by: "created_at",
				sort_order: "desc",
				storage_path: undefined,
			});
		});
		expect(screen.getByTestId("location-search").textContent).toContain(
			"offset=20",
		);
	});

	it("resets offset when changing file page size or sort", async () => {
		mockState.listFiles.mockResolvedValue({
			items: [createFile()],
			total: 65,
		});

		renderPage("files", "/admin/files?offset=20");

		await waitFor(() => {
			expect(mockState.listFiles).toHaveBeenCalledWith({
				blob_id: undefined,
				deleted: undefined,
				limit: 20,
				name: undefined,
				offset: 20,
				owner_user_id: undefined,
				policy_id: undefined,
				sort_by: "created_at",
				sort_order: "desc",
				team_id: undefined,
			});
		});

		fireEvent.click(screen.getByRole("button", { name: "size:50" }));

		await waitFor(() => {
			expect(mockState.listFiles).toHaveBeenLastCalledWith({
				blob_id: undefined,
				deleted: undefined,
				limit: 50,
				name: undefined,
				offset: 0,
				owner_user_id: undefined,
				policy_id: undefined,
				sort_by: "created_at",
				sort_order: "desc",
				team_id: undefined,
			});
		});
		await waitFor(() => {
			const search = screen.getByTestId("location-search").textContent ?? "";
			expect(search).toContain("pageSize=50");
			expect(search).not.toContain("offset=20");
		});

		fireEvent.click(screen.getByRole("button", { name: "core:name" }));

		await waitFor(() => {
			expect(mockState.listFiles).toHaveBeenLastCalledWith({
				blob_id: undefined,
				deleted: undefined,
				limit: 50,
				name: undefined,
				offset: 0,
				owner_user_id: undefined,
				policy_id: undefined,
				sort_by: "name",
				sort_order: "asc",
				team_id: undefined,
			});
		});
		await waitFor(() => {
			const search = screen.getByTestId("location-search").textContent ?? "";
			expect(search).toContain("pageSize=50");
			expect(search).toContain("sortBy=name");
			expect(search).toContain("sortOrder=asc");
			expect(search).not.toContain("offset=20");
		});
	});

	it("opens blob details from keyboard activation", async () => {
		renderPage("blobs");

		const blobCell = await screen.findByText("fedcba9876...fedcba");
		const row = blobCell.closest("tr");
		if (!row) {
			throw new Error("Expected blob row");
		}
		fireEvent.keyDown(row, { key: " " });

		await waitFor(() => {
			expect(mockState.getBlob).toHaveBeenCalledWith(31);
		});
	});

	it("renders every blob health state with reference counters", async () => {
		mockState.listBlobs.mockResolvedValue({
			items: [
				createBlob({
					actual_ref_count: 1,
					health: "healthy",
					id: 41,
					ref_count: 1,
				}),
				createBlob({
					actual_ref_count: 0,
					file_ref_count: 0,
					health: "orphan",
					id: 42,
					ref_count: 0,
					version_ref_count: 0,
				}),
				createBlob({
					actual_ref_count: 1,
					file_ref_count: 1,
					health: "ref_count_mismatch",
					id: 43,
					ref_count: 7,
					version_ref_count: 0,
				}),
				createBlob({
					actual_ref_count: 0,
					file_ref_count: 0,
					health: "cleanup_claimed",
					id: 44,
					ref_count: -1,
					version_ref_count: 0,
				}),
			],
			total: 4,
		});

		renderPage("blobs", "/admin/file-blobs");

		expect(
			await screen.findByText("admin_blob_health_healthy"),
		).toBeInTheDocument();
		expect(screen.getByText("admin_blob_health_orphan")).toBeInTheDocument();
		expect(
			screen.getByText("admin_blob_health_ref_count_mismatch"),
		).toBeInTheDocument();
		expect(
			screen.getByText("admin_blob_health_cleanup_claimed"),
		).toBeInTheDocument();
		expect(
			screen.getAllByText("admin_blob_actual_ref_count_short").length,
		).toBe(4);
	});

	it("shows blob detail reference counter boundaries", async () => {
		mockState.getBlob.mockResolvedValueOnce(
			createBlobDetail({
				actual_ref_count: 3,
				file_ref_count: 2,
				health: "ref_count_mismatch",
				ref_count: 9,
				uploader_count: 3,
				uploaders: [
					createUserSummary(),
					createUserSummary({
						id: 2,
						profile: {
							avatar: {
								source: "none",
								url_1024: null,
								url_512: null,
								version: 0,
							},
							display_name: "Second User",
						},
						username: "second",
					}),
				],
				version_ref_count: 1,
			}),
		);

		renderPage("blobs");
		fireEvent.click(await screen.findByText("fedcba9876...fedcba"));

		expect(await screen.findByText("Blob #31")).toBeInTheDocument();
		expect(screen.getByText("admin_actual_ref_count")).toBeInTheDocument();
		expect(screen.getByText("admin_file_ref_count")).toBeInTheDocument();
		expect(screen.getByText("admin_version_ref_count")).toBeInTheDocument();
		expect(
			screen.getAllByText("admin_blob_health_ref_count_mismatch").length,
		).toBeGreaterThan(0);
		expect(screen.getByText("9")).toBeInTheDocument();
		expect(screen.getByText("3")).toBeInTheDocument();
		expect(screen.getByText("+1")).toBeInTheDocument();
	});

	it("creates blob maintenance tasks from blob detail actions", async () => {
		renderPage("blobs");
		fireEvent.click(await screen.findByText("fedcba9876...fedcba"));

		expect(await screen.findByText("Blob #31")).toBeInTheDocument();
		fireEvent.click(
			screen.getAllByRole("button", {
				name: /admin_blob_action_integrity_check/,
			})[1],
		);

		await waitFor(() => {
			expect(mockState.createBlobMaintenanceTask).toHaveBeenCalledWith({
				action: "integrity_check",
				blob_ids: [31],
			});
		});
		expect(
			screen.getAllByRole("button", {
				name: /admin_blob_action_cleanup_orphan/,
			})[1],
		).toBeDisabled();
	});

	it("creates full blob maintenance tasks from the page action menu", async () => {
		renderPage("blobs");

		fireEvent.click(
			await screen.findByRole("button", {
				name: /admin_blob_action_reconcile_refs/,
			}),
		);
		expect(
			screen.getByText(
				"admin_blob_full_maintenance_confirm_desc_ref_count_reconcile",
			),
		).toBeInTheDocument();
		fireEvent.click(
			screen.getByRole("button", {
				name: "admin_blob_full_maintenance_confirm",
			}),
		);

		await waitFor(() => {
			expect(mockState.createBlobMaintenanceTask).toHaveBeenCalledWith({
				action: "ref_count_reconcile",
			});
		});
	});

	it("can cancel and confirm destructive full blob maintenance", async () => {
		renderPage("blobs");

		fireEvent.click(
			await screen.findByRole("button", {
				name: /admin_blob_action_cleanup_orphan/,
			}),
		);
		expect(
			screen.getByText(
				"admin_blob_full_maintenance_confirm_desc_orphan_cleanup",
			),
		).toBeInTheDocument();
		fireEvent.click(screen.getByText("close-confirm"));
		expect(
			screen.queryByText(
				"admin_blob_full_maintenance_confirm_desc_orphan_cleanup",
			),
		).not.toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", {
				name: /admin_blob_action_cleanup_orphan/,
			}),
		);
		fireEvent.click(
			screen.getByRole("button", {
				name: "admin_blob_full_maintenance_confirm",
			}),
		);

		await waitFor(() => {
			expect(mockState.createBlobMaintenanceTask).toHaveBeenCalledWith({
				action: "orphan_cleanup",
			});
		});
	});

	it("only enables orphan cleanup for orphan blobs", async () => {
		mockState.listBlobs.mockResolvedValue({
			items: [
				createBlob({
					actual_ref_count: 0,
					file_ref_count: 0,
					health: "orphan",
					id: 32,
					ref_count: 0,
					version_ref_count: 0,
				}),
			],
			total: 1,
		});
		mockState.getBlob.mockResolvedValueOnce(
			createBlobDetail({
				actual_ref_count: 0,
				file_ref_count: 0,
				health: "orphan",
				id: 32,
				ref_count: 0,
				version_ref_count: 0,
			}),
		);

		renderPage("blobs");
		const blobCell = await screen.findByText("fedcba9876...fedcba");
		fireEvent.click(blobCell);
		await waitFor(() => {
			expect(mockState.getBlob).toHaveBeenCalledWith(32);
		});
		expect(await screen.findByText("Blob #32")).toBeInTheDocument();

		const cleanupButton = (
			await screen.findAllByRole("button", {
				name: /admin_blob_action_cleanup_orphan/,
			})
		)[1];
		expect(cleanupButton).toBeEnabled();
		fireEvent.click(cleanupButton);

		await waitFor(() => {
			expect(mockState.createBlobMaintenanceTask).toHaveBeenCalledWith({
				action: "orphan_cleanup",
				blob_ids: [32],
			});
		});
	});

	it("reports blob maintenance action failures through the shared API error handler", async () => {
		const error = new Error("maintenance failed");
		mockState.createBlobMaintenanceTask.mockRejectedValueOnce(error);

		renderPage("blobs");
		fireEvent.click(await screen.findByText("fedcba9876...fedcba"));
		expect(await screen.findByText("Blob #31")).toBeInTheDocument();
		fireEvent.click(
			(
				await screen.findAllByRole("button", {
					name: /admin_blob_action_integrity_check/,
				})
			)[1],
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
	});

	it("reports detail loading failures through the shared API error handler", async () => {
		const error = new Error("detail failed");
		mockState.getFile.mockRejectedValueOnce(error);

		renderPage("files");

		fireEvent.click(await screen.findByText("report.txt"));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(screen.queryByText("admin_file_versions")).not.toBeInTheDocument();
	});
});
