import {
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { useState } from "react";
import { MemoryRouter, useLocation } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AdminSharesPage from "@/pages/admin/AdminSharesPage";
import type { ShareInfo, UserSummary } from "@/types/api";

const mockState = vi.hoisted(() => ({
	deleteShare: vi.fn(),
	handleApiError: vi.fn(),
	list: vi.fn(),
	toastSuccess: vi.fn(),
}));

function createUserSummary(): UserSummary {
	return {
		id: 9,
		username: "root",
		profile: {
			display_name: "Root",
			avatar: {
				source: "none",
				url_1024: null,
				url_512: null,
				version: 0,
			},
		},
	};
}

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/admin/AdminOffsetPagination", () => ({
	AdminOffsetPagination: ({
		currentPage,
		nextDisabled,
		onNext,
		onPageSizeChange,
		onPrevious,
		pageSize,
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
		pageSize: string;
		pageSizeOptions: Array<{ label: string; value: string }>;
		prevDisabled: boolean;
		total: number;
		totalPages: number;
	}) => (
		<div>
			<div>{`pagination:${currentPage}/${totalPages}:${pageSize}:${total}`}</div>
			<button type="button" onClick={onPrevious} disabled={prevDisabled}>
				CaretLeft
			</button>
			<button type="button" onClick={onNext} disabled={nextDisabled}>
				CaretRight
			</button>
			<select
				data-testid="page-size"
				value={pageSize}
				onChange={(event) => onPageSizeChange(event.target.value)}
			>
				{pageSizeOptions.map((option) => (
					<option key={option.value} value={option.value}>
						{option.label}
					</option>
				))}
			</select>
		</div>
	),
}));

vi.mock("@/components/common/AdminTableList", () => ({
	AdminTableList: ({
		items,
		loading,
		emptyTitle,
		emptyDescription,
		headerRow,
		pagination,
		renderRow,
	}: {
		items: unknown[];
		loading: boolean;
		emptyTitle: string;
		emptyDescription: string;
		headerRow: React.ReactNode;
		pagination?: React.ReactNode;
		renderRow: (item: never) => React.ReactNode;
	}) =>
		loading ? (
			<div>loading</div>
		) : items.length === 0 ? (
			<div>{`${emptyTitle}:${emptyDescription}`}</div>
		) : (
			<div>
				{headerRow}
				{items.map((item) => (
					<div key={String((item as { id: number }).id)}>
						{renderRow(item as never)}
					</div>
				))}
				{pagination}
			</div>
		),
}));

vi.mock("@/components/common/ConfirmDialog", () => ({
	ConfirmDialog: ({
		open,
		title,
		description,
		confirmLabel,
		onConfirm,
	}: {
		open: boolean;
		title: string;
		description: string;
		confirmLabel: string;
		onConfirm: () => void;
	}) =>
		open ? (
			<div>
				<div>{title}</div>
				<div>{description}</div>
				<button type="button" onClick={onConfirm}>
					{confirmLabel}
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
		title,
		description,
	}: {
		title: string;
		description: string;
	}) => (
		<div>
			<h1>{title}</h1>
			<p>{description}</p>
		</div>
	),
}));

vi.mock("@/components/layout/AdminPageShell", () => ({
	AdminPageShell: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({ children }: { children: React.ReactNode }) => (
		<span>{children}</span>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		"aria-label": ariaLabel,
		children,
		className,
		disabled,
		onClick,
		title,
	}: {
		"aria-label"?: string;
		children: React.ReactNode;
		className?: string;
		disabled?: boolean;
		onClick?: () => void;
		title?: string;
	}) => (
		<button
			type="button"
			aria-label={ariaLabel}
			className={className}
			disabled={disabled}
			onClick={onClick}
			title={title}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/select", () => ({
	Select: ({ children }: { children: React.ReactNode }) => <>{children}</>,
	SelectContent: ({ children }: { children: React.ReactNode }) => (
		<>{children}</>
	),
	SelectItem: ({ children }: { children: React.ReactNode }) => (
		<option>{children}</option>
	),
	SelectTrigger: ({ children }: { children: React.ReactNode }) => (
		<>{children}</>
	),
	SelectValue: () => null,
}));

