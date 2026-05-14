import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setPublicSiteUrls } from "@/lib/publicSiteUrl";
import WebdavAccountsPage from "@/pages/WebdavAccountsPage";

const mockState = vi.hoisted(() => ({
	create: vi.fn(),
	fileListRoot: vi.fn(),
	handleApiError: vi.fn(),
	getSettings: vi.fn(),
	reload: vi.fn(),
	requestConfirm: vi.fn(),
	testConnection: vi.fn(),
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
	toggle: vi.fn(),
	useApiList: vi.fn(),
	writeText: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key.replace(/^core:/, ""),
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		error: (...args: unknown[]) => mockState.toastError(...args),
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
					<div
						key={String((item as { id?: number | string }).id ?? "webdav-item")}
					>
						{renderRow(item as never)}
					</div>
				))}
			</div>
		),
}));

vi.mock("@/components/common/ConfirmDialog", () => ({
	ConfirmDialog: () => null,
}));

vi.mock("@/components/layout/AppLayout", () => ({
	AppLayout: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
}));

vi.mock("@/components/ui/badge", () => ({
	Badge: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => <span className={className}>{children}</span>,
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		className,
		disabled,
		onClick,
		title,
	}: {
		children: React.ReactNode;
		className?: string;
		disabled?: boolean;
		onClick?: () => void;
		title?: string;
	}) => (
		<button
			type="button"
			className={className}
			disabled={disabled}
			onClick={onClick}
			title={title}
		>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/dialog", () => ({
	Dialog: ({ children, open }: { children: React.ReactNode; open: boolean }) =>
		open ? <div>{children}</div> : null,
	DialogContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogDescription: ({ children }: { children: React.ReactNode }) => (
		<p>{children}</p>
	),
	DialogFooter: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: () => <span aria-hidden="true" />,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({
		className,
		id,
		onChange,
		placeholder,
		readOnly,
		type,
		value,
	}: {
		className?: string;
		id?: string;
		onChange?: (event: { target: { value: string } }) => void;
		placeholder?: string;
		readOnly?: boolean;
		type?: string;
		value?: string;
	}) => (
		<input
			className={className}
			id={id}
			onChange={(event) =>
				onChange?.({ target: { value: event.target.value } })
			}
			placeholder={placeholder}
			readOnly={readOnly}
			type={type}
			value={value}
		/>
	),
}));

vi.mock("@/components/ui/label", () => ({
	Label: ({
		children,
		htmlFor,
	}: {
		children: React.ReactNode;
		htmlFor?: string;
	}) => <label htmlFor={htmlFor}>{children}</label>,
}));

vi.mock("@/components/ui/select", () => ({
	Select: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	SelectContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	SelectItem: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	SelectTrigger: ({
		children,
		className,
		id,
	}: {
		children: React.ReactNode;
		className?: string;
		id?: string;
	}) => (
		<div className={className} id={id}>
			{children}
		</div>
	),
	SelectValue: () => <span>select-value</span>,
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
	useConfirmDialog: () => ({
		requestConfirm: (...args: unknown[]) => mockState.requestConfirm(...args),
		dialogProps: {
			onConfirm: vi.fn(),
			onOpenChange: vi.fn(),
			open: false,
		},
	}),
}));

vi.mock("@/lib/format", () => ({
	formatDateShort: (value: string) => `date:${value}`,
}));

vi.mock("@/services/fileService", () => ({
	fileService: {
		listRoot: (...args: unknown[]) => mockState.fileListRoot(...args),
	},
}));

vi.mock("@/services/webdavAccountService", () => ({
	webdavAccountService: {
		create: (...args: unknown[]) => mockState.create(...args),
		delete: vi.fn(),
		list: vi.fn(),
		settings: (...args: unknown[]) => mockState.getSettings(...args),
		test: (...args: unknown[]) => mockState.testConnection(...args),
		toggle: (...args: unknown[]) => mockState.toggle(...args),
	},
}));

function createAccount(overrides: Record<string, unknown> = {}) {
	return {
		created_at: "2026-03-28T00:00:00Z",
		id: 11,
		is_active: true,
		root_folder_path: null,
		username: "dav-user",
		...overrides,
	};
}

