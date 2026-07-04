import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AdminRemoteNodesPage from "@/pages/admin/AdminRemoteNodesPage";

const mockState = vi.hoisted(() => ({
	clipboard: vi.fn(),
	frontendConfigSiteUrl: null as string | null,
	handleApiError: vi.fn(),
	reload: vi.fn(),
	searchParams: "",
	setItems: vi.fn(),
	setSearchParams: vi.fn(),
	setTotal: vi.fn(),
	toastError: vi.fn(),
	toastInfo: vi.fn(),
	toastSuccess: vi.fn(),
	useApiList: vi.fn(),
}));

const adminRemoteNodeServiceMocks = vi.hoisted(() => ({
	create: vi.fn(),
	createEnrollmentCommand: vi.fn(),
	createStorageTarget: vi.fn(),
	delete: vi.fn(),
	deleteStorageTarget: vi.fn(),
	get: vi.fn(),
	list: vi.fn(),
	listStorageTargetDrivers: vi.fn(),
	listStorageTargets: vi.fn(),
	testConnection: vi.fn(),
	update: vi.fn(),
	updateStorageTarget: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, options?: Record<string, unknown>) => {
			if (key === "page_size_option") return `page-size:${options?.count}`;
			return key;
		},
	}),
}));

vi.mock("react-router-dom", () => ({
	useSearchParams: () => [
		new URLSearchParams(mockState.searchParams),
		mockState.setSearchParams,
	],
}));

