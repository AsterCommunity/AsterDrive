import {
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invalidateAdminRemoteNodeLookup } from "@/lib/adminRemoteNodeLookup";
import AdminPoliciesPage from "@/pages/admin/AdminPoliciesPage";
import { ApiError } from "@/services/http";
import { ApiSubcode } from "@/types/api-helpers";

const mockState = vi.hoisted(() => ({
	create: vi.fn(),
	deletePolicy: vi.fn(),
	handleApiError: vi.fn(),
	items: [] as Array<Record<string, unknown>>,
	listRemoteNodes: vi.fn(),
	loading: false,
	reload: vi.fn(),
	remoteNodes: [] as Array<Record<string, unknown>>,
	searchParams: "",
	setSearchParams: vi.fn(),
	testConnection: vi.fn(),
	testParams: vi.fn(),
	total: 0,
	toastSuccess: vi.fn(),
	update: vi.fn(),
}));

vi.mock("react-router-dom", () => ({
	useSearchParams: () => [
		new URLSearchParams(mockState.searchParams),
		mockState.setSearchParams,
	],
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => {
			switch (key) {
				case "driver_type_local":
					return "Local";
				case "driver_type_s3":
					return "S3";
				case "driver_type_remote":
					return "Remote";
				case "access_key":
					return "Access Key";
				case "secret_key":
					return "Secret Key";
				default:
					return key;
			}
		},
	}),
	initReactI18next: {
		type: "3rdParty",
		init: () => undefined,
	},
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
			<table>
				{headerRow}
				<tbody>{items.map((item) => renderRow(item as never))}</tbody>
			</table>
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
		description?: string;
		confirmLabel?: string;
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
	Badge: ({
		children,
		className,
		"data-testid": dataTestId,
		variant,
	}: {
		children: React.ReactNode;
		className?: string;
		"data-testid"?: string;
		variant?: string;
	}) => (
		<span className={className} data-testid={dataTestId} data-variant={variant}>
			{children}
		</span>
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
		type,
		variant,
	}: {
		"aria-label"?: string;
		children: React.ReactNode;
		className?: string;
		disabled?: boolean;
		onClick?: () => void;
		title?: string;
		type?: "button" | "submit";
		variant?: string;
	}) => (
		<button
			type={type ?? "button"}
			aria-label={ariaLabel}
			className={className}
			data-variant={variant}
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
	DialogContent: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => (
		<div data-testid="dialog-content" className={className}>
			{children}
		</div>
	),
	DialogDescription: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogFooter: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => (
		<div data-testid="dialog-footer" className={className}>
			{children}
		</div>
	),
	DialogHeader: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	DialogTitle: ({ children }: { children: React.ReactNode }) => (
		<h2>{children}</h2>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/ui/input", () => ({
	Input: ({
		"aria-invalid": ariaInvalid,
		autoComplete,
		className,
		id,
		onChange,
		onBlur,
		placeholder,
		required,
		type,
		value,
	}: {
		"aria-invalid"?: boolean;
		autoComplete?: string;
		className?: string;
		id?: string;
		onChange?: (event: { target: { value: string } }) => void;
		onBlur?: () => void;
		placeholder?: string;
		required?: boolean;
		type?: string;
		value?: string;
	}) => (
		<input
			aria-invalid={ariaInvalid}
			autoComplete={autoComplete}
			className={className}
			id={id}
			onChange={(event) =>
				onChange?.({ target: { value: event.target.value } })
			}
			onBlur={onBlur}
			placeholder={placeholder}
			required={required}
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

vi.mock("@/components/ui/select", () => {
	const { createContext, useContext } =
		require("react") as typeof import("react");

	const SelectContext = createContext<{
		onValueChange?: (value: string) => void;
		disabled?: boolean;
	}>({});

	return {
		Select: ({
			children,
			disabled,
			onValueChange,
		}: {
			children: React.ReactNode;
			disabled?: boolean;
			onValueChange?: (value: string) => void;
		}) => (
			<SelectContext.Provider value={{ disabled, onValueChange }}>
				<div>{children}</div>
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
					aria-label={`select-item:${value}`}
					disabled={context.disabled}
					onClick={() => context.onValueChange?.(value)}
				>
					{children}
				</button>
			);
		},
		SelectTrigger: ({
			children,
			className,
		}: {
			children: React.ReactNode;
			className?: string;
		}) => <div className={className}>{children}</div>,
		SelectValue: () => <span>select-value</span>,
	};
});

vi.mock("@/components/ui/switch", () => ({
	Switch: ({
		checked,
		id,
		onCheckedChange,
	}: {
		checked: boolean;
		id?: string;
		onCheckedChange?: (checked: boolean) => void;
	}) => (
		<button
			type="button"
			aria-label={`switch:${id ?? "toggle"}:${checked}`}
			onClick={() => onCheckedChange?.(!checked)}
		/>
	),
}));

vi.mock("@/components/ui/tooltip", () => ({
	Tooltip: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	TooltipContent: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	TooltipProvider: ({ children }: { children: React.ReactNode }) => (
		<div>{children}</div>
	),
	TooltipTrigger: ({
		children,
		render,
	}: {
		children?: React.ReactNode;
		render?: React.ReactNode;
	}) => render ?? children,
}));

vi.mock("@/components/ui/table", () => ({
	TableCell: ({
		children,
		className,
		onClick,
		onKeyDown,
	}: {
		children: React.ReactNode;
		className?: string;
		onClick?: (event: { stopPropagation?: () => void }) => void;
		onKeyDown?: (event: {
			key: string;
			preventDefault?: () => void;
			stopPropagation?: () => void;
		}) => void;
	}) => (
		<td
			data-slot="table-cell"
			className={className}
			onClick={onClick}
			onKeyDown={onKeyDown}
		>
			{children}
		</td>
	),
	TableHead: ({
		children,
		className,
	}: {
		children: React.ReactNode;
		className?: string;
	}) => (
		<th data-slot="table-head" className={className}>
			{children}
		</th>
	),
	TableHeader: ({ children }: { children: React.ReactNode }) => (
		<thead data-slot="table-header">{children}</thead>
	),
	TableRow: ({
		children,
		className,
		onClick,
		onKeyDown,
		tabIndex,
	}: {
		children: React.ReactNode;
		className?: string;
		onClick?: () => void;
		onKeyDown?: (event: {
			key: string;
			preventDefault?: () => void;
			stopPropagation?: () => void;
		}) => void;
		tabIndex?: number;
	}) => (
		<tr
			data-slot="table-row"
			className={className}
			onClick={onClick}
			onKeyDown={onKeyDown}
			tabIndex={tabIndex}
		>
			{children}
		</tr>
	),
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/hooks/useApiList", () => ({
	useApiList: () => {
		const [items, setItems] = useState(mockState.items);
		const [total, setTotal] = useState(mockState.total || items.length);
		return {
			items,
			loading: mockState.loading,
			reload: async () => {
				await mockState.reload();
				setItems(mockState.items);
				setTotal(mockState.total || mockState.items.length);
			},
			setItems,
			setTotal,
			total,
		};
	},
}));

vi.mock("@/services/adminService", () => ({
	adminPolicyService: {
		create: (...args: unknown[]) => mockState.create(...args),
		delete: (...args: unknown[]) => mockState.deletePolicy(...args),
		list: vi.fn(),
		testConnection: (...args: unknown[]) => mockState.testConnection(...args),
		testParams: (...args: unknown[]) => mockState.testParams(...args),
		update: (...args: unknown[]) => mockState.update(...args),
	},
	adminRemoteNodeService: {
		list: (...args: unknown[]) => mockState.listRemoteNodes(...args),
	},
}));

function createPolicy(overrides: Record<string, unknown> = {}) {
	return {
		allowed_types: [],
		base_path: "",
		bucket: "",
		chunk_size: 5 * 1024 * 1024,
		created_at: "2026-03-28T00:00:00Z",
		driver_type: "local",
		endpoint: "",
		id: 1,
		is_default: false,
		max_file_size: 0,
		name: "Local Policy",
		options: {},
		remote_node_id: null,
		updated_at: "2026-03-28T00:00:00Z",
		...overrides,
	};
}

function openCreateWizard(driver: "local" | "s3" = "local") {
	fireEvent.click(screen.getByRole("button", { name: /new_policy/i }));
	if (driver === "s3") {
		fireEvent.click(screen.getByRole("button", { name: /^S3\b/ }));
	}
	fireEvent.click(screen.getByRole("button", { name: "policy_wizard_next" }));
}

function advanceCreateWizardToRulesStep() {
	fireEvent.click(screen.getByRole("button", { name: "policy_wizard_review" }));
}

function openEditPolicy(name: string) {
	fireEvent.click(screen.getByText(name));
}

describe("AdminPoliciesPage", () => {
	beforeEach(() => {
		mockState.create.mockReset();
		mockState.deletePolicy.mockReset();
		mockState.handleApiError.mockReset();
		invalidateAdminRemoteNodeLookup();
		mockState.items = [];
		mockState.listRemoteNodes.mockReset();
		mockState.loading = false;
		mockState.reload.mockReset();
		mockState.remoteNodes = [];
		mockState.searchParams = "";
		mockState.setSearchParams.mockReset();
		mockState.testConnection.mockReset();
		mockState.testParams.mockReset();
		mockState.total = 0;
		mockState.toastSuccess.mockReset();
		mockState.update.mockReset();

		mockState.create.mockImplementation(async (payload) =>
			createPolicy({
				...(payload as Record<string, unknown>),
				id: 99,
			}),
		);
		mockState.deletePolicy.mockImplementation(async (id: number) => {
			mockState.items = mockState.items.filter((policy) => policy.id !== id);
		});
		mockState.reload.mockResolvedValue(undefined);
		mockState.listRemoteNodes.mockImplementation(async () => ({
			items: mockState.remoteNodes,
			total: mockState.remoteNodes.length,
		}));
		mockState.testConnection.mockResolvedValue(undefined);
		mockState.testParams.mockResolvedValue(undefined);
		mockState.update.mockImplementation(async (id, payload) =>
			createPolicy({
				...(payload as Record<string, unknown>),
				driver_type: "s3",
				id,
			}),
		);
	});

	it("renders local and s3 rows, including default and local fallback path states", () => {
		mockState.items = [
			createPolicy({
				id: 1,
				name: "Default Local",
				is_default: true,
			}),
			createPolicy({
				id: 2,
				name: "Archive S3",
				driver_type: "s3",
				endpoint: "https://s3.example.com",
				bucket: "archive",
			}),
		];

		render(<AdminPoliciesPage />);

		expect(screen.getByText("policies")).toBeInTheDocument();
		expect(screen.getByText("policies_intro")).toBeInTheDocument();
		expect(screen.getByText("Default Local")).toBeInTheDocument();
		expect(screen.getByText("Archive S3")).toBeInTheDocument();
		expect(screen.getByText("./data")).toBeInTheDocument();
		expect(screen.getByText("https://s3.example.com")).toBeInTheDocument();
		expect(screen.getByText("archive")).toBeInTheDocument();
		expect(screen.getAllByText("is_default")).toHaveLength(2);
		expect(
			screen.queryByRole("button", { name: "PencilSimple" }),
		).not.toBeInTheDocument();
		const localBadge = screen.getByText("Local");
		const s3Badge = screen.getByText("S3");
		expect(localBadge).toHaveAttribute("data-variant", "outline");
		expect(localBadge).toHaveClass("bg-emerald-500/10", "text-emerald-600");
		expect(s3Badge).toHaveAttribute("data-variant", "outline");
		expect(s3Badge).toHaveClass("bg-blue-500/10", "text-blue-600");
	});

	it("renders remote policies with the bound remote node name", async () => {
		mockState.remoteNodes = [
			{
				id: 7,
				name: "Edge East",
			},
		];
		mockState.items = [
			createPolicy({
				id: 3,
				name: "Remote Archive",
				driver_type: "remote",
				base_path: "tenant-a/archive",
				remote_node_id: 7,
			}),
		];

		render(<AdminPoliciesPage />);

		expect(screen.getByText("Remote")).toBeInTheDocument();
		expect(screen.getByText("tenant-a/archive")).toBeInTheDocument();
		await waitFor(() => {
			expect(screen.getByText("Edge East")).toBeInTheDocument();
		});
	});

	it("opens edit from any non-delete policy cell", () => {
		mockState.items = [
			createPolicy({
				id: 2,
				name: "Archive S3",
				driver_type: "s3",
				endpoint: "https://s3.example.com",
				bucket: "archive",
			}),
		];

		render(<AdminPoliciesPage />);

		fireEvent.click(screen.getByText("S3"));
		expect(screen.getByDisplayValue("Archive S3")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "delete_policy" }));
		expect(screen.getByText('delete_policy "Archive S3"?')).toBeInTheDocument();
	});

	it("prevents deleting the protected policy with id 1", () => {
		mockState.items = [
			createPolicy({
				id: 1,
				name: "System Policy",
			}),
		];

		render(<AdminPoliciesPage />);

		const deleteButton = screen.getByRole("button", { name: "delete_policy" });
		expect(deleteButton).toBeDisabled();
		expect(deleteButton).toHaveAttribute(
			"title",
			"initial_policy_delete_blocked",
		);

		fireEvent.click(deleteButton);

		expect(
			screen.queryByText('delete_policy "System Policy"?'),
		).not.toBeInTheDocument();
		expect(mockState.deletePolicy).not.toHaveBeenCalled();
	});

	it("tests create params and creates a new local policy", async () => {
		render(<AdminPoliciesPage />);

		openCreateWizard();

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Primary Local" },
		});
		fireEvent.change(screen.getByLabelText("base_path"), {
			target: { value: "/srv/data" },
		});
		advanceCreateWizardToRulesStep();
		expect(
			screen.queryByText("policy_wizard_local_rules_helper"),
		).not.toBeInTheDocument();
		fireEvent.change(screen.getByLabelText("max_file_size (bytes)"), {
			target: { value: "2048" },
		});
		fireEvent.change(screen.getByLabelText("chunk_size"), {
			target: { value: "8" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "switch:is_default:false" }),
		);

		fireEvent.click(screen.getByRole("button", { name: /test_connection/i }));

		await waitFor(() => {
			expect(mockState.testParams).toHaveBeenCalledWith({
				access_key: undefined,
				base_path: "/srv/data",
				bucket: undefined,
				driver_type: "local",
				endpoint: undefined,
				remote_node_id: undefined,
				secret_key: undefined,
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("connection_success");

		fireEvent.click(screen.getByRole("button", { name: /core:create/i }));

		await waitFor(() => {
			expect(mockState.create).toHaveBeenCalledWith({
				access_key: "",
				base_path: "/srv/data",
				bucket: "",
				chunk_size: 8 * 1024 * 1024,
				driver_type: "local",
				endpoint: "",
				is_default: true,
				max_file_size: 2048,
				name: "Primary Local",
				options: {},
				remote_node_id: undefined,
				secret_key: "",
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("policy_created");
	});

	it("creates a local policy without policy-level media processor overrides", async () => {
		render(<AdminPoliciesPage />);

		openCreateWizard();

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Native Thumb Local" },
		});
		advanceCreateWizardToRulesStep();
		fireEvent.click(screen.getByRole("button", { name: /core:create/i }));

		await waitFor(() => {
			expect(mockState.create).toHaveBeenCalledWith({
				access_key: "",
				base_path: "",
				bucket: "",
				chunk_size: 5 * 1024 * 1024,
				driver_type: "local",
				endpoint: "",
				is_default: false,
				max_file_size: undefined,
				name: "Native Thumb Local",
				options: {},
				remote_node_id: undefined,
				secret_key: "",
			});
		});
	});

	it("keeps the create dialog shell fixed and scrolls the form body internally", () => {
		const { container } = render(<AdminPoliciesPage />);

		fireEvent.click(screen.getByRole("button", { name: /new_policy/i }));

		const dialogContent = screen.getByTestId("dialog-content");
		expect(dialogContent).toHaveClass(
			"flex",
			"flex-col",
			"overflow-hidden",
			"p-0",
		);
		expect(dialogContent).not.toHaveClass("overflow-y-auto");

		const form = dialogContent.querySelector("form");
		if (!form) {
			throw new Error("Expected create dialog form to render");
		}
		expect(form).toHaveClass(
			"flex",
			"min-h-0",
			"flex-1",
			"flex-col",
			"overflow-hidden",
		);

		const scrollBody = container.querySelector("form > .overflow-y-auto");
		if (!scrollBody) {
			throw new Error(
				"Expected create dialog body to be internally scrollable",
			);
		}
		expect(scrollBody).toHaveClass(
			"min-h-0",
			"flex-1",
			"overflow-y-auto",
			"px-6",
		);
		expect(
			screen.queryByRole("button", { name: /core:cancel/i }),
		).not.toBeInTheDocument();

		const footer = screen.getByTestId("dialog-footer");
		expect(footer).toHaveClass("w-full", "flex-row");

		const nextButton = screen.getByRole("button", {
			name: "policy_wizard_next",
		});
		expect(nextButton.parentElement).toHaveClass(
			"ml-auto",
			"flex-nowrap",
			"justify-end",
		);

		fireEvent.click(nextButton);

		const forwardAnimatedPanel = screen.getByTestId("policy-step-panel");
		expect(forwardAnimatedPanel).toHaveClass(
			"animate-in",
			"fade-in",
			"slide-in-from-right-6",
		);

		fireEvent.click(screen.getByRole("button", { name: /core:back/i }));

		const backwardAnimatedPanel = screen.getByTestId("policy-step-panel");
		expect(backwardAnimatedPanel).toHaveClass(
			"animate-in",
			"fade-in",
			"slide-in-from-left-6",
		);
	});

	it("shows S3 connection testing in step two before review", async () => {
		render(<AdminPoliciesPage />);

		openCreateWizard("s3");

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Archive S3" },
		});
		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "archive" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "https://s3.example.com" },
		});

		expect(
			screen.getByRole("button", { name: /test_connection/i }),
		).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: /core:cancel/i }),
		).not.toBeInTheDocument();

		const reviewButton = screen.getByRole("button", {
			name: "policy_wizard_review",
		});
		expect(reviewButton.parentElement).toHaveClass(
			"ml-auto",
			"flex-nowrap",
			"justify-end",
		);

		fireEvent.click(screen.getByRole("button", { name: /test_connection/i }));

		await waitFor(() => {
			expect(mockState.testParams).toHaveBeenCalledWith({
				access_key: undefined,
				base_path: undefined,
				bucket: "archive",
				driver_type: "s3",
				endpoint: "https://s3.example.com",
				remote_node_id: undefined,
				secret_key: undefined,
			});
		});
	});

	it("tests remote policy params and creates a bound remote policy", async () => {
		mockState.remoteNodes = [
			{
				id: 7,
				name: "Edge East",
				base_url: "https://remote.example.com",
			},
		];

		render(<AdminPoliciesPage />);

		fireEvent.click(screen.getByRole("button", { name: /new_policy/i }));
		fireEvent.click(screen.getByRole("button", { name: /Remote/ }));
		fireEvent.click(screen.getByRole("button", { name: "policy_wizard_next" }));

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Remote Archive" },
		});
		await waitFor(() => {
			expect(
				screen.getByRole("button", { name: "select-item:7" }),
			).toBeInTheDocument();
		});
		fireEvent.click(screen.getByRole("button", { name: "select-item:7" }));

		fireEvent.click(screen.getByRole("button", { name: /test_connection/i }));

		await waitFor(() => {
			expect(mockState.testParams).toHaveBeenCalledWith({
				access_key: undefined,
				base_path: undefined,
				bucket: undefined,
				driver_type: "remote",
				endpoint: undefined,
				remote_node_id: 7,
				secret_key: undefined,
			});
		});

		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);
		fireEvent.click(screen.getByRole("button", { name: /core:create/i }));

		await waitFor(() => {
			expect(mockState.create).toHaveBeenCalledWith({
				access_key: "",
				base_path: "",
				bucket: "",
				chunk_size: 5 * 1024 * 1024,
				driver_type: "remote",
				endpoint: "",
				is_default: false,
				max_file_size: undefined,
				name: "Remote Archive",
				options: {
					remote_download_strategy: "relay_stream",
					remote_upload_strategy: "relay_stream",
				},
				remote_node_id: 7,
				secret_key: "",
			});
		});
	});

	it("creates a remote policy with presigned upload strategy", async () => {
		mockState.remoteNodes = [
			{
				id: 9,
				name: "Edge West",
				base_url: "https://remote-west.example.com",
			},
		];

		render(<AdminPoliciesPage />);

		fireEvent.click(screen.getByRole("button", { name: /new_policy/i }));
		fireEvent.click(screen.getByRole("button", { name: /Remote/ }));
		fireEvent.click(screen.getByRole("button", { name: "policy_wizard_next" }));

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Remote Presigned Archive" },
		});
		await waitFor(() => {
			expect(
				screen.getByRole("button", { name: "select-item:9" }),
			).toBeInTheDocument();
		});
		fireEvent.click(screen.getByRole("button", { name: "select-item:9" }));

		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);
		fireEvent.click(
			screen.getAllByRole("button", { name: "select-item:presigned" })[1],
		);
		fireEvent.click(screen.getByRole("button", { name: /core:create/i }));

		await waitFor(() => {
			expect(mockState.create).toHaveBeenCalledWith({
				access_key: "",
				base_path: "",
				bucket: "",
				chunk_size: 5 * 1024 * 1024,
				driver_type: "remote",
				endpoint: "",
				is_default: false,
				max_file_size: undefined,
				name: "Remote Presigned Archive",
				options: {
					remote_download_strategy: "relay_stream",
					remote_upload_strategy: "presigned",
				},
				remote_node_id: 9,
				secret_key: "",
			});
		});
	});

	it("creates a remote policy with presigned download strategy", async () => {
		mockState.remoteNodes = [
			{
				id: 10,
				name: "Edge Download",
				base_url: "https://remote-download.example.com",
			},
		];

		render(<AdminPoliciesPage />);

		fireEvent.click(screen.getByRole("button", { name: /new_policy/i }));
		fireEvent.click(screen.getByRole("button", { name: /Remote/ }));
		fireEvent.click(screen.getByRole("button", { name: "policy_wizard_next" }));

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Remote Presigned Download Archive" },
		});
		await waitFor(() => {
			expect(
				screen.getByRole("button", { name: "select-item:10" }),
			).toBeInTheDocument();
		});
		fireEvent.click(screen.getByRole("button", { name: "select-item:10" }));

		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);
		fireEvent.click(
			screen.getAllByRole("button", { name: "select-item:presigned" })[0],
		);
		fireEvent.click(screen.getByRole("button", { name: /core:create/i }));

		await waitFor(() => {
			expect(mockState.create).toHaveBeenCalledWith({
				access_key: "",
				base_path: "",
				bucket: "",
				chunk_size: 5 * 1024 * 1024,
				driver_type: "remote",
				endpoint: "",
				is_default: false,
				max_file_size: undefined,
				name: "Remote Presigned Download Archive",
				options: {
					remote_download_strategy: "presigned",
					remote_upload_strategy: "relay_stream",
				},
				remote_node_id: 10,
				secret_key: "",
			});
		});
	});

	it("does not save when moving from S3 step two to review", () => {
		render(<AdminPoliciesPage />);

		openCreateWizard("s3");

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Archive S3" },
		});
		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "archive" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "https://s3.example.com" },
		});

		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);

		expect(mockState.create).not.toHaveBeenCalled();
		expect(mockState.update).not.toHaveBeenCalled();
		expect(
			screen.getByRole("button", { name: /core:create/i }),
		).toBeInTheDocument();
	});

	it("keeps edit dialog primary actions right aligned", () => {
		mockState.items = [createPolicy({ id: 3, name: "Edit Me" })];

		render(<AdminPoliciesPage />);

		openEditPolicy("Edit Me");

		const editShell = screen.getByTestId("policy-edit-shell");
		expect(editShell).toHaveClass(
			"grid",
			"gap-6",
			"lg:grid-cols-[300px_minmax(0,1fr)]",
		);
		expect(
			screen.getByText("policy_editor_overview_title"),
		).toBeInTheDocument();
		expect(
			screen.queryByText("policy_editor_storage_title"),
		).not.toBeInTheDocument();
		expect(screen.getByText("policy_editor_rules_title")).toBeInTheDocument();
		expect(screen.getByTestId("policy-summary-card")).toBeInTheDocument();
		expect(screen.getByTestId("policy-summary-card").parentElement).toHaveClass(
			"order-2",
			"lg:sticky",
			"lg:top-0",
			"lg:order-1",
			"lg:self-start",
		);
		expect(
			screen.queryByText("policy_wizard_driver_panel_title"),
		).not.toBeInTheDocument();
		expect(screen.getByTestId("policy-driver-badge")).toHaveAttribute(
			"data-variant",
			"outline",
		);
		expect(screen.getByTestId("policy-driver-badge")).toHaveClass(
			"bg-emerald-500/10",
			"text-emerald-600",
		);

		const footer = screen.getByTestId("dialog-footer");
		expect(footer).toHaveClass("w-full", "flex-row");

		const saveButton = screen.getByRole("button", { name: /save_changes/i });
		expect(saveButton.parentElement).toHaveClass(
			"ml-auto",
			"flex-nowrap",
			"justify-end",
		);
	});

	it("tests changed s3 params and updates with provided credentials", async () => {
		mockState.items = [
			createPolicy({
				id: 7,
				name: "Archive S3",
				driver_type: "s3",
				endpoint: "https://s3.example.com",
				bucket: "archive",
				base_path: "tenant-a",
				max_file_size: 4096,
				options: { s3_upload_strategy: "presigned" },
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("Archive S3");

		expect(screen.getByText("s3_endpoint_hint")).toBeInTheDocument();
		expect(screen.getByTestId("policy-driver-badge")).toHaveAttribute(
			"data-variant",
			"outline",
		);
		expect(screen.getByTestId("policy-driver-badge")).toHaveClass(
			"bg-blue-500/10",
			"text-blue-600",
		);
		expect(screen.getByDisplayValue("Archive S3")).toBeInTheDocument();
		expect(screen.getByDisplayValue("tenant-a")).toBeInTheDocument();
		expect(screen.getByDisplayValue("4096")).toBeInTheDocument();
		expect(screen.getByDisplayValue("5")).toBeInTheDocument();
		expect(screen.getByLabelText("Access Key")).toHaveAttribute(
			"placeholder",
			"policy_editor_credentials_keep_placeholder",
		);
		expect(screen.getByLabelText("Access Key")).toHaveAttribute(
			"autocomplete",
			"off",
		);
		expect(screen.getByLabelText("Secret Key")).toHaveAttribute(
			"placeholder",
			"policy_editor_credentials_keep_placeholder",
		);
		expect(screen.getByLabelText("Secret Key")).toHaveAttribute(
			"autocomplete",
			"new-password",
		);

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Archive S3 Updated" },
		});
		fireEvent.change(screen.getByLabelText("Access Key"), {
			target: { value: "NEWKEY" },
		});
		fireEvent.change(screen.getByLabelText("Secret Key"), {
			target: { value: "NEWSECRET" },
		});
		fireEvent.click(
			screen.getAllByRole("button", { name: "select-item:relay_stream" })[0],
		);
		fireEvent.click(screen.getByRole("button", { name: /test_connection/i }));

		await waitFor(() => {
			expect(mockState.testParams).toHaveBeenCalledWith({
				access_key: "NEWKEY",
				base_path: "tenant-a",
				bucket: "archive",
				driver_type: "s3",
				endpoint: "https://s3.example.com",
				remote_node_id: undefined,
				secret_key: "NEWSECRET",
			});
		});
		await waitFor(() => {
			expect(mockState.toastSuccess).toHaveBeenCalledWith("connection_success");
		});
		expect(mockState.testConnection).not.toHaveBeenCalled();

		fireEvent.click(screen.getByRole("button", { name: /save_changes/i }));

		await waitFor(() => {
			expect(mockState.update).toHaveBeenCalledTimes(1);
		});
		expect(mockState.testParams).toHaveBeenCalledTimes(1);

		const [, payload] = mockState.update.mock.calls[0] as [
			number,
			Record<string, unknown>,
		];
		expect(mockState.update).toHaveBeenCalledWith(
			7,
			expect.objectContaining({
				base_path: "tenant-a",
				bucket: "archive",
				chunk_size: 5 * 1024 * 1024,
				endpoint: "https://s3.example.com",
				is_default: false,
				max_file_size: 4096,
				name: "Archive S3 Updated",
				options: {
					s3_download_strategy: "relay_stream",
					s3_upload_strategy: "relay_stream",
				},
			}),
		);
		expect(payload).toHaveProperty("access_key", "NEWKEY");
		expect(payload).toHaveProperty("secret_key", "NEWSECRET");
		expect(mockState.toastSuccess).toHaveBeenCalledWith("policy_updated");
	});

	it("parses and updates local content dedup options", async () => {
		mockState.items = [
			createPolicy({
				id: 11,
				name: "Dedup Local",
				driver_type: "local",
				base_path: "/srv/dedup",
				options: { content_dedup: true },
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("Dedup Local");

		expect(screen.getByDisplayValue("Dedup Local")).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "switch:content_dedup:true" }),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "switch:content_dedup:true" }),
		);
		fireEvent.click(screen.getByRole("button", { name: /save_changes/i }));

		await waitFor(() => {
			expect(mockState.update).toHaveBeenCalledWith(
				11,
				expect.objectContaining({
					options: {},
				}),
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("policy_updated");
	});

	it("splits an R2 bucket path into the endpoint and bucket inputs on blur", () => {
		render(<AdminPoliciesPage />);

		openCreateWizard("s3");

		const endpointInput = screen.getByLabelText("endpoint");
		fireEvent.change(endpointInput, {
			target: {
				value: "https://demo-account.r2.cloudflarestorage.com/photos",
			},
		});
		fireEvent.blur(endpointInput);

		expect(
			screen.getByDisplayValue("https://demo-account.r2.cloudflarestorage.com"),
		).toBeInTheDocument();
		expect(screen.getByDisplayValue("photos")).toBeInTheDocument();
	});

	it("marks public r2.dev endpoints as invalid", () => {
		render(<AdminPoliciesPage />);

		openCreateWizard("s3");

		const endpointInput = screen.getByLabelText("endpoint");
		fireEvent.change(endpointInput, {
			target: {
				value: "https://pub-dsaifhoiuahfas.r2.dev/aster-drive",
			},
		});

		expect(endpointInput).toHaveAttribute("aria-invalid", "true");
		expect(
			screen.getByText("s3_endpoint_public_r2_dev_error"),
		).toBeInTheDocument();
	});

	it("displays presigned strategy from structured options", async () => {
		mockState.items = [
			createPolicy({
				id: 10,
				name: "Legacy Presigned S3",
				driver_type: "s3",
				endpoint: "https://s3.example.com",
				bucket: "legacy-bucket",
				base_path: "legacy-path",
				options: { s3_upload_strategy: "presigned" },
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("Legacy Presigned S3");

		expect(screen.getByDisplayValue("Legacy Presigned S3")).toBeInTheDocument();
		expect(screen.getByDisplayValue("legacy-bucket")).toBeInTheDocument();
		expect(screen.getByDisplayValue("legacy-path")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: /save_changes/i }));

		await waitFor(() => {
			expect(mockState.update).toHaveBeenCalledWith(
				10,
				expect.objectContaining({
					options: {
						s3_download_strategy: "relay_stream",
						s3_upload_strategy: "presigned",
					},
				}),
			);
		});

		const [, payload] = mockState.update.mock.calls[0] as [
			number,
			Record<string, unknown>,
		];
		expect(payload).not.toHaveProperty("access_key");
		expect(payload).not.toHaveProperty("secret_key");
		expect(mockState.toastSuccess).toHaveBeenCalledWith("policy_updated");
	});

	it("tests relay_stream params and updates s3 policy without blank secrets", async () => {
		mockState.items = [
			createPolicy({
				id: 9,
				name: "Relay S3",
				driver_type: "s3",
				endpoint: "https://s3.example.com",
				bucket: "relay-bucket",
				base_path: "tenant-relay",
				max_file_size: 4096,
				options: { s3_upload_strategy: "relay_stream" },
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("Relay S3");

		expect(screen.getByDisplayValue("Relay S3")).toBeInTheDocument();
		expect(screen.getByDisplayValue("tenant-relay")).toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("Access Key"), {
			target: { value: "NEWKEY" },
		});
		fireEvent.change(screen.getByLabelText("Secret Key"), {
			target: { value: "NEWSECRET" },
		});
		fireEvent.click(
			screen.getAllByRole("button", { name: "select-item:relay_stream" })[0],
		);
		fireEvent.click(screen.getByRole("button", { name: /test_connection/i }));

		await waitFor(() => {
			expect(mockState.testParams).toHaveBeenCalledWith({
				access_key: "NEWKEY",
				base_path: "tenant-relay",
				bucket: "relay-bucket",
				driver_type: "s3",
				endpoint: "https://s3.example.com",
				remote_node_id: undefined,
				secret_key: "NEWSECRET",
			});
		});

		fireEvent.click(screen.getByRole("button", { name: /save_changes/i }));

		await waitFor(() => {
			expect(mockState.update).toHaveBeenCalledTimes(1);
		});

		const [, payload] = mockState.update.mock.calls[0] as [
			number,
			Record<string, unknown>,
		];
		expect(mockState.update).toHaveBeenCalledWith(
			9,
			expect.objectContaining({
				access_key: "NEWKEY",
				base_path: "tenant-relay",
				bucket: "relay-bucket",
				chunk_size: 5 * 1024 * 1024,
				endpoint: "https://s3.example.com",
				is_default: false,
				max_file_size: 4096,
				name: "Relay S3",
				options: {
					s3_download_strategy: "relay_stream",
					s3_upload_strategy: "relay_stream",
				},
				secret_key: "NEWSECRET",
			}),
		);
		expect(payload).toHaveProperty("secret_key", "NEWSECRET");
		expect(mockState.toastSuccess).toHaveBeenCalledWith("policy_updated");
	});

	it("shows unlimited for zero max file size while preserving raw limit inputs in edit mode", () => {
		mockState.items = [
			createPolicy({
				id: 8,
				name: "Direct Put S3",
				driver_type: "s3",
				endpoint: "https://s3.example.com",
				bucket: "direct-put",
				max_file_size: 0,
				chunk_size: 0,
				options: { s3_upload_strategy: "presigned" },
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("Direct Put S3");

		expect(screen.getByDisplayValue("Direct Put S3")).toBeInTheDocument();
		expect(screen.getAllByDisplayValue("0")).toHaveLength(2);
		expect(screen.getByText("core:unlimited")).toBeInTheDocument();
		expect(screen.queryByDisplayValue("5")).not.toBeInTheDocument();
	});

	it("asks for confirmation before force-saving a failing s3 create", async () => {
		mockState.testParams.mockRejectedValueOnce(new Error("bad s3 config"));

		render(<AdminPoliciesPage />);

		openCreateWizard("s3");

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Broken S3" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "https://s3.example.com" },
		});
		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "broken-bucket" },
		});
		fireEvent.change(screen.getByLabelText("Access Key"), {
			target: { value: "BROKENKEY" },
		});
		fireEvent.change(screen.getByLabelText("Secret Key"), {
			target: { value: "BROKENSECRET" },
		});
		advanceCreateWizardToRulesStep();

		fireEvent.click(screen.getByRole("button", { name: /core:create/i }));

		await waitFor(() => {
			expect(mockState.testParams).toHaveBeenCalledWith({
				access_key: "BROKENKEY",
				base_path: undefined,
				bucket: "broken-bucket",
				driver_type: "s3",
				endpoint: "https://s3.example.com",
				remote_node_id: undefined,
				secret_key: "BROKENSECRET",
			});
		});
		expect(mockState.create).not.toHaveBeenCalled();
		expect(mockState.handleApiError).not.toHaveBeenCalled();
		expect(
			await screen.findByText("connection_test_failed"),
		).toBeInTheDocument();
		expect(
			await screen.findByText("policy_test_failed_confirm_desc"),
		).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "save_anyway" }));

		await waitFor(() => {
			expect(mockState.create).toHaveBeenCalledWith({
				access_key: "BROKENKEY",
				base_path: "",
				bucket: "broken-bucket",
				chunk_size: 5 * 1024 * 1024,
				driver_type: "s3",
				endpoint: "https://s3.example.com",
				is_default: false,
				max_file_size: undefined,
				name: "Broken S3",
				options: {
					s3_download_strategy: "relay_stream",
					s3_upload_strategy: "relay_stream",
				},
				remote_node_id: undefined,
				secret_key: "BROKENSECRET",
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("policy_created");
	});

	it("confirms deletion and removes the policy row", async () => {
		mockState.items = [
			createPolicy({
				id: 8,
				name: "Remove Me",
			}),
		];

		render(<AdminPoliciesPage />);

		fireEvent.click(screen.getByRole("button", { name: "delete_policy" }));

		expect(screen.getByText('delete_policy "Remove Me"?')).toBeInTheDocument();
		expect(screen.getByText("delete_policy_desc")).toBeInTheDocument();

		fireEvent.click(
			within(
				screen.getByText('delete_policy "Remove Me"?')
					.parentElement as HTMLElement,
			).getByRole("button", { name: "core:delete" }),
		);

		await waitFor(() => {
			expect(mockState.deletePolicy).toHaveBeenCalledWith(8);
		});
		await waitFor(() => {
			expect(screen.queryByText("Remove Me")).not.toBeInTheDocument();
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("policy_deleted");
	});

	it("offers force deletion when upload sessions still reference the policy", async () => {
		mockState.items = [
			createPolicy({
				id: 8,
				name: "Remove Me",
			}),
		];
		mockState.deletePolicy
			.mockRejectedValueOnce(
				new ApiError(1003, "upload sessions exist", {
					subcode: ApiSubcode.PolicyUploadSessionsExist,
				}),
			)
			.mockImplementationOnce(async (id: number) => {
				mockState.items = mockState.items.filter((policy) => policy.id !== id);
			});

		render(<AdminPoliciesPage />);

		fireEvent.click(screen.getByRole("button", { name: "delete_policy" }));
		fireEvent.click(
			within(
				screen.getByText('delete_policy "Remove Me"?')
					.parentElement as HTMLElement,
			).getByRole("button", { name: "core:delete" }),
		);

		expect(
			await screen.findByText('force_delete_policy "Remove Me"?'),
		).toBeInTheDocument();
		expect(screen.getByText("force_delete_policy_desc")).toBeInTheDocument();
		expect(mockState.handleApiError).not.toHaveBeenCalled();

		fireEvent.click(
			screen.getByRole("button", { name: "force_delete_policy_confirm" }),
		);

		await waitFor(() => {
			expect(mockState.deletePolicy).toHaveBeenNthCalledWith(1, 8);
			expect(mockState.deletePolicy).toHaveBeenNthCalledWith(2, 8, {
				force: true,
			});
		});
		await waitFor(() => {
			expect(screen.queryByText("Remove Me")).not.toBeInTheDocument();
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("policy_force_deleted");
	});

	it("shows a deleting state while policy deletion is pending", async () => {
		let resolveDelete: (() => void) | null = null;
		mockState.items = [
			createPolicy({
				id: 8,
				name: "Remove Me",
			}),
		];
		mockState.deletePolicy.mockImplementationOnce(
			() =>
				new Promise<void>((resolve) => {
					resolveDelete = resolve;
				}),
		);

		render(<AdminPoliciesPage />);

		fireEvent.click(screen.getByRole("button", { name: "delete_policy" }));
		fireEvent.click(
			within(
				screen.getByText('delete_policy "Remove Me"?')
					.parentElement as HTMLElement,
			).getByRole("button", { name: "core:delete" }),
		);

		await waitFor(() => {
			expect(
				screen.getByRole("button", { name: "policy_deleting" }),
			).toBeDisabled();
		});

		resolveDelete?.();
		await waitFor(() => {
			expect(mockState.toastSuccess).toHaveBeenCalledWith("policy_deleted");
		});
	});

	it("reports backend validation errors for incomplete s3 connection tests", async () => {
		const validationError = new Error("access_key is required");
		mockState.testParams.mockRejectedValueOnce(validationError);
		render(<AdminPoliciesPage />);

		openCreateWizard("s3");

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Broken S3" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "https://s3.example.com" },
		});
		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "broken-bucket" },
		});
		advanceCreateWizardToRulesStep();

		expect(
			screen.getByRole("button", { name: /test_connection/i }),
		).not.toBeDisabled();
		fireEvent.click(screen.getByRole("button", { name: /test_connection/i }));
		await waitFor(() => {
			expect(mockState.testParams).toHaveBeenCalledWith({
				access_key: undefined,
				base_path: undefined,
				bucket: "broken-bucket",
				driver_type: "s3",
				endpoint: "https://s3.example.com",
				remote_node_id: undefined,
				secret_key: undefined,
			});
		});
		expect(mockState.handleApiError).toHaveBeenCalledWith(validationError);
		expect(mockState.testConnection).not.toHaveBeenCalled();
	});
});
