import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import MySharesPage from "@/pages/MySharesPage";
import { useUploadAreaControlsStore } from "@/stores/uploadAreaControlsStore";
import type { MyShareInfo } from "@/types/api";

const mockState = vi.hoisted(() => ({
	batchDelete: vi.fn(),
	deleteShare: vi.fn(),
	handleApiError: vi.fn(),
	listMine: vi.fn(),
	openWindow: vi.fn(),
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
	writeText: vi.fn(async () => undefined),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, opts?: Record<string, unknown>) => {
			if (key === "share:my_shares_delete_title") {
				return `${key}:${opts?.name}`;
			}
			if (key === "share:my_shares_batch_delete_title") {
				return `${key}:${opts?.count}`;
			}
			if (key === "share:my_shares_pagination_desc") {
				return `${key}:${opts?.current}/${opts?.total}/${opts?.count}`;
			}
			if (key === "core:selected_count") {
				return `${key}:${opts?.count}`;
			}
			if (opts?.date) return `${key}:${opts.date}`;
			return key;
		},
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		error: mockState.toastError,
		success: mockState.toastSuccess,
	},
}));

vi.mock("@/components/common/ConfirmDialog", () => ({
	ConfirmDialog: (props: {
		confirmLabel: string;
		description: string;
		onConfirm: () => void;
		onOpenChange: (open: boolean) => void;
		open: boolean;
		title: string;
	}) =>
		props.open ? (
			<div>
				<div>{props.title}</div>
				<div>{props.description}</div>
				<button type="button" onClick={props.onConfirm}>
					{props.confirmLabel}
				</button>
				<button type="button" onClick={() => props.onOpenChange(false)}>
					close
				</button>
			</div>
		) : null,
}));

vi.mock("@/components/common/EmptyState", () => ({
	EmptyState: (props: { description: string; title: string }) => (
		<div>{`${props.title}:${props.description}`}</div>
	),
}));

vi.mock("@/components/files/EditShareDialog", () => ({
	EditShareDialog: (props: { open: boolean; share: MyShareInfo | null }) =>
		props.open ? <div>{`edit-dialog:${props.share?.id ?? "none"}`}</div> : null,
}));

vi.mock("@/components/layout/AppLayout", () => ({
	AppLayout: (props: { children: React.ReactNode }) => (
		<div>{props.children}</div>
	),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: (props: { children: React.ReactNode }) => (
		<span>{props.children}</span>
	),
}));

vi.mock("@/components/files/FileTypeIcon", () => ({
	FileTypeIcon: (props: { fileName?: string; mimeType: string }) => (
		<span>{`file-icon:${props.fileName ?? ""}:${props.mimeType}`}</span>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: (props: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
	}) => (
		<button type="button" disabled={props.disabled} onClick={props.onClick}>
			{props.children}
		</button>
	),
}));

vi.mock("@/components/ui/card", () => ({
	Card: (props: {
		children: React.ReactNode;
		className?: string;
		onClick?: () => void;
		onKeyDown?: (event: React.KeyboardEvent) => void;
		role?: string;
		tabIndex?: number;
	}) => (
		// biome-ignore lint/a11y/noStaticElementInteractions: test mock mirrors the interactive card container
		<div
			className={props.className}
			onClick={props.onClick}
			onKeyDown={props.onKeyDown}
			role={props.role ?? "button"}
			tabIndex={props.tabIndex}
		>
			{props.children}
		</div>
	),
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
		onClick?: () => void;
	}) => (
		<button type="button" onClick={props.onClick}>
			{props.children}
		</button>
	),
	ContextMenuSeparator: () => <hr />,
	ContextMenuTrigger: (props: { children: React.ReactNode }) => (
		<div>{props.children}</div>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: (props: { name: string }) => <span>{`icon:${props.name}`}</span>,
}));