vi.mock("@/components/ui/tooltip", () => ({
	Tooltip: ({ children }: { children: React.ReactNode }) => <>{children}</>,
	TooltipContent: ({ children }: { children: React.ReactNode }) => (
		<>{children}</>
	),
	TooltipProvider: ({ children }: { children: React.ReactNode }) => (
		<>{children}</>
	),
	TooltipTrigger: ({
		children,
		render,
	}: {
		children?: React.ReactNode;
		render?: React.ReactNode;
	}) => <>{render ?? children}</>,
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/table", () => ({
	TableCell: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	TableHead: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	TableHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	TableRow: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/hooks/useConfirmDialog", () => ({
	useConfirmDialog: (handler: (id: number) => Promise<void>) => {
		const [confirmId, setConfirmId] = useState<number | null>(null);

		return {
			confirmId,
			requestConfirm: (id: number) => setConfirmId(id),
			dialogProps: {
				open: confirmId !== null,
				onConfirm: () => {
					if (confirmId !== null) {
						void handler(confirmId);
					}
				},
				onOpenChange: (open: boolean) => {
					if (!open) setConfirmId(null);
				},
			},
		};
	},
}));

vi.mock("@/lib/format", () => ({
	formatDateShort: (value: string) => `date:${value}`,
}));

vi.mock("@/services/adminService", () => ({
	adminShareService: {
		delete: (...args: unknown[]) => mockState.deleteShare(...args),
		list: (...args: unknown[]) => mockState.list(...args),
	},
}));

function createShare(overrides: Partial<ShareInfo> = {}): ShareInfo {
	return {
		created_at: "2026-03-28T00:00:00Z",
		download_count: 1,
		expires_at: null,
		id: 11,
		max_downloads: 0,
		target: { type: "file", id: 5 },
		team_id: null,
		token: "share-token",
		updated_at: "2026-03-28T00:00:00Z",
		user: createUserSummary(),
		view_count: 0,
		...overrides,
	};
}

function renderPage(initialEntry = "/admin/shares") {
	return render(
		<MemoryRouter initialEntries={[initialEntry]}>
			<LocationProbe />
			<AdminSharesPage />
		</MemoryRouter>,
	);
}

function LocationProbe() {
	const location = useLocation();

	return <div data-testid="location-search">{location.search}</div>;
}