describe("WebdavAccountsPage", () => {
	beforeEach(() => {
		mockState.create.mockReset();
		mockState.fileListRoot.mockReset();
		mockState.handleApiError.mockReset();
		mockState.getSettings.mockReset();
		mockState.reload.mockReset();
		mockState.requestConfirm.mockReset();
		mockState.testConnection.mockReset();
		mockState.toastError.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.toggle.mockReset();
		mockState.useApiList.mockReset();
		mockState.writeText.mockReset();

		mockState.fileListRoot.mockResolvedValue({
			folders: [{ id: 5, name: "Docs" }],
		});
		mockState.getSettings.mockResolvedValue({
			prefix: "/dav",
		});
		mockState.useApiList.mockReturnValue({
			items: [createAccount()],
			loading: false,
			reload: mockState.reload,
		});
		mockState.create.mockResolvedValue({
			password: "generated-pass",
			username: "dav-user",
		});
		mockState.testConnection.mockResolvedValue(undefined);
		mockState.toggle.mockResolvedValue(undefined);
		mockState.writeText.mockResolvedValue(undefined);
		setPublicSiteUrls(null);

		Object.defineProperty(navigator, "clipboard", {
			configurable: true,
			value: {
				writeText: mockState.writeText,
			},
		});
	});

	it("loads folders on mount, copies the endpoint, and toggles accounts", async () => {
		render(<WebdavAccountsPage />);

		await waitFor(() => {
			expect(mockState.fileListRoot).toHaveBeenCalledWith({
				file_limit: 0,
				folder_limit: 1000,
			});
		});
		await screen.findByDisplayValue("http://localhost:3000/dav/");
		expect(screen.getByText("dav-user")).toBeInTheDocument();
		expect(screen.getByText("date:2026-03-28T00:00:00Z")).toBeInTheDocument();

		fireEvent.click(screen.getByText("webdav:webdav_copy_endpoint"));

		await waitFor(() => {
			expect(mockState.writeText).toHaveBeenCalledWith(
				expect.stringContaining("/dav/"),
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("copied_to_clipboard");

		fireEvent.click(screen.getByTitle("disabled_status"));

		await waitFor(() => {
			expect(mockState.toggle).toHaveBeenCalledWith(11);
		});
		expect(mockState.reload).toHaveBeenCalledTimes(1);
	});

	it("uses the configured public site URL when copying the WebDAV endpoint", async () => {
		setPublicSiteUrls(["https://drive.example.com"]);

		render(<WebdavAccountsPage />);
		await screen.findByDisplayValue("https://drive.example.com/dav/");

		fireEvent.click(screen.getByText("webdav:webdav_copy_endpoint"));

		await waitFor(() => {
			expect(mockState.writeText).toHaveBeenCalledWith(
				"https://drive.example.com/dav/",
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("copied_to_clipboard");
	});

	it("creates accounts and tests the returned credentials", async () => {
		render(<WebdavAccountsPage />);

		fireEvent.click(
			screen.getByRole("button", { name: "webdav:create_webdav_account" }),
		);

		fireEvent.change(screen.getByLabelText("username"), {
			target: { value: "  dav-user  " },
		});
		fireEvent.change(screen.getByLabelText("password"), {
			target: { value: "secret" },
		});
		fireEvent.click(screen.getByRole("button", { name: "create" }));

		await waitFor(() => {
			expect(mockState.create).toHaveBeenCalledWith(
				"dav-user",
				"secret",
				undefined,
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"admin:webdav_account_created",
		);
		expect(mockState.reload).toHaveBeenCalledTimes(1);
		expect(screen.getByDisplayValue("dav-user")).toBeInTheDocument();
		expect(
			await screen.findByDisplayValue("generated-pass"),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "admin:test_connection" }),
		);

		await waitFor(() => {
			expect(mockState.testConnection).toHaveBeenCalledWith(
				"dav-user",
				"generated-pass",
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"admin:connection_success",
		);
		expect(screen.getByText("admin:connection_success")).toBeInTheDocument();
	});

	it("reports folder loading failures through handleApiError", async () => {
		const error = new Error("folder load failed");
		mockState.fileListRoot.mockRejectedValueOnce(error);

		render(<WebdavAccountsPage />);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
	});
});