vi.mock("@/components/ui/item-checkbox", () => ({
	ItemCheckbox: (props: { checked: boolean; onChange: () => void }) => (
		<button
			type="button"
			data-testid="item-checkbox"
			aria-pressed={props.checked}
			onClick={props.onChange}
		>
			{props.checked ? "checked" : "unchecked"}
		</button>
	),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: mockState.handleApiError,
}));

vi.mock("@/lib/format", () => ({
	formatDateAbsolute: (value: string) => `fmt:${value}`,
}));

vi.mock("@/services/shareService", () => ({
	shareService: {
		batchDelete: mockState.batchDelete,
		delete: mockState.deleteShare,
		listMine: mockState.listMine,
		pagePath: (token: string) => `/s/${token}`,
		pageUrl: (token: string) => `${window.location.origin}/s/${token}`,
	},
}));

function createShare(overrides: Partial<MyShareInfo> = {}): MyShareInfo {
	return {
		id: 1,
		token: "token-1",
		resource_id: 1,
		resource_name: "Document.pdf",
		resource_type: "file",
		resource_deleted: false,
		status: "active",
		has_password: false,
		max_downloads: 0,
		download_count: 0,
		view_count: 0,
		remaining_downloads: null,
		created_at: "2026-03-28T00:00:00Z",
		expires_at: null,
		updated_at: "2026-03-28T00:00:00Z",
		...overrides,
	} as MyShareInfo;
}