describe("AdminSharesPage", () => {
	beforeEach(() => {
		mockState.deleteShare.mockReset();
		mockState.handleApiError.mockReset();
		mockState.list.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.deleteShare.mockResolvedValue(undefined);
	});

	it("loads the first page with the default pagination state", async () => {
		mockState.list.mockResolvedValueOnce({
			items: [
				createShare(),
				createShare({
					id: 12,
					token: "expired-token",
					expires_at: "2020-01-01T00:00:00Z",
				}),
				createShare({
					id: 13,
					token: "limited-token",
					max_downloads: 1,
				}),
			],
			total: 25,
		});

		renderPage();

		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledWith({
				limit: 20,
				offset: 0,
				sort_by: "created_at",
				sort_order: "desc",
			});
		});

		expect(screen.getByText("shares")).toBeInTheDocument();
		expect(screen.getByText("shares_intro")).toBeInTheDocument();
		expect(screen.getByText("pagination:1/2:20:25")).toBeInTheDocument();
		expect(screen.getByText("share-token")).toBeInTheDocument();
		expect(screen.getByRole("link", { name: /share-token/ })).toHaveAttribute(
			"href",
			"/s/share-token",
		);
		expect(screen.getAllByText("core:active")).toHaveLength(1);
		expect(screen.getByText("core:expired")).toBeInTheDocument();
		expect(screen.getByText("limit_reached")).toBeInTheDocument();
		expect(screen.getByText("1 / 1")).toBeInTheDocument();
		expect(screen.getAllByText("date:2026-03-28T00:00:00Z")).toHaveLength(3);
		expect(screen.getByTestId("location-search").textContent).toBe("");
	});

	it("deletes a share after confirmation and updates the list", async () => {
		mockState.list
			.mockResolvedValueOnce({
				items: [
					createShare({
						id: 21,
						token: "page-two-share",
					}),
				],
				total: 21,
			})
			.mockResolvedValueOnce({
				items: [
					createShare({
						id: 1,
						token: "page-one-share",
					}),
				],
				total: 20,
			});

		renderPage("/admin/shares?offset=20&pageSize=20");

		await waitFor(() => {
			expect(mockState.list).toHaveBeenCalledWith({
				limit: 20,
				offset: 20,
				sort_by: "created_at",
				sort_order: "desc",
			});
		});

		fireEvent.click(screen.getAllByRole("button", { name: "core:delete" })[0]);

		expect(
			screen.getByText('core:delete "page-two-share"?'),
		).toBeInTheDocument();
		expect(screen.getByText("delete_share_desc")).toBeInTheDocument();

		fireEvent.click(
			within(
				screen.getByText('core:delete "page-two-share"?')
					.parentElement as HTMLElement,
			).getByRole("button", { name: "core:delete" }),
		);

		await waitFor(() => {
			expect(mockState.deleteShare).toHaveBeenCalledWith(21);
		});
		await waitFor(() => {
			expect(mockState.list).toHaveBeenLastCalledWith({
				limit: 20,
				offset: 0,
				sort_by: "created_at",
				sort_order: "desc",
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("share_deleted");
		await waitFor(() => {
			expect(screen.getByTestId("location-search").textContent).toBe("");
		});
	});

	it("routes delete failures through handleApiError", async () => {
		const error = new Error("delete failed");
		mockState.deleteShare.mockRejectedValueOnce(error);

		mockState.list.mockResolvedValueOnce({
			items: [createShare()],
			total: 1,
		});

		renderPage();

		await screen.findByText("share-token");

		fireEvent.click(screen.getAllByRole("button", { name: "core:delete" })[0]);
		fireEvent.click(
			within(
				screen.getByText('core:delete "share-token"?')
					.parentElement as HTMLElement,
			).getByRole("button", { name: "core:delete" }),
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
	});

	it("shows a deleting state while share deletion is pending", async () => {
		let resolveDelete: (() => void) | null = null;
		mockState.list.mockResolvedValueOnce({
			items: [createShare()],
			total: 1,
		});
		mockState.deleteShare.mockImplementationOnce(
			() =>
				new Promise<void>((resolve) => {
					resolveDelete = resolve;
				}),
		);

		renderPage();

		await screen.findByText("share-token");
		fireEvent.click(screen.getAllByRole("button", { name: "core:delete" })[0]);
		fireEvent.click(
			within(
				screen.getByText('core:delete "share-token"?')
					.parentElement as HTMLElement,
			).getByRole("button", { name: "core:delete" }),
		);

		await waitFor(() => {
			expect(
				screen.getByRole("button", { name: "share_deleting" }),
			).toBeDisabled();
		});

		resolveDelete?.();
		await waitFor(() => {
			expect(mockState.toastSuccess).toHaveBeenCalledWith("share_deleted");
		});
	});

	it("syncs offset and page size into the url and refetches when pagination changes", async () => {
		mockState.list
			.mockResolvedValueOnce({
				items: [createShare({ id: 1, token: "first-page" })],
				total: 25,
			})
			.mockResolvedValueOnce({
				items: [createShare({ id: 21, token: "second-page" })],
				total: 25,
			})
			.mockResolvedValueOnce({
				items: [createShare({ id: 1, token: "page-size-change" })],
				total: 25,
			});

		renderPage();

		await waitFor(() => {
			expect(mockState.list).toHaveBeenNthCalledWith(1, {
				limit: 20,
				offset: 0,
				sort_by: "created_at",
				sort_order: "desc",
			});
		});

		fireEvent.click(screen.getByRole("button", { name: "CaretRight" }));

		await waitFor(() => {
			expect(mockState.list).toHaveBeenNthCalledWith(2, {
				limit: 20,
				offset: 20,
				sort_by: "created_at",
				sort_order: "desc",
			});
		});
		expect(screen.getByTestId("location-search").textContent).toBe(
			"?offset=20",
		);

		fireEvent.change(screen.getByTestId("page-size"), {
			target: { value: "50" },
		});

		await waitFor(() => {
			expect(mockState.list).toHaveBeenNthCalledWith(3, {
				limit: 50,
				offset: 0,
				sort_by: "created_at",
				sort_order: "desc",
			});
		});
		expect(screen.getByTestId("location-search").textContent).toBe(
			"?pageSize=50",
		);
	});
});
