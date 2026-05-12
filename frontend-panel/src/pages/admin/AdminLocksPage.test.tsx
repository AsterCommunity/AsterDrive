import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AdminLocksPage from "@/pages/admin/AdminLocksPage";
import type { LockPage, UserSummary } from "@/types/api";

const mockState = vi.hoisted(() => ({
	cleanupExpired: vi.fn(),
	forceUnlock: vi.fn(),
	handleApiError: vi.fn(),
	reload: vi.fn(),
	setItems: vi.fn(),
	toastSuccess: vi.fn(),
	useApiList: vi.fn(),
}));

function createUserSummary(): UserSummary {
	return {
		id: 8,
		username: "owner",
		profile: {
			display_name: "Owner",
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
		t: (key: string, options?: Record<string, unknown>) => {
			if (key === "expired_locks_cleaned") {
				return `expired_locks_cleaned:${options?.count}`;
			}
			return key;
		},
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/common/AdminTableList", () => ({
	AdminTableList: ({
		items,
		loading,
		emptyTitle,
		emptyDescription,
		headerRow,
		renderRow,
	}: {
		items: unknown[];
		loading: boolean;
		emptyTitle: string;
		emptyDescription: string;
		headerRow: React.ReactNode;
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

vi.mock("@/components/common/StatusBadge", () => ({
	StatusBadge: ({ status }: { status: string }) => (
		<span>{`status:${status}`}</span>
	),
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
		actions,
	}: {
		title: string;
		description: string;
		actions?: React.ReactNode;
	}) => (
		<div>
			<h1>{title}</h1>
			<p>{description}</p>
			<div>{actions}</div>
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
		children,
		className,
		disabled,
		onClick,
	}: {
		children: React.ReactNode;
		className?: string;
		disabled?: boolean;
		onClick?: () => void;
	}) => (
		<button
			type="button"
			className={className}
			disabled={disabled}
			onClick={onClick}
		>
			{children}
		</button>
	),
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

vi.mock("@/hooks/useApiList", () => ({
	useApiList: (...args: unknown[]) => mockState.useApiList(...args),
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
	adminLockService: {
		cleanupExpired: (...args: unknown[]) => mockState.cleanupExpired(...args),
		forceUnlock: (...args: unknown[]) => mockState.forceUnlock(...args),
		list: vi.fn(),
	},
}));

function createLock(
	overrides: Partial<LockPage["items"][number]> = {},
): LockPage["items"][number] {
	return {
		created_at: "2026-03-28T00:00:00Z",
		deep: false,
		entity_id: 42,
		entity_type: "file",
		id: 21,
		owner: createUserSummary(),
		owner_info: {
			kind: "text",
			value: "user@example.com",
		},
		path: "/docs/report.pdf",
		shared: false,
		timeout_at: null,
		...overrides,
	};
}

function renderPage() {
	return render(
		<MemoryRouter initialEntries={["/admin/locks"]}>
			<AdminLocksPage />
		</MemoryRouter>,
	);
}

describe("AdminLocksPage", () => {
	beforeEach(() => {
		mockState.cleanupExpired.mockReset();
		mockState.forceUnlock.mockReset();
		mockState.handleApiError.mockReset();
		mockState.reload.mockReset();
		mockState.setItems.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.useApiList.mockReset();

		mockState.cleanupExpired.mockResolvedValue({ removed: 2 });
		mockState.forceUnlock.mockResolvedValue(undefined);
		mockState.useApiList.mockReturnValue({
			items: [
				createLock(),
				createLock({
					id: 22,
					path: "/docs/expired.pdf",
					timeout_at: "2020-01-01T00:00:00Z",
					shared: true,
					deep: true,
				}),
			],
			loading: false,
			reload: mockState.reload,
			setItems: mockState.setItems,
		});
	});

	it("renders lock rows, statuses, and cleanup action", async () => {
		renderPage();

		expect(screen.getByText("webdav_locks")).toBeInTheDocument();
		expect(screen.getByText("locks_intro")).toBeInTheDocument();
		expect(screen.getByText("/docs/report.pdf")).toBeInTheDocument();
		expect(screen.getAllByText("user@example.com")).toHaveLength(2);
		expect(screen.getByText("exclusive")).toBeInTheDocument();
		expect(screen.getByText("shared_lock")).toBeInTheDocument();
		expect(screen.getByText("deep")).toBeInTheDocument();
		expect(screen.getByText("status:active")).toBeInTheDocument();
		expect(screen.getByText("status:expired")).toBeInTheDocument();
		expect(screen.getAllByText("date:2026-03-28T00:00:00Z")).toHaveLength(2);

		fireEvent.click(screen.getByRole("button", { name: "clean_expired" }));

		await waitFor(() => {
			expect(mockState.cleanupExpired).toHaveBeenCalledTimes(1);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"expired_locks_cleaned:2",
		);
		expect(mockState.reload).toHaveBeenCalledTimes(1);
	});

	it("force unlocks a lock after confirmation", async () => {
		renderPage();

		fireEvent.click(screen.getAllByRole("button", { name: "Trash" })[0]);

		expect(
			screen.getByText('Force unlock "/docs/report.pdf"?'),
		).toBeInTheDocument();
		expect(screen.getByText("force_unlock_desc")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "core:confirm" }));

		await waitFor(() => {
			expect(mockState.forceUnlock).toHaveBeenCalledWith(21);
		});
		expect(mockState.setItems).toHaveBeenCalledWith(expect.any(Function));
		expect(mockState.toastSuccess).toHaveBeenCalledWith("lock_released");
	});

	it("routes service failures through handleApiError", async () => {
		const error = new Error("unlock failed");
		mockState.forceUnlock.mockRejectedValueOnce(error);

		renderPage();

		fireEvent.click(screen.getAllByRole("button", { name: "Trash" })[0]);
		fireEvent.click(screen.getByRole("button", { name: "core:confirm" }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
	});
});