describe("MySharesPage", () => {
	beforeEach(() => {
		mockState.batchDelete.mockReset();
		mockState.batchDelete.mockResolvedValue({
			succeeded: 2,
			failed: 0,
			errors: [],
		});
		mockState.deleteShare.mockReset();
		mockState.deleteShare.mockResolvedValue(undefined);
		mockState.handleApiError.mockReset();
		mockState.listMine.mockReset();
		mockState.openWindow.mockReset();
		mockState.toastError.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.writeText.mockReset();
		mockState.writeText.mockResolvedValue(undefined);
		useUploadAreaControlsStore.getState().setUploadPanelPresence({
			open: false,
			visible: false,
		});

		Object.defineProperty(navigator, "clipboard", {
			configurable: true,
			value: {
				writeText: mockState.writeText,
			},
		});

		Object.defineProperty(window, "open", {
			configurable: true,
			value: mockState.openWindow,
		});
	});

	it("loads shares, paginates forward, and returns to the previous page after deleting the last item", async () => {
		mockState.listMine
			.mockResolvedValueOnce({
				items: [
					createShare({ id: 1, resource_name: "Page One", token: "page-1" }),
				],
				total: 51,
			})
			.mockResolvedValueOnce({
				items: [
					createShare({ id: 51, resource_name: "Last Item", token: "page-2" }),
				],
				total: 51,
			})
			.mockResolvedValueOnce({
				items: [],
				total: 50,
			})
			.mockResolvedValueOnce({
				items: [
					createShare({ id: 1, resource_name: "Page One", token: "page-1" }),
				],
				total: 50,
			});

		render(<MySharesPage />);

		await screen.findByText("Page One");
		expect(mockState.listMine).toHaveBeenNthCalledWith(1, {
			limit: 50,
			offset: 0,
		});

		fireEvent.click(screen.getByText("share:my_shares_next"));

		await screen.findByText("Last Item");
		expect(mockState.listMine).toHaveBeenNthCalledWith(2, {
			limit: 50,
			offset: 50,
		});

		fireEvent.click(screen.getByText("share:my_shares_card_delete"));

		await screen.findByText("share:my_shares_delete_title:Last Item");
		fireEvent.click(screen.getByText("delete"));

		await waitFor(() => {
			expect(mockState.deleteShare).toHaveBeenCalledWith(51);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"share:my_shares_delete_success",
		);
		await waitFor(() => {
			expect(mockState.listMine).toHaveBeenNthCalledWith(4, {
				limit: 50,
				offset: 0,
			});
		});
	});

	it("copies and opens share links from card actions", async () => {
		mockState.listMine.mockResolvedValue({
			items: [
				createShare({
					id: 7,
					resource_name: "Doc.pdf",
					token: "token-doc",
				}),
			],
			total: 1,
		});

		render(<MySharesPage />);

		await screen.findByText("Doc.pdf");

		fireEvent.click(screen.getByText("share:my_shares_card_copy"));

		await waitFor(() => {
			expect(mockState.writeText).toHaveBeenCalledWith(
				"http://localhost:3000/s/token-doc",
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("copied_to_clipboard");

		fireEvent.click(screen.getByText("share:my_shares_card_open"));

		expect(mockState.openWindow).toHaveBeenCalledWith(
			"/s/token-doc",
			"_blank",
			"noopener,noreferrer",
		);
	});

	it("uses file-browser style icons for file and folder shares", async () => {
		mockState.listMine.mockResolvedValue({
			items: [
				createShare({
					id: 8,
					resource_name: "Document.pdf",
					resource_type: "file",
				}),
				createShare({
					id: 9,
					resource_name: "Projects",
					resource_type: "folder",
				}),
			],
			total: 2,
		});

		render(<MySharesPage />);

		await screen.findByText("Document.pdf");
		expect(screen.getByText("file-icon:Document.pdf:")).toBeInTheDocument();
		expect(screen.getByText("icon:Folder")).toBeInTheDocument();
	});

	it("supports batch deleting selected shares", async () => {
		mockState.listMine
			.mockResolvedValueOnce({
				items: [
					createShare({ id: 11, resource_name: "First", token: "first" }),
					createShare({ id: 12, resource_name: "Second", token: "second" }),
				],
				total: 2,
			})
			.mockResolvedValueOnce({
				items: [],
				total: 0,
			});

		render(<MySharesPage />);

		await screen.findByText("First");

		const checkboxes = screen.getAllByTestId("item-checkbox");
		fireEvent.click(checkboxes[0]);
		fireEvent.click(checkboxes[1]);

		await waitFor(() => {
			expect(
				screen.getAllByText("core:selected_count:2").length,
			).toBeGreaterThan(0);
		});
		expect(screen.getByTestId("my-shares-scroll-container")).toHaveClass(
			"pb-[calc(5.5rem+env(safe-area-inset-bottom))]",
		);
		fireEvent.click(screen.getByText("share:my_shares_batch_delete"));

		await screen.findByText("share:my_shares_batch_delete_title:2");
		fireEvent.click(screen.getByText("delete"));

		await waitFor(() => {
			expect(mockState.batchDelete).toHaveBeenCalledWith({
				share_ids: [11, 12],
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"share:my_shares_batch_delete_success",
		);
	});

	it("reserves bottom space while the upload panel is visible", async () => {
		useUploadAreaControlsStore.getState().setUploadPanelPresence({
			open: false,
			visible: true,
		});
		mockState.listMine.mockResolvedValue({
			items: [createShare({ id: 41, resource_name: "Upload Visible" })],
			total: 1,
		});

		render(<MySharesPage />);

		await screen.findByText("Upload Visible");

		expect(screen.getByTestId("my-shares-scroll-container")).toHaveClass(
			"pb-[calc(var(--bottom-right-activity-shell-height,0px)+2rem+env(safe-area-inset-bottom))]",
		);

		useUploadAreaControlsStore.getState().setUploadPanelPresence({
			open: true,
			visible: true,
		});

		await waitFor(() => {
			expect(screen.getByTestId("my-shares-scroll-container")).toHaveClass(
				"pb-[calc(var(--bottom-right-activity-shell-height,0px)+2rem+env(safe-area-inset-bottom))]",
			);
		});
	});

	it("opens the edit dialog from share actions", async () => {
		mockState.listMine.mockResolvedValue({
			items: [createShare({ id: 30, resource_name: "Editable" })],
			total: 1,
		});

		render(<MySharesPage />);

		await screen.findByText("Editable");
		fireEvent.click(screen.getByText("core:edit"));

		expect(screen.getByText("edit-dialog:30")).toBeInTheDocument();
	});
});