vi.mock("sonner", () => ({
	toast: {
		error: (...args: unknown[]) => mockState.toastError(...args),
		info: (...args: unknown[]) => mockState.toastInfo(...args),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/components/admin/AdminOffsetPagination", () => ({
	AdminOffsetPagination: () => null,
}));

vi.mock("@/components/admin/admin-remote-nodes-page/RemoteNodeDialog", () => ({
	RemoteNodeDialog: ({
		editingNode,
		form,
		remoteStorageTargetDriverDescriptors = [],
		remoteStorageTargetDriverDescriptorsError = null,
		remoteStorageTargets = [],
		remoteStorageTargetsEnabled,
		remoteStorageTargetsError,
		mode,
		onCreateRemoteStorageTarget,
		onCreateBack,
		onCreateNext,
		createStep,
		onDeleteRemoteStorageTarget,
		onFieldChange,
		open,
		onOpenChange,
		onRunConnectionTest,
		onSubmit,
		onUpdateRemoteStorageTarget,
	}: {
		editingNode: { id: number; name: string } | null;
		form: { base_url: string; is_enabled: boolean; name: string };
		remoteStorageTargetDriverDescriptors?: unknown[];
		remoteStorageTargetDriverDescriptorsError?: string | null;
		remoteStorageTargets?: unknown[];
		remoteStorageTargetsEnabled: boolean;
		remoteStorageTargetsError: string | null;
		mode: "create" | "edit";
		onCreateRemoteStorageTarget: (payload: unknown) => Promise<void>;
		onCreateBack: () => void;
		onCreateNext: () => void;
		createStep: number;
		onDeleteRemoteStorageTarget: (profile: {
			target_key: string;
		}) => Promise<void>;
		onFieldChange: (
			key: "base_url" | "is_enabled" | "name",
			value: string | boolean,
		) => void;
		onOpenChange: (open: boolean) => void;
		onRunConnectionTest: () => Promise<boolean>;
		onSubmit: () => void;
		onUpdateRemoteStorageTarget: (
			target_key: string,
			payload: unknown,
		) => Promise<void>;
		open: boolean;
	}) =>
		open ? (
			<div data-testid="remote-node-dialog">
				<div>{mode}</div>
				<div data-testid="remote-node-name">{form.name}</div>
				<div data-testid="create-step">{createStep}</div>
				<div data-testid="managed-ingress-enabled">
					{String(remoteStorageTargetsEnabled)}
				</div>
				<div data-testid="managed-ingress-error">
					{remoteStorageTargetsError ?? ""}
				</div>
				<div data-testid="managed-ingress-profile-count">
					{remoteStorageTargets.length}
				</div>
				<div data-testid="managed-ingress-driver-count">
					{remoteStorageTargetDriverDescriptors.length}
				</div>
				<div data-testid="managed-ingress-driver-error">
					{remoteStorageTargetDriverDescriptorsError ?? ""}
				</div>
				<div data-testid="editing-node-name">{editingNode?.name ?? ""}</div>
				<button
					type="button"
					onClick={() => onFieldChange("name", "Edge Beta")}
				>
					change-name
				</button>
				<button
					type="button"
					onClick={() => onFieldChange("base_url", "https://edge.example.com")}
				>
					change-base-url
				</button>
				<button type="button" onClick={onCreateBack}>
					create-back
				</button>
				<button type="button" onClick={onCreateNext}>
					create-next
				</button>
				<button type="button" onClick={onSubmit}>
					submit-node
				</button>
				<button type="button" onClick={() => onOpenChange(false)}>
					close-node-dialog
				</button>
				<button
					type="button"
					onClick={() => {
						void onRunConnectionTest();
					}}
				>
					run-connection-test
				</button>
				<button
					type="button"
					onClick={() => {
						void onCreateRemoteStorageTarget({ name: "Ingress" });
					}}
				>
					create-ingress
				</button>
				<button
					type="button"
					onClick={() => {
						void onUpdateRemoteStorageTarget("default", { name: "Ingress" });
					}}
				>
					update-ingress
				</button>
				<button
					type="button"
					onClick={() => {
						void onDeleteRemoteStorageTarget({ target_key: "default" });
					}}
				>
					delete-ingress
				</button>
			</div>
		) : null,
}));

vi.mock(
	"@/components/admin/admin-remote-nodes-page/RemoteNodeEnrollmentDialog",
	() => ({
		RemoteNodeEnrollmentDialog: ({
			canTestConnection,
			command,
			onCopy,
			onOpenChange,
			onVerifyConnection,
			open,
		}: {
			canTestConnection: boolean;
			command: { command: string; remote_node_id: number } | null;
			onCopy: (value: string) => Promise<void>;
			onOpenChange: (open: boolean) => void;
			onVerifyConnection: (remoteNodeId: number) => Promise<boolean>;
			open: boolean;
		}) =>
			open ? (
				<div data-testid="enrollment-dialog">
					<div>{command?.command}</div>
					<div data-testid="can-test">{String(canTestConnection)}</div>
					<button
						type="button"
						onClick={() => {
							void onCopy(command?.command ?? "");
						}}
					>
						copy-command
					</button>
					<button
						type="button"
						onClick={() => {
							void onVerifyConnection(command?.remote_node_id ?? 0);
						}}
					>
						verify-command
					</button>
					<button type="button" onClick={() => onOpenChange(false)}>
						close-enrollment
					</button>
				</div>
			) : null,
	}),
);

vi.mock("@/components/admin/admin-remote-nodes-page/RemoteNodesTable", () => ({
	RemoteNodesTable: ({
		items,
		onEdit,
		onGenerateEnrollmentCommand,
		onRequestDelete,
		onSortChange,
	}: {
		items: Array<{ id: number; name: string }>;
		onEdit: (node: (typeof items)[number]) => void;
		onGenerateEnrollmentCommand: (node: (typeof items)[number]) => void;
		onRequestDelete: (id: number) => void;
		onSortChange: (sortBy: "name", sortOrder: "asc") => void;
	}) => (
		<div data-testid="remote-nodes-table">
			{items.map((item) => (
				<div key={item.id}>
					<span>{item.name}</span>
					<button type="button" onClick={() => onEdit(item)}>
						edit:{item.id}
					</button>
					<button
						type="button"
						onClick={() => onGenerateEnrollmentCommand(item)}
					>
						enroll:{item.id}
					</button>
					<button type="button" onClick={() => onRequestDelete(item.id)}>
						delete:{item.id}
					</button>
				</div>
			))}
			<button type="button" onClick={() => onSortChange("name", "asc")}>
				sort-name
			</button>
		</div>
	),
}));

vi.mock("@/components/common/ConfirmDialog", () => ({
	ConfirmDialog: ({
		confirmLabel,
		onConfirm,
		open,
		title,
	}: {
		confirmLabel: string;
		onConfirm: () => void;
		open: boolean;
		title: string;
	}) =>
		open ? (
			<dialog open>
				<h2>{title}</h2>
				<button type="button" onClick={onConfirm}>
					{confirmLabel}
				</button>
			</dialog>
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
	}: {
		actions?: React.ReactNode;
		description?: string;
		title: string;
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

vi.mock("@/components/ui/icon", () => ({
	Icon: () => <span aria-hidden="true" />,
}));

vi.mock("@/hooks/useApiError", () => ({
	getApiErrorMessage: (error: unknown) => String(error),
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

vi.mock("@/hooks/usePageTitle", () => ({
	usePageTitle: vi.fn(),
}));

vi.mock("@/lib/clipboard", () => ({
	writeTextToClipboard: (...args: unknown[]) => mockState.clipboard(...args),
}));

vi.mock("@/lib/logger", () => ({
	logger: {
		warn: vi.fn(),
	},
}));

vi.mock("@/services/adminService", () => ({
	adminRemoteNodeService: adminRemoteNodeServiceMocks,
}));

vi.mock("@/stores/frontendConfigStore", () => ({
	useFrontendConfigStore: (
		selector: (state: { siteUrl: string | null }) => unknown,
	) =>
		selector({
			siteUrl: mockState.frontendConfigSiteUrl,
		}),
}));

function renderPage() {
	render(<AdminRemoteNodesPage />);
}

describe("AdminRemoteNodesPage", () => {
	beforeEach(() => {
		mockState.clipboard.mockReset();
		mockState.clipboard.mockResolvedValue(undefined);
		mockState.frontendConfigSiteUrl = null;
		mockState.handleApiError.mockReset();
		mockState.reload.mockReset();
		mockState.searchParams = "";
		mockState.setItems.mockReset();
		mockState.setSearchParams.mockReset();
		mockState.setTotal.mockReset();
		mockState.toastError.mockReset();
		mockState.toastInfo.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.useApiList.mockReset();
		mockState.useApiList.mockReturnValue({
			items: [],
			loading: false,
			reload: mockState.reload,
			setItems: mockState.setItems,
			setTotal: mockState.setTotal,
			total: 0,
		});
		for (const mock of Object.values(adminRemoteNodeServiceMocks)) {
			mock.mockReset();
		}
		adminRemoteNodeServiceMocks.create.mockResolvedValue({
			base_url: "https://edge.example.com",
			id: 9,
			name: "Edge Beta",
		});
		adminRemoteNodeServiceMocks.createEnrollmentCommand.mockResolvedValue({
			command: "asterdrive-node enroll token",
			expires_at: "2026-05-22T00:00:00Z",
			master_url: "https://drive.example.com",
			remote_node_id: 9,
			remote_node_name: "Edge Beta",
			token: "token",
		});
		adminRemoteNodeServiceMocks.delete.mockResolvedValue(undefined);
		adminRemoteNodeServiceMocks.createStorageTarget.mockResolvedValue(
			undefined,
		);
		adminRemoteNodeServiceMocks.deleteStorageTarget.mockResolvedValue(
			undefined,
		);
		adminRemoteNodeServiceMocks.get.mockResolvedValue({
			base_url: "https://edge.example.com",
			id: 7,
			name: "Edge Alpha",
		});
		adminRemoteNodeServiceMocks.list.mockResolvedValue({
			items: [],
			total: 0,
		});
		adminRemoteNodeServiceMocks.listStorageTargetDrivers.mockResolvedValue([
			{
				description_key: "remote_node_ingress_profile_local_scope_hint",
				driver_type: "local",
				fields: [
					{
						help_key: "remote_node_ingress_profile_local_path_hint",
						kind: "text",
						label_key: "base_path",
						name: "base_path",
						placeholder: "tenant-a/incoming",
						required: true,
						secret: false,
					},
				],
				label_key: "remote_node_ingress_profile_driver_local",
			},
		]);
		adminRemoteNodeServiceMocks.listStorageTargets.mockResolvedValue([
			{
				applied_revision: 1,
				base_path: "incoming",
				bucket: "",
				created_at: "2026-05-01T00:00:00Z",
				desired_revision: 1,
				driver_type: "local",
				endpoint: "",
				is_default: true,
				last_error: "",
				max_file_size: 0,
				name: "Default ingress",
				target_key: "default",
				updated_at: "2026-05-02T00:00:00Z",
			},
		]);
		adminRemoteNodeServiceMocks.testConnection.mockResolvedValue({
			base_url: "https://edge.example.com",
			enrollment_status: "completed",
			id: 7,
			name: "Edge Alpha updated",
		});
		adminRemoteNodeServiceMocks.update.mockResolvedValue({
			base_url: "https://edge.example.com",
			id: 7,
			name: "Edge Beta",
		});
		adminRemoteNodeServiceMocks.updateStorageTarget.mockResolvedValue(
			undefined,
		);
	});

	it("blocks the create dialog when the primary public site URL is not set", () => {
		renderPage();

		fireEvent.click(screen.getByRole("button", { name: "new_remote_node" }));

		expect(mockState.toastError).toHaveBeenCalledWith(
			"remote_node_primary_site_url_required",
		);
		expect(screen.queryByTestId("remote-node-dialog")).not.toBeInTheDocument();
	});

	it("opens the create dialog when the primary public site URL is set", () => {
		mockState.frontendConfigSiteUrl = "https://drive.example.com";

		renderPage();

		fireEvent.click(screen.getByRole("button", { name: "new_remote_node" }));

		expect(mockState.toastError).not.toHaveBeenCalled();
		expect(screen.getByTestId("remote-node-dialog")).toHaveTextContent(
			"create",
		);
	});

	it("creates a remote node, prepares enrollment, copies and verifies the command", async () => {
		mockState.frontendConfigSiteUrl = "https://drive.example.com";
		mockState.useApiList.mockReturnValue({
			items: [],
			loading: false,
			reload: mockState.reload,
			setItems: mockState.setItems,
			setTotal: mockState.setTotal,
			total: 0,
		});

		renderPage();

		fireEvent.click(screen.getByRole("button", { name: "new_remote_node" }));
		fireEvent.click(screen.getByRole("button", { name: "change-name" }));
		fireEvent.click(screen.getByRole("button", { name: "create-next" }));
		fireEvent.click(screen.getByRole("button", { name: "change-base-url" }));
		fireEvent.click(screen.getByRole("button", { name: "create-next" }));
		fireEvent.click(screen.getByRole("button", { name: "submit-node" }));

		await screen.findByTestId("enrollment-dialog");
		expect(adminRemoteNodeServiceMocks.create).toHaveBeenCalledWith({
			base_url: "https://edge.example.com",
			is_enabled: true,
			name: "Edge Beta",
			transport_mode: "direct",
		});
		expect(
			adminRemoteNodeServiceMocks.createEnrollmentCommand,
		).toHaveBeenCalledWith(9);
		expect(screen.getByTestId("can-test")).toHaveTextContent("true");
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"remote_node_enrollment_prepared",
		);

		fireEvent.click(screen.getByRole("button", { name: "copy-command" }));
		await waitFor(() => {
			expect(mockState.clipboard).toHaveBeenCalledWith(
				"asterdrive-node enroll token",
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"core:copied_to_clipboard",
		);

		fireEvent.click(screen.getByRole("button", { name: "verify-command" }));
		await waitFor(() => {
			expect(adminRemoteNodeServiceMocks.testConnection).toHaveBeenCalledWith(
				9,
			);
		});
		expect(mockState.setItems).toHaveBeenCalled();
	});

	it("opens completed nodes for editing and manages remote storage targets", async () => {
		const node = {
			base_url: "https://edge.example.com",
			enrollment_status: "completed",
			id: 7,
			name: "Edge Alpha",
		};
		mockState.useApiList.mockReturnValue({
			items: [node],
			loading: false,
			reload: mockState.reload,
			setItems: mockState.setItems,
			setTotal: mockState.setTotal,
			total: 1,
		});

		renderPage();

		fireEvent.click(screen.getByRole("button", { name: "edit:7" }));

		await waitFor(() => {
			expect(
				adminRemoteNodeServiceMocks.listStorageTargets,
			).toHaveBeenCalledWith(7);
		});
		expect(
			adminRemoteNodeServiceMocks.listStorageTargetDrivers,
		).toHaveBeenCalledWith(7);
		expect(screen.getByTestId("remote-node-dialog")).toHaveTextContent("edit");
		expect(screen.getByTestId("managed-ingress-enabled")).toHaveTextContent(
			"true",
		);

		fireEvent.click(screen.getByRole("button", { name: "change-name" }));
		fireEvent.click(screen.getByRole("button", { name: "submit-node" }));

		await waitFor(() => {
			expect(adminRemoteNodeServiceMocks.update).toHaveBeenCalledWith(
				7,
				expect.objectContaining({
					base_url: "https://edge.example.com",
					name: "Edge Beta",
				}),
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("remote_node_updated");

		fireEvent.click(screen.getByRole("button", { name: "edit:7" }));
		fireEvent.click(screen.getByRole("button", { name: "create-ingress" }));
		await waitFor(() => {
			expect(
				adminRemoteNodeServiceMocks.createStorageTarget,
			).toHaveBeenCalledWith(7, { name: "Ingress" });
		});
		fireEvent.click(screen.getByRole("button", { name: "update-ingress" }));
		await waitFor(() => {
			expect(
				adminRemoteNodeServiceMocks.updateStorageTarget,
			).toHaveBeenCalledWith(7, "default", { name: "Ingress" });
		});
		fireEvent.click(screen.getByRole("button", { name: "delete-ingress" }));
		await waitFor(() => {
			expect(
				adminRemoteNodeServiceMocks.deleteStorageTarget,
			).toHaveBeenCalledWith(7, "default");
		});
	});

	it("keeps managed ingress blocked for direct nodes without base_url", async () => {
		mockState.useApiList.mockReturnValue({
			items: [
				{
					base_url: "",
					enrollment_status: "completed",
					id: 7,
					name: "Direct Empty",
					transport_mode: "direct",
				},
			],
			loading: false,
			reload: mockState.reload,
			setItems: mockState.setItems,
			setTotal: mockState.setTotal,
			total: 1,
		});

		renderPage();

		fireEvent.click(screen.getByRole("button", { name: "edit:7" }));

		expect(screen.getByTestId("managed-ingress-enabled")).toHaveTextContent(
			"true",
		);
		expect(screen.getByTestId("managed-ingress-error")).toHaveTextContent(
			"remote_node_ingress_profiles_base_url_required",
		);
		expect(
			adminRemoteNodeServiceMocks.listStorageTargets,
		).not.toHaveBeenCalled();
		expect(
			adminRemoteNodeServiceMocks.listStorageTargetDrivers,
		).not.toHaveBeenCalled();
	});

	it("loads managed remote storage targets for reverse tunnel nodes without base_url", async () => {
		mockState.useApiList.mockReturnValue({
			items: [
				{
					base_url: "",
					enrollment_status: "completed",
					id: 7,
					name: "Reverse Tunnel",
					transport_mode: "reverse_tunnel",
					tunnel: {
						last_error: "",
						last_seen_at: "2026-05-29T00:00:00Z",
						status: "online",
					},
				},
			],
			loading: false,
			reload: mockState.reload,
			setItems: mockState.setItems,
			setTotal: mockState.setTotal,
			total: 1,
		});

		renderPage();

		fireEvent.click(screen.getByRole("button", { name: "edit:7" }));

		await waitFor(() => {
			expect(
				adminRemoteNodeServiceMocks.listStorageTargets,
			).toHaveBeenCalledWith(7);
		});
		expect(
			adminRemoteNodeServiceMocks.listStorageTargetDrivers,
		).toHaveBeenCalledWith(7);
		expect(screen.getByTestId("managed-ingress-enabled")).toHaveTextContent(
			"true",
		);
		expect(screen.getByTestId("managed-ingress-error")).toBeEmptyDOMElement();
	});

	it("loads managed remote storage targets for auto nodes without base_url", async () => {
		mockState.useApiList.mockReturnValue({
			items: [
				{
					base_url: "",
					enrollment_status: "completed",
					id: 7,
					name: "Auto Tunnel",
					transport_mode: "auto",
					tunnel: {
						last_error: "",
						last_seen_at: "2026-05-29T00:00:00Z",
						status: "online",
					},
				},
			],
			loading: false,
			reload: mockState.reload,
			setItems: mockState.setItems,
			setTotal: mockState.setTotal,
			total: 1,
		});

		renderPage();

		fireEvent.click(screen.getByRole("button", { name: "edit:7" }));

		await waitFor(() => {
			expect(
				adminRemoteNodeServiceMocks.listStorageTargets,
			).toHaveBeenCalledWith(7);
		});
		expect(
			adminRemoteNodeServiceMocks.listStorageTargetDrivers,
		).toHaveBeenCalledWith(7);
		expect(screen.getByTestId("managed-ingress-enabled")).toHaveTextContent(
			"true",
		);
		expect(screen.getByTestId("managed-ingress-error")).toBeEmptyDOMElement();
	});

	it("surfaces managed ingress errors for nodes that cannot load profiles", async () => {
		adminRemoteNodeServiceMocks.listStorageTargets.mockRejectedValueOnce(
			new Error("profile failed"),
		);
		mockState.useApiList.mockReturnValue({
			items: [
				{
					base_url: "https://edge.example.com",
					enrollment_status: "completed",
					id: 7,
					name: "Edge Alpha",
				},
			],
			loading: false,
			reload: mockState.reload,
			setItems: mockState.setItems,
			setTotal: mockState.setTotal,
			total: 1,
		});

		renderPage();

		fireEvent.click(screen.getByRole("button", { name: "edit:7" }));

		await waitFor(() => {
			expect(screen.getByTestId("managed-ingress-error")).toHaveTextContent(
				"Error: profile failed",
			);
		});
		expect(mockState.handleApiError).toHaveBeenCalledWith(expect.any(Error));
	});

	it("surfaces managed ingress driver descriptor errors without canceling profile loading", async () => {
		adminRemoteNodeServiceMocks.listStorageTargetDrivers.mockRejectedValueOnce(
			new Error("descriptor failed"),
		);
		mockState.useApiList.mockReturnValue({
			items: [
				{
					base_url: "https://edge.example.com",
					enrollment_status: "completed",
					id: 7,
					name: "Edge Alpha",
				},
			],
			loading: false,
			reload: mockState.reload,
			setItems: mockState.setItems,
			setTotal: mockState.setTotal,
			total: 1,
		});

		renderPage();

		fireEvent.click(screen.getByRole("button", { name: "edit:7" }));

		await waitFor(() => {
			expect(
				adminRemoteNodeServiceMocks.listStorageTargets,
			).toHaveBeenCalledWith(7);
			expect(
				adminRemoteNodeServiceMocks.listStorageTargetDrivers,
			).toHaveBeenCalledWith(7);
		});
		await waitFor(() => {
			expect(
				screen.getByTestId("managed-ingress-driver-error"),
			).toHaveTextContent("Error: descriptor failed");
		});
		expect(
			screen.getByTestId("managed-ingress-driver-count"),
		).toHaveTextContent("0");
		expect(
			screen.getByTestId("managed-ingress-profile-count"),
		).toHaveTextContent("1");
		expect(screen.getByTestId("managed-ingress-error")).toBeEmptyDOMElement();
		expect(mockState.handleApiError).toHaveBeenCalledWith(expect.any(Error));
	});

	it("handles enrollment generation, completed-node guard, refresh, sorting and deletion", async () => {
		const nodes = [
			{
				base_url: "",
				enrollment_status: "pending",
				id: 7,
				name: "Edge Alpha",
			},
			{
				base_url: "https://done.example.com",
				enrollment_status: "completed",
				id: 8,
				name: "Edge Done",
			},
		];
		mockState.useApiList.mockReturnValue({
			items: nodes,
			loading: false,
			reload: mockState.reload,
			setItems: mockState.setItems,
			setTotal: mockState.setTotal,
			total: 2,
		});

		renderPage();

		fireEvent.click(screen.getByRole("button", { name: "sort-name" }));
		expect(mockState.setSearchParams).toHaveBeenLastCalledWith(
			expect.any(URLSearchParams),
			{ replace: true },
		);

		fireEvent.click(screen.getByRole("button", { name: "core:refresh" }));
		await waitFor(() => {
			expect(adminRemoteNodeServiceMocks.list).toHaveBeenCalled();
		});
		expect(mockState.setItems).toHaveBeenCalledWith([]);
		expect(mockState.setTotal).toHaveBeenCalledWith(0);

		fireEvent.click(screen.getByRole("button", { name: "enroll:7" }));
		await screen.findByTestId("enrollment-dialog");
		expect(screen.getByTestId("can-test")).toHaveTextContent("false");

		fireEvent.click(screen.getByRole("button", { name: "close-enrollment" }));
		expect(screen.queryByTestId("enrollment-dialog")).not.toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: "enroll:8" }));
		expect(mockState.toastInfo).toHaveBeenCalledWith(
			"remote_node_enrollment_completed_action_disabled",
		);

		fireEvent.click(screen.getByRole("button", { name: "delete:7" }));
		expect(
			screen.getByText('delete_remote_node "Edge Alpha"?'),
		).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "core:delete" }));
		await waitFor(() => {
			expect(adminRemoteNodeServiceMocks.delete).toHaveBeenCalledWith(7);
		});
		expect(mockState.reload).toHaveBeenCalled();
	});
});
