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
import {
	invalidateAdminStorageDriverDescriptors,
	primeAdminStorageDriverDescriptors,
} from "@/lib/adminStorageDriverDescriptors";
import AdminPoliciesPage from "@/pages/admin/AdminPoliciesPage";
import { ApiError } from "@/services/http";
import type { StorageConnectorFieldDescriptor } from "@/types/api";
import { ApiErrorCode } from "@/types/api-helpers";

const mockState = vi.hoisted(() => ({
	create: vi.fn(),
	dryRunMigration: vi.fn(),
	executeDraftPolicyAction: vi.fn(),
	executeSavedPolicyAction: vi.fn(),
	getPolicy: vi.fn(),
	createMigration: vi.fn(),
	deletePolicy: vi.fn(),
	handleApiError: vi.fn(),
	items: [] as Array<Record<string, unknown>>,
	listAllPolicies: vi.fn(),
	listPolicies: vi.fn(),
	listRemoteNodes: vi.fn(),
	listStorageTargets: vi.fn(),
	listStorageDriverDescriptors: vi.fn(),
	listStorageCredentials: vi.fn(),
	loading: false,
	navigate: vi.fn(),
	promoteS3CompatibleDriver: vi.fn(),
	reload: vi.fn(),
	remoteNodes: [] as Array<Record<string, unknown>>,
	remoteStorageTargetsByNode: {} as Record<
		number,
		Array<Record<string, unknown>>
	>,
	searchParams: "",
	setSearchParams: vi.fn(),
	testConnection: vi.fn(),
	testParams: vi.fn(),
	startStorageAuthorization: vi.fn(),
	total: 0,
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
	update: vi.fn(),
	validateStorageCredential: vi.fn(),
}));

vi.mock("react-router-dom", () => ({
	useNavigate: () => mockState.navigate,
	useSearchParams: () => [
		new URLSearchParams(mockState.searchParams),
		mockState.setSearchParams,
	],
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, values?: Record<string, string | number>) => {
			if (typeof values?.driver === "string") {
				return `${key}:${values.driver}`;
			}
			switch (key) {
				case "driver_type_local":
					return "Local";
				case "driver_type_s3":
					return "S3";
				case "driver_type_tencent_cos":
					return "Tencent COS";
				case "driver_type_azure_blob":
					return "Azure Blob";
				case "driver_type_remote":
					return "Remote";
				case "driver_type_onedrive":
					return "OneDrive";
				case "azure_blob_account_name":
					return "Account Name";
				case "azure_blob_account_key":
					return "Account Key";
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
		error: (...args: unknown[]) => mockState.toastError(...args),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/lib/publicSiteUrl", () => ({
	getPublicSiteUrl: () => "https://drive.example.com",
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

vi.mock("@/components/common/AnimatedCollapsible", () => ({
	AnimatedCollapsible: ({
		children,
		open,
	}: {
		children: React.ReactNode;
		open: boolean;
	}) => (open ? <div>{children}</div> : null),
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
			disabled,
			value,
		}: {
			children: React.ReactNode;
			disabled?: boolean;
			value: string;
		}) => {
			const context = useContext(SelectContext);

			return (
				<button
					type="button"
					aria-label={`select-item:${value}`}
					disabled={context.disabled || disabled}
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
		SelectValue: ({
			children,
			placeholder,
		}: {
			children?: React.ReactNode;
			placeholder?: string;
		}) => <span>{children ?? placeholder ?? "select-value"}</span>,
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
		createMigration: (...args: unknown[]) => mockState.createMigration(...args),
		dryRunMigration: (...args: unknown[]) => mockState.dryRunMigration(...args),
		delete: (...args: unknown[]) => mockState.deletePolicy(...args),
		executeDraftPolicyAction: (...args: unknown[]) =>
			mockState.executeDraftPolicyAction(...args),
		executeSavedPolicyAction: (...args: unknown[]) =>
			mockState.executeSavedPolicyAction(...args),
		get: (...args: unknown[]) => mockState.getPolicy(...args),
		getCapacity: vi.fn(async () => ({
			blob_count: 2,
			blob_total_bytes: 1024,
			capacity: {
				available_bytes: 1024,
				observed_at: "2026-03-28T00:00:00Z",
				source: "local_filesystem",
				status: "supported",
				total_bytes: 2048,
				used_bytes: 1024,
			},
			driver_type: "local",
			policy_id: 1,
		})),
		list: (...args: unknown[]) => mockState.listPolicies(...args),
		listAll: (...args: unknown[]) => mockState.listAllPolicies(...args),
		listStorageDriverDescriptors: (...args: unknown[]) =>
			mockState.listStorageDriverDescriptors(...args),
		listStorageCredentials: (...args: unknown[]) =>
			mockState.listStorageCredentials(...args),
		promoteS3CompatibleDriver: (...args: unknown[]) =>
			mockState.promoteS3CompatibleDriver(...args),
		startStorageAuthorization: (...args: unknown[]) =>
			mockState.startStorageAuthorization(...args),
		testConnection: (...args: unknown[]) => mockState.testConnection(...args),
		testParams: (...args: unknown[]) => mockState.testParams(...args),
		update: (...args: unknown[]) => mockState.update(...args),
		validateStorageCredential: (...args: unknown[]) =>
			mockState.validateStorageCredential(...args),
	},
	adminRemoteNodeService: {
		list: (...args: unknown[]) => mockState.listRemoteNodes(...args),
		listStorageTargets: (...args: unknown[]) =>
			mockState.listStorageTargets(...args),
	},
}));

type TestStorageFieldDescriptor = StorageConnectorFieldDescriptor;

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
		remote_storage_target_key: null,
		updated_at: "2026-03-28T00:00:00Z",
		...overrides,
	};
}

function createRemoteStorageTarget(overrides: Record<string, unknown> = {}) {
	return {
		applied_revision: 1,
		base_path: "",
		bucket: "",
		created_at: "2026-03-28T00:00:00Z",
		desired_revision: 1,
		driver_type: "local",
		endpoint: "",
		is_default: false,
		last_error: "",
		max_file_size: 0,
		name: "Default Target",
		target_key: "rst_default",
		updated_at: "2026-03-28T00:00:00Z",
		...overrides,
	};
}

function fieldDescriptor(
	name: string,
	scope: TestStorageFieldDescriptor["scope"],
	kind: TestStorageFieldDescriptor["kind"],
	overrides: Partial<TestStorageFieldDescriptor> = {},
): TestStorageFieldDescriptor {
	return {
		kind,
		label_key: name,
		name,
		required: false,
		scope,
		secret: kind === "secret",
		...overrides,
	};
}

function objectStorageConnectionFields(
	driverType: "s3" | "tencent_cos" | "azure_blob",
) {
	const endpointDisplayByDriver = {
		azure_blob: {
			help_key: "azure_blob_endpoint_hint",
			placeholder: "https://<account>.blob.core.windows.net",
		},
		s3: {
			help_key: "s3_endpoint_hint",
			placeholder: "https://s3.amazonaws.com",
		},
		tencent_cos: {
			help_key: "cos_endpoint_hint",
			placeholder: "https://<bucket-appid>.cos.<region>.myqcloud.com",
		},
	} satisfies Record<
		typeof driverType,
		Pick<TestStorageFieldDescriptor, "help_key" | "placeholder">
	>;

	return [
		fieldDescriptor("endpoint", "connection", "text", {
			...endpointDisplayByDriver[driverType],
			invalid_protocol_message_key:
				driverType === "azure_blob"
					? "azure_blob_endpoint_protocol_required_error"
					: "s3_endpoint_protocol_required_error",
			required: true,
		}),
		fieldDescriptor("bucket", "connection", "text", {
			required: true,
			required_message_key:
				driverType === "azure_blob"
					? "policy_wizard_container_required"
					: "policy_wizard_bucket_required",
		}),
		fieldDescriptor("access_key", "connection", "text", {
			label_key:
				driverType === "azure_blob" ? "azure_blob_account_name" : "access_key",
			required: true,
			trim_on_blur: driverType === "azure_blob",
		}),
		fieldDescriptor("secret_key", "connection", "secret", {
			label_key:
				driverType === "azure_blob" ? "azure_blob_account_key" : "secret_key",
			required: true,
		}),
		fieldDescriptor("base_path", "connection", "text"),
	];
}

const objectStoragePolicyOptionFields = [
	fieldDescriptor(
		"object_storage_upload_strategy",
		"policy_options",
		"select",
		{
			options: ["relay_stream", "presigned"],
			required: true,
		},
	),
	fieldDescriptor(
		"object_storage_download_strategy",
		"policy_options",
		"select",
		{
			options: ["relay_stream", "presigned"],
			required: true,
		},
	),
] as const;

const s3PolicyOptionFields = [
	...objectStoragePolicyOptionFields,
	fieldDescriptor("s3_path_style", "policy_options", "boolean", {
		help_key: "s3_path_style_desc",
		visible_when_driver_types: ["s3"],
	}),
] as const;

const remotePolicyOptionFields = [
	fieldDescriptor("remote_download_strategy", "policy_options", "select", {
		options: ["relay_stream", "presigned"],
		required: true,
	}),
	fieldDescriptor("remote_upload_strategy", "policy_options", "select", {
		options: ["relay_stream", "presigned"],
		required: true,
	}),
] as const;

function storageConnectorUi(driverType: string) {
	const sharedObjectStorageUi = {
		base_path_empty_display: "core:root",
		base_path_placeholder: "tenant/prefix",
		config_step_title_key: "policy_wizard_step_connection_title",
		edit_context_key: "policy_edit_context_object_storage_desc",
	};

	switch (driverType) {
		case "local":
			return {
				base_path_empty_display: "./data",
				base_path_placeholder: "./data",
				config_step_description_key: "policy_wizard_step_local_desc",
				config_step_title_key: "policy_wizard_step_local_title",
				description_key: "policy_wizard_local_storage_desc",
				edit_context_key: "policy_edit_context_local_desc",
				helper_key: "policy_wizard_local_helper",
				icon_name: null,
				icon_src: "/static/asterdrive/asterdrive-dark.svg",
				label_key: "driver_type_local",
			};
		case "remote":
			return {
				base_path_empty_display: "core:root",
				base_path_placeholder: "workspace/path",
				config_step_description_key: "policy_wizard_step_remote_desc",
				config_step_title_key: "policy_wizard_step_remote_title",
				description_key: "policy_wizard_remote_storage_desc",
				edit_context_key: "policy_edit_context_remote_desc",
				helper_key: "policy_wizard_remote_helper",
				icon_name: null,
				icon_src: "/static/storage/asterdrive-node.svg",
				label_key: "driver_type_remote",
			};
		case "tencent_cos":
			return {
				...sharedObjectStorageUi,
				config_step_description_key:
					"policy_wizard_step_tencent_cos_connection_desc",
				description_key: "policy_wizard_tencent_cos_storage_desc",
				helper_key: "policy_wizard_tencent_cos_helper",
				icon_name: null,
				icon_src: "/static/storage/tencent-cloud-cos.webp",
				label_key: "driver_type_tencent_cos",
			};
		case "azure_blob":
			return {
				...sharedObjectStorageUi,
				config_step_description_key:
					"policy_wizard_step_azure_blob_connection_desc",
				description_key: "policy_wizard_azure_blob_storage_desc",
				helper_key: "policy_wizard_azure_blob_helper",
				icon_name: null,
				icon_src: "/static/storage/azure-blob.svg",
				label_key: "driver_type_azure_blob",
			};
		case "one_drive":
			return {
				base_path_empty_display: "policy_base_path_root",
				base_path_placeholder: "Documents/Projects",
				config_step_description_key: "policy_wizard_step_onedrive_desc",
				config_step_title_key: "policy_wizard_step_onedrive_title",
				description_key: "policy_wizard_onedrive_storage_desc",
				edit_context_key: "policy_edit_context_onedrive_desc",
				helper_key: "policy_wizard_onedrive_helper",
				icon_name: null,
				icon_src: "/static/storage/onedrive.svg",
				label_key: "driver_type_onedrive",
			};
		default:
			return {
				...sharedObjectStorageUi,
				config_step_description_key:
					"policy_wizard_step_object_storage_connection_desc",
				description_key: "policy_wizard_s3_storage_desc",
				helper_key: "policy_wizard_object_storage_helper",
				icon_name: null,
				icon_src: "/static/storage/s3.svg",
				label_key: "driver_type_s3",
			};
	}
}

function createStorageDriverDescriptor(
	driverType: string,
	overrides: Record<string, unknown> = {},
) {
	const defaultActions = [
		{
			affordance_action: "test_draft_connection",
			endpoints: ["test_policy_params"],
			kind: "connection_test",
			mutates_remote_state: false,
			requires_authorization: false,
			requires_saved_policy: false,
		},
		{
			affordance_action: "test_saved_connection",
			endpoints: ["test_policy_connection"],
			kind: "connection_test",
			mutates_remote_state: false,
			requires_authorization: false,
			requires_saved_policy: true,
		},
	] as const;

	return {
		actions: defaultActions,
		authorization_provider: null,
		capabilities: {
			capacity: true,
			efficient_range: true,
			list: true,
			presigned_download: false,
			remote_node_binding: false,
			object_storage_transfer_strategy: false,
			storage_native_media_metadata: false,
			storage_native_thumbnail: false,
		},
		credential_mode: "none",
		description: `${driverType} descriptor`,
		driver_type: driverType,
		enabled: true,
		fields: [],
		label: driverType,
		driver_recommendations: [],
		related_issues: [328],
		requires_authorization: false,
		ui: storageConnectorUi(driverType),
		upload_workflows: {
			frontend_direct_provider_resumable_upload: false,
			object_multipart_upload: false,
			presigned_upload: false,
			provider_resumable_upload: false,
			simple_upload: true,
			stream_upload: true,
		},
		...overrides,
	};
}

function createStorageDriverDescriptors() {
	return [
		createStorageDriverDescriptor("local", {
			fields: [
				{
					kind: "text",
					name: "base_path",
					required: false,
					scope: "connection",
					secret: false,
				},
				{
					kind: "boolean",
					name: "content_dedup",
					required: false,
					scope: "policy_options",
					secret: false,
				},
			],
		}),
		createStorageDriverDescriptor("remote", {
			capabilities: {
				...createStorageDriverDescriptor("remote").capabilities,
				remote_node_binding: true,
			},
			fields: [
				{
					kind: "select",
					name: "remote_node_id",
					required: true,
					scope: "remote_node_binding",
					secret: false,
				},
				{
					kind: "text",
					name: "base_path",
					required: false,
					scope: "connection",
					secret: false,
				},
				...remotePolicyOptionFields,
			],
			upload_workflows: {
				...createStorageDriverDescriptor("remote").upload_workflows,
				object_multipart_upload: true,
				presigned_upload: true,
			},
		}),
		createStorageDriverDescriptor("s3", {
			capabilities: {
				...createStorageDriverDescriptor("s3").capabilities,
				presigned_download: true,
				object_storage_transfer_strategy: true,
			},
			driver_recommendations: [
				{
					target_driver_type: "tencent_cos",
					endpoint_host_rules: [
						{ equals: "myqcloud.com" },
						{ ends_with: ".myqcloud.com" },
					],
				},
			],
			fields: [...objectStorageConnectionFields("s3"), ...s3PolicyOptionFields],
			upload_workflows: {
				...createStorageDriverDescriptor("s3").upload_workflows,
				object_multipart_upload: true,
				presigned_upload: true,
			},
		}),
		createStorageDriverDescriptor("tencent_cos", {
			actions: [
				{
					affordance_action: "test_draft_connection",
					endpoints: ["test_policy_params"],
					kind: "connection_test",
					mutates_remote_state: false,
					requires_authorization: false,
					requires_saved_policy: false,
				},
				{
					affordance_action: "test_saved_connection",
					endpoints: ["test_policy_connection"],
					kind: "connection_test",
					mutates_remote_state: false,
					requires_authorization: false,
					requires_saved_policy: true,
				},
				{
					policy_action: "configure_tencent_cos_cors",
					endpoints: [
						"execute_draft_storage_policy_action",
						"execute_saved_storage_policy_action",
					],
					kind: "policy_action",
					mutates_remote_state: true,
					requires_authorization: false,
					requires_saved_policy: false,
				},
			],
			capabilities: {
				...createStorageDriverDescriptor("tencent_cos").capabilities,
				presigned_download: true,
				object_storage_transfer_strategy: true,
				storage_native_media_metadata: true,
				storage_native_thumbnail: true,
			},
			fields: [
				...objectStorageConnectionFields("tencent_cos"),
				...objectStoragePolicyOptionFields,
			],
			upload_workflows: {
				...createStorageDriverDescriptor("tencent_cos").upload_workflows,
				object_multipart_upload: true,
				presigned_upload: true,
			},
		}),
		createStorageDriverDescriptor("azure_blob", {
			capabilities: {
				...createStorageDriverDescriptor("azure_blob").capabilities,
				presigned_download: true,
				object_storage_transfer_strategy: true,
			},
			fields: [
				...objectStorageConnectionFields("azure_blob"),
				...objectStoragePolicyOptionFields,
			],
			upload_workflows: {
				...createStorageDriverDescriptor("azure_blob").upload_workflows,
				object_multipart_upload: true,
				presigned_upload: true,
			},
		}),
		createStorageDriverDescriptor("one_drive", {
			actions: [
				{
					affordance_action: "start_authorization",
					endpoints: ["start_storage_authorization"],
					kind: "authorization",
					mutates_remote_state: false,
					requires_authorization: false,
					requires_saved_policy: true,
				},
				{
					affordance_action: "validate_credential",
					endpoints: ["validate_storage_policy_credential"],
					kind: "credential_validation",
					mutates_remote_state: false,
					requires_authorization: true,
					requires_saved_policy: true,
				},
				{
					affordance_action: "test_saved_connection",
					endpoints: ["test_policy_connection"],
					kind: "connection_test",
					mutates_remote_state: false,
					requires_authorization: true,
					requires_saved_policy: true,
				},
			],
			authorization_provider: "microsoft_graph",
			capabilities: {
				...createStorageDriverDescriptor("one_drive").capabilities,
				list: false,
			},
			upload_workflows: {
				...createStorageDriverDescriptor("one_drive").upload_workflows,
				provider_resumable_upload: true,
			},
			credential_mode: "oauth_delegated",
			fields: [
				fieldDescriptor("client_id", "application_credential", "text", {
					required: true,
				}),
				fieldDescriptor("client_secret", "application_credential", "secret", {
					required: true,
				}),
				fieldDescriptor("cloud", "policy_options", "select", {
					options: ["global", "china"],
					required: true,
				}),
				fieldDescriptor("account_mode", "policy_options", "select", {
					options: [
						"personal",
						"work_or_school",
						"sharepoint_site",
						"group_drive",
					],
					required: true,
				}),
				fieldDescriptor("tenant", "policy_options", "text"),
				fieldDescriptor("drive_id", "policy_options", "text"),
				fieldDescriptor("root_item_id", "policy_options", "text"),
				fieldDescriptor("site_id", "policy_options", "text"),
				fieldDescriptor("group_id", "policy_options", "text"),
			],
			requires_authorization: true,
		}),
	];
}

function openCreateWizard(
	driver:
		| "local"
		| "remote"
		| "s3"
		| "tencent_cos"
		| "azure_blob"
		| "one_drive" = "local",
) {
	fireEvent.click(screen.getByRole("button", { name: /new_policy/i }));
	if (driver === "local") {
		fireEvent.click(screen.getByRole("button", { name: /^Local\b/ }));
	} else if (driver === "remote") {
		fireEvent.click(screen.getByRole("button", { name: /^Remote\b/ }));
	} else if (driver === "s3") {
		fireEvent.click(screen.getByRole("button", { name: /^S3\b/ }));
	} else if (driver === "tencent_cos") {
		fireEvent.click(screen.getByRole("button", { name: /Tencent COS/ }));
	} else if (driver === "azure_blob") {
		fireEvent.click(screen.getByRole("button", { name: /Azure Blob/ }));
	} else if (driver === "one_drive") {
		fireEvent.click(screen.getByRole("button", { name: /OneDrive/ }));
	}
}

function advanceCreateWizardToRulesStep() {
	fireEvent.click(screen.getByRole("button", { name: "policy_wizard_review" }));
}

function openEditPolicy(name: string) {
	fireEvent.click(screen.getByText(name));
}

async function openMigrationDialog() {
	fireEvent.click(
		screen.getByRole("button", {
			name: /policy_migration_action/,
		}),
	);
	await screen.findByText("policy_migration_title");
}

describe("AdminPoliciesPage", () => {
	beforeEach(() => {
		mockState.executeDraftPolicyAction.mockReset();
		mockState.executeSavedPolicyAction.mockReset();
		mockState.getPolicy.mockReset();
		mockState.create.mockReset();
		mockState.dryRunMigration.mockReset();
		mockState.createMigration.mockReset();
		mockState.deletePolicy.mockReset();
		mockState.handleApiError.mockReset();
		invalidateAdminRemoteNodeLookup();
		invalidateAdminStorageDriverDescriptors();
		mockState.items = [];
		mockState.listAllPolicies.mockReset();
		mockState.listPolicies.mockReset();
		mockState.listRemoteNodes.mockReset();
		mockState.listStorageTargets.mockReset();
		mockState.listStorageDriverDescriptors.mockReset();
		mockState.listStorageCredentials.mockReset();
		mockState.loading = false;
		mockState.navigate.mockReset();
		mockState.promoteS3CompatibleDriver.mockReset();
		mockState.reload.mockReset();
		mockState.remoteNodes = [];
		mockState.remoteStorageTargetsByNode = {};
		mockState.searchParams = "";
		mockState.setSearchParams.mockReset();
		mockState.testConnection.mockReset();
		mockState.testParams.mockReset();
		mockState.startStorageAuthorization.mockReset();
		mockState.total = 0;
		mockState.toastError.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.update.mockReset();
		mockState.validateStorageCredential.mockReset();

		mockState.create.mockImplementation(async (payload) =>
			createPolicy({
				...(payload as Record<string, unknown>),
				id: 99,
			}),
		);
		mockState.createMigration.mockResolvedValue({
			id: 42,
			kind: "storage_policy_migration",
		});
		mockState.dryRunMigration.mockResolvedValue({
			can_start: true,
			content_sha256_blob_count: 2,
			delete_source_after_success_supported: false,
			estimated_copy_blob_count: 4,
			opaque_key_conflict_count: 0,
			opaque_blob_count: 3,
			source_blob_count: 5,
			source_policy_id: 1,
			source_total_bytes: 1536,
			target_capacity: {
				available_bytes: null,
				observed_at: new Date().toISOString(),
				source: "local_filesystem",
				status: "unavailable",
				total_bytes: null,
				used_bytes: null,
			},
			target_capacity_check: "unavailable",
			target_connection_ok: true,
			target_matching_blob_count: 1,
			target_policy_id: 2,
			target_supports_stream_upload: true,
			warnings: [],
		});
		mockState.deletePolicy.mockImplementation(async (id: number) => {
			mockState.items = mockState.items.filter((policy) => policy.id !== id);
		});
		mockState.reload.mockResolvedValue(undefined);
		mockState.listRemoteNodes.mockImplementation(async () => ({
			items: mockState.remoteNodes,
			total: mockState.remoteNodes.length,
		}));
		mockState.listStorageTargets.mockImplementation(async (nodeId: number) =>
			(
				mockState.remoteStorageTargetsByNode[nodeId] ?? [
					createRemoteStorageTarget(),
				]
			).map((target) => ({ ...target })),
		);
		mockState.listStorageDriverDescriptors.mockResolvedValue(
			createStorageDriverDescriptors(),
		);
		primeAdminStorageDriverDescriptors(
			createStorageDriverDescriptors() as never,
		);
		mockState.listPolicies.mockImplementation(async () => ({
			items: mockState.items,
			total: mockState.total || mockState.items.length,
		}));
		mockState.getPolicy.mockImplementation(async (id: number) => {
			const policy = mockState.items.find((item) => item.id === id);
			if (!policy) {
				throw new Error(`policy ${id} not found`);
			}
			return policy;
		});
		mockState.listAllPolicies.mockImplementation(async () => mockState.items);
		mockState.listStorageCredentials.mockResolvedValue([]);
		mockState.startStorageAuthorization.mockResolvedValue({
			authorization_url: "https://login.example.test/authorize",
		});
		mockState.testConnection.mockResolvedValue({ ok: true });
		mockState.testParams.mockResolvedValue({ ok: true });
		mockState.executeDraftPolicyAction.mockResolvedValue({
			action: "configure_tencent_cos_cors",
			tencent_cos_cors: {
				allowed_origins: ["https://drive.example.com"],
				preserved_rule_count: 1,
				replaced_existing_rule: false,
				request_id: "cos-request-draft",
				response_vary: true,
				rule_id: "asterdrive-presigned-access",
			},
		});
		mockState.executeSavedPolicyAction.mockResolvedValue({
			action: "configure_tencent_cos_cors",
			tencent_cos_cors: {
				allowed_origins: ["https://drive.example.com"],
				preserved_rule_count: 2,
				replaced_existing_rule: true,
				request_id: "cos-request-saved",
				response_vary: true,
				rule_id: "asterdrive-presigned-access",
			},
		});
		mockState.promoteS3CompatibleDriver.mockImplementation(
			async (id, payload) =>
				createPolicy({
					...((mockState.items.find((policy) => policy.id === id) ??
						{}) as Record<string, unknown>),
					driver_type: (payload as { target_driver_type: string })
						.target_driver_type,
					id,
				}),
		);
		mockState.update.mockImplementation(async (id, payload) =>
			createPolicy({
				...((mockState.items.find((policy) => policy.id === id) ??
					{}) as Record<string, unknown>),
				...(payload as Record<string, unknown>),
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

	it("shows OneDrive authorization success returned from callback and refreshes policies", async () => {
		mockState.searchParams =
			"storage_authorization=success&policy_id=12&sortBy=name";
		mockState.items = [createPolicy({ id: 1, name: "Current Page Local" })];
		mockState.getPolicy.mockResolvedValue(
			createPolicy({
				id: 12,
				name: "Authorized OneDrive",
				driver_type: "one_drive",
				options: {
					onedrive_account_mode: "work_or_school",
					onedrive_cloud: "global",
					onedrive_root_item_id: "root",
					onedrive_tenant: "common",
				},
			}),
		);
		mockState.listStorageCredentials.mockResolvedValue([
			{
				account_label: "root",
				authorized_at: "2026-06-15T10:20:00Z",
				created_at: "2026-06-15T10:20:00Z",
				credential_kind: "oauth_delegated",
				expires_at: null,
				id: 7,
				last_refreshed_at: null,
				last_validated_at: null,
				policy_id: 12,
				provider: "microsoft_graph",
				scopes: ["offline_access", "Files.ReadWrite.All"],
				status: "authorized",
				status_reason: null,
				subject: "root-id",
				tenant_id: "common",
				updated_at: "2026-06-15T10:20:00Z",
			},
		]);

		render(<AdminPoliciesPage />);

		await waitFor(() => {
			expect(mockState.toastSuccess).toHaveBeenCalledWith(
				"onedrive_authorization_completed",
				{
					description: "onedrive_authorization_completed_policy",
				},
			);
		});
		expect(mockState.reload).toHaveBeenCalled();
		await waitFor(() => {
			expect(mockState.getPolicy).toHaveBeenCalledWith(12);
		});
		expect(mockState.listStorageCredentials).toHaveBeenCalledWith(12);
		expect(
			await screen.findByDisplayValue("Authorized OneDrive"),
		).toBeInTheDocument();
		expect(
			await screen.findByText("onedrive_credential_status_authorized"),
		).toBeInTheDocument();
		const cleanupCall = mockState.setSearchParams.mock.calls.find(
			([params]) =>
				params instanceof URLSearchParams &&
				!params.has("storage_authorization") &&
				!params.has("policy_id") &&
				params.get("sortBy") === "name",
		);
		expect(cleanupCall).toBeTruthy();
	});

	it("handles identical OneDrive authorization callbacks after the previous one completes", async () => {
		const callbackSearch =
			"storage_authorization=success&policy_id=12&sortBy=name";
		mockState.searchParams = callbackSearch;
		mockState.getPolicy.mockResolvedValue(
			createPolicy({
				id: 12,
				name: "Authorized OneDrive",
				driver_type: "one_drive",
				options: {
					onedrive_account_mode: "work_or_school",
					onedrive_cloud: "global",
					onedrive_root_item_id: "root",
					onedrive_tenant: "common",
				},
			}),
		);
		mockState.listStorageCredentials.mockResolvedValue([]);

		const { rerender } = render(<AdminPoliciesPage />);

		await waitFor(() => {
			expect(mockState.toastSuccess).toHaveBeenCalledTimes(1);
		});
		expect(mockState.reload).toHaveBeenCalledTimes(1);

		mockState.searchParams = "";
		rerender(<AdminPoliciesPage />);
		mockState.searchParams = callbackSearch;
		rerender(<AdminPoliciesPage />);

		await waitFor(() => {
			expect(mockState.toastSuccess).toHaveBeenCalledTimes(2);
		});
		expect(mockState.reload).toHaveBeenCalledTimes(2);
	});

	it("shows OneDrive authorization callback failures without refreshing policies", async () => {
		mockState.searchParams =
			"storage_authorization=error&policy_id=12&reason=invalid_state";

		render(<AdminPoliciesPage />);

		await waitFor(() => {
			expect(mockState.toastError).toHaveBeenCalledWith(
				"onedrive_authorization_failed_invalid_state",
			);
		});
		expect(mockState.reload).not.toHaveBeenCalled();
		const cleanupCall = mockState.setSearchParams.mock.calls.find(
			([params]) =>
				params instanceof URLSearchParams &&
				!params.has("storage_authorization") &&
				!params.has("policy_id") &&
				!params.has("reason"),
		);
		expect(cleanupCall).toBeTruthy();
	});

	it("shows unsupported OneDrive authorization provider callback failures", async () => {
		mockState.searchParams =
			"storage_authorization=error&policy_id=12&reason=unsupported_provider";

		render(<AdminPoliciesPage />);

		await waitFor(() => {
			expect(mockState.toastError).toHaveBeenCalledWith(
				"onedrive_authorization_failed_unsupported_provider",
			);
		});
		expect(mockState.reload).not.toHaveBeenCalled();
	});

	it("renders Azure Blob rows with Azure-specific badge styling", () => {
		mockState.items = [
			createPolicy({
				id: 4,
				name: "Azure Archive",
				driver_type: "azure_blob",
				endpoint: "https://acct.blob.core.windows.net",
				bucket: "archive",
			}),
		];

		render(<AdminPoliciesPage />);

		expect(screen.getByText("Azure Archive")).toBeInTheDocument();
		expect(
			screen.getByText("https://acct.blob.core.windows.net"),
		).toBeInTheDocument();
		expect(screen.getByText("archive")).toBeInTheDocument();
		const azureBadge = screen.getByText("Azure Blob");
		expect(azureBadge).toHaveAttribute("data-variant", "outline");
		expect(azureBadge).toHaveClass("bg-sky-500/10", "text-sky-700");
	});

	it("renders Tencent COS and remote fallback metadata", () => {
		mockState.items = [
			createPolicy({
				id: 5,
				name: "COS Media",
				driver_type: "tencent_cos",
				endpoint: "https://cos.ap-guangzhou.myqcloud.com",
				bucket: "media-1250000000",
			}),
			createPolicy({
				id: 6,
				name: "Remote Missing",
				driver_type: "remote",
				base_path: "",
				remote_node_id: 99,
			}),
			createPolicy({
				id: 7,
				name: "Remote Unbound",
				driver_type: "remote",
				base_path: "tenant-b",
				remote_node_id: null,
			}),
		];

		render(<AdminPoliciesPage />);

		const cosBadge = screen.getByText("Tencent COS");
		expect(cosBadge).toHaveClass("bg-cyan-500/10", "text-cyan-700");
		expect(
			screen.getByText("https://cos.ap-guangzhou.myqcloud.com"),
		).toBeInTheDocument();
		expect(screen.getByText("media-1250000000")).toBeInTheDocument();
		expect(screen.getByText("core:root")).toBeInTheDocument();
		expect(screen.getByText("#99")).toBeInTheDocument();
		expect(screen.getAllByText("-").length).toBeGreaterThan(0);
	});

	it("opens policy rows from the keyboard without letting action cells bubble", () => {
		mockState.items = [
			createPolicy({
				id: 8,
				name: "Keyboard S3",
				driver_type: "s3",
				endpoint: "https://s3.example.com",
				bucket: "keyboard",
			}),
		];

		render(<AdminPoliciesPage />);

		const row = screen.getByText("Keyboard S3").closest("tr");
		const actionCell = screen
			.getByRole("button", { name: "delete_policy" })
			.closest("td");
		if (!row || !actionCell) {
			throw new Error("Expected policy row and action cell");
		}

		fireEvent.keyDown(actionCell, { key: "a" });
		expect(screen.queryByDisplayValue("Keyboard S3")).not.toBeInTheDocument();

		fireEvent.keyDown(actionCell, { key: "Enter" });
		expect(screen.queryByDisplayValue("Keyboard S3")).not.toBeInTheDocument();

		fireEvent.keyDown(row, { key: " " });
		expect(screen.getByDisplayValue("Keyboard S3")).toBeInTheDocument();
	});

	it("updates pagination offset from the policy pager", () => {
		mockState.items = [createPolicy({ id: 1, name: "Default Local" })];
		mockState.total = 45;

		render(<AdminPoliciesPage />);

		const buttons = screen.getAllByRole("button");
		fireEvent.click(buttons[buttons.length - 1]);

		expect(mockState.setSearchParams).toHaveBeenLastCalledWith(
			new URLSearchParams("offset=20"),
			{ replace: true },
		);
	});

	it("moves to the previous policy page without going below zero", () => {
		mockState.searchParams = "offset=20";
		mockState.items = [createPolicy({ id: 1, name: "Default Local" })];
		mockState.total = 45;

		render(<AdminPoliciesPage />);

		const buttons = screen.getAllByRole("button");
		fireEvent.click(buttons[buttons.length - 2]);

		expect(mockState.setSearchParams).toHaveBeenLastCalledWith(
			new URLSearchParams(""),
			{ replace: true },
		);
	});

	it("checks a storage policy migration plan before creating the task", async () => {
		mockState.items = [
			createPolicy({ id: 1, name: "Hot Local" }),
			createPolicy({
				id: 2,
				name: "Archive Local",
				driver_type: "local",
				root_path: "./archive",
			}),
		];
		mockState.dryRunMigration.mockResolvedValueOnce({
			can_start: true,
			content_sha256_blob_count: 2,
			delete_source_after_success_supported: false,
			estimated_copy_blob_count: 4,
			opaque_blob_count: 3,
			opaque_key_conflict_count: 1,
			source_blob_count: 5,
			source_policy_id: 1,
			source_total_bytes: 1536,
			target_capacity: {
				available_bytes: 4096,
				observed_at: new Date().toISOString(),
				source: "local_filesystem",
				status: "supported",
				total_bytes: 8192,
				used_bytes: 4096,
			},
			target_capacity_check: "ok",
			target_connection_ok: true,
			target_matching_blob_count: 1,
			target_policy_id: 2,
			target_supports_stream_upload: true,
			warnings: ["opaque_key_conflict"],
		});

		render(<AdminPoliciesPage />);

		await openMigrationDialog();
		expect(screen.getAllByText("#1 · Hot Local").length).toBeGreaterThan(1);
		expect(screen.getAllByText("#2 · Archive Local").length).toBeGreaterThan(1);
		expect(
			screen.getByRole("button", { name: /policy_migration_submit/ }),
		).toBeDisabled();

		fireEvent.click(
			screen.getByRole("button", { name: /policy_migration_dry_run/ }),
		);

		await waitFor(() => {
			expect(mockState.dryRunMigration).toHaveBeenCalledWith({
				source_policy_id: 1,
				target_policy_id: 2,
				delete_source_after_success: false,
			});
		});
		expect(
			screen.getByText("policy_migration_dry_run_title"),
		).toBeInTheDocument();
		expect(screen.getByText("policy_migration_can_start")).toBeInTheDocument();
		expect(
			screen.getByText("policy_migration_capacity_available_of_total"),
		).toBeInTheDocument();
		expect(
			screen.getByText("policy_migration_warning_opaque_key_conflict"),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: /policy_migration_submit/ }),
		);

		await waitFor(() => {
			expect(mockState.createMigration).toHaveBeenCalledWith({
				source_policy_id: 1,
				target_policy_id: 2,
				delete_source_after_success: false,
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"policy_migration_created",
		);
		expect(mockState.navigate).toHaveBeenCalledWith(
			"/admin/tasks?kind=storage_policy_migration",
			{ viewTransition: false },
		);
	});

	it("reports errors when opening the storage policy migration dialog fails", async () => {
		const error = new Error("list all failed");
		mockState.listAllPolicies.mockRejectedValueOnce(error);
		mockState.items = [
			createPolicy({ id: 1, name: "Hot Local" }),
			createPolicy({ id: 2, name: "Archive Local" }),
		];
		mockState.total = 2;

		render(<AdminPoliciesPage />);

		fireEvent.click(
			screen.getByRole("button", {
				name: /policy_migration_action/,
			}),
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(
			screen.queryByText("policy_migration_title"),
		).not.toBeInTheDocument();
	});

	it("invalidates a checked migration plan when the target changes", async () => {
		mockState.items = [
			createPolicy({ id: 1, name: "Hot Local" }),
			createPolicy({ id: 2, name: "Archive S3", driver_type: "s3" }),
			createPolicy({ id: 3, name: "Cold S3", driver_type: "s3" }),
		];

		render(<AdminPoliciesPage />);

		await openMigrationDialog();
		fireEvent.click(
			screen.getByRole("button", { name: /policy_migration_dry_run/ }),
		);

		await screen.findByText("policy_migration_dry_run_title");
		fireEvent.click(
			screen.getAllByRole("button", { name: "select-item:3" })[1],
		);

		expect(
			screen.queryByText("policy_migration_dry_run_title"),
		).not.toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: /policy_migration_submit/ }),
		).toBeDisabled();
		expect(mockState.createMigration).not.toHaveBeenCalled();
	});

	it("disables storage policy migration when there is no distinct target policy", () => {
		mockState.items = [createPolicy({ id: 1, name: "Only Policy" })];

		render(<AdminPoliciesPage />);

		expect(
			screen.getByRole("button", { name: /policy_migration_action/ }),
		).toBeDisabled();
	});

	it("clears a matching migration target when the source changes", async () => {
		mockState.items = [
			createPolicy({ id: 1, name: "Hot Local" }),
			createPolicy({ id: 2, name: "Archive S3", driver_type: "s3" }),
			createPolicy({ id: 3, name: "Cold S3", driver_type: "s3" }),
		];

		render(<AdminPoliciesPage />);

		await openMigrationDialog();
		fireEvent.click(
			screen.getAllByRole("button", { name: "select-item:2" })[0],
		);

		expect(
			screen.getByRole("button", { name: /policy_migration_dry_run/ }),
		).toBeDisabled();
		expect(
			screen.getByRole("button", { name: /policy_migration_submit/ }),
		).toBeDisabled();
	});

	it("prevents submitting a storage policy migration with the same source and target", async () => {
		mockState.items = [
			createPolicy({ id: 1, name: "Hot Local" }),
			createPolicy({ id: 2, name: "Archive S3", driver_type: "s3" }),
		];

		render(<AdminPoliciesPage />);

		await openMigrationDialog();

		expect(
			screen.getAllByRole("button", { name: "select-item:1" })[1],
		).toBeDisabled();
		expect(
			screen.getByRole("button", { name: /policy_migration_dry_run/ }),
		).toBeEnabled();
		expect(
			screen.getByRole("button", { name: /policy_migration_submit/ }),
		).toBeDisabled();
		expect(mockState.createMigration).not.toHaveBeenCalled();
	});

	it("keeps the storage migration dialog open and reports dry-run API errors", async () => {
		const error = new Error("migration failed");
		mockState.dryRunMigration.mockRejectedValueOnce(error);
		mockState.items = [
			createPolicy({ id: 1, name: "Hot Local" }),
			createPolicy({ id: 2, name: "Archive S3", driver_type: "s3" }),
		];

		render(<AdminPoliciesPage />);

		await openMigrationDialog();
		fireEvent.click(
			screen.getByRole("button", { name: /policy_migration_dry_run/ }),
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(screen.getByText("policy_migration_title")).toBeInTheDocument();
		expect(mockState.navigate).not.toHaveBeenCalled();
		expect(mockState.createMigration).not.toHaveBeenCalled();
		expect(mockState.toastSuccess).not.toHaveBeenCalledWith(
			"policy_migration_created",
		);
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

	it("refreshes policies and reports refresh failures", async () => {
		mockState.items = [createPolicy({ id: 4, name: "Refresh Me" })];
		mockState.remoteNodes = [{ id: 8, name: "Edge Refresh" }];

		render(<AdminPoliciesPage />);

		fireEvent.click(screen.getByRole("button", { name: /core:refresh/ }));

		await waitFor(() => {
			expect(mockState.listPolicies).toHaveBeenCalledWith({
				limit: 20,
				offset: 0,
				sort_by: "created_at",
				sort_order: "desc",
			});
		});
		expect(mockState.listRemoteNodes).toHaveBeenCalled();
		expect(screen.getByText("Refresh Me")).toBeInTheDocument();

		const error = new Error("refresh failed");
		mockState.listPolicies.mockRejectedValueOnce(error);
		fireEvent.click(screen.getByRole("button", { name: /core:refresh/ }));

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
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
				options: {},
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

	it("shows admin diagnostic details when a connection test returns an API diagnostic", async () => {
		const error = new ApiError(
			ApiErrorCode.StorageMisconfigured,
			"Storage Driver Error",
			{
				diagnostic: {
					kind: "misconfigured",
					message: "connection test failed",
				},
			},
		);
		mockState.testParams.mockRejectedValueOnce(error);
		render(<AdminPoliciesPage />);

		openCreateWizard();

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Broken Local" },
		});
		fireEvent.change(screen.getByLabelText("base_path"), {
			target: { value: "/tmp/private" },
		});
		advanceCreateWizardToRulesStep();

		fireEvent.click(screen.getByRole("button", { name: /test_connection/i }));

		await waitFor(() => {
			expect(mockState.testParams).toHaveBeenCalledWith({
				access_key: undefined,
				base_path: "/tmp/private",
				bucket: undefined,
				driver_type: "local",
				endpoint: undefined,
				options: {},
				remote_node_id: undefined,
				secret_key: undefined,
			});
		});
		expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		expect(mockState.toastSuccess).not.toHaveBeenCalledWith(
			"connection_success",
		);
	});

	it("calls handleApiError when connection test fails without diagnostics", async () => {
		const error = new ApiError(
			ApiErrorCode.StorageMisconfigured,
			"Storage Driver Error",
		);
		mockState.testParams.mockRejectedValueOnce(error);
		render(<AdminPoliciesPage />);

		openCreateWizard();

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Broken Local" },
		});
		fireEvent.change(screen.getByLabelText("base_path"), {
			target: { value: "/tmp/private" },
		});
		advanceCreateWizardToRulesStep();

		fireEvent.click(screen.getByRole("button", { name: /test_connection/i }));

		await waitFor(() => {
			expect(mockState.testParams).toHaveBeenCalled();
		});
		expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		expect(mockState.toastSuccess).not.toHaveBeenCalledWith(
			"connection_success",
		);
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

	it("drops object storage transfer options when switching the create wizard back to local storage", async () => {
		render(<AdminPoliciesPage />);

		openCreateWizard("s3");

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Switched Local" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "https://s3.example.com" },
		});
		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "archive" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);
		fireEvent.click(
			screen.getAllByRole("button", { name: "select-item:presigned" })[0],
		);
		fireEvent.click(
			screen.getAllByRole("button", { name: "select-item:presigned" })[1],
		);

		fireEvent.click(screen.getByRole("button", { name: "core:back" }));
		fireEvent.click(screen.getByRole("button", { name: "core:back" }));
		fireEvent.click(screen.getByRole("button", { name: /^Local\b/ }));
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
				driver_type: "local",
				endpoint: "",
				is_default: false,
				max_file_size: undefined,
				name: "Switched Local",
				options: {},
				remote_node_id: undefined,
				secret_key: "",
			});
		});
	});

	it("uses storage driver descriptors to suppress unsupported OneDrive draft tests", async () => {
		render(<AdminPoliciesPage />);

		openCreateWizard("one_drive");

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Graph Draft" },
		});
		fireEvent.change(screen.getByLabelText("onedrive_client_id"), {
			target: { value: "client-id" },
		});
		fireEvent.change(screen.getByLabelText("onedrive_client_secret"), {
			target: { value: "client-secret" },
		});

		expect(
			screen.queryByRole("button", { name: /test_connection/ }),
		).not.toBeInTheDocument();
		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);
		expect(
			screen.queryByRole("button", { name: /test_connection/ }),
		).not.toBeInTheDocument();
		expect(mockState.testParams).not.toHaveBeenCalled();
	});

	it("uses descriptor connection fields instead of driver type to render object storage create fields", async () => {
		const descriptors = createStorageDriverDescriptors().map((descriptor) =>
			descriptor.driver_type === "s3"
				? {
						...descriptor,
						capabilities: {
							...descriptor.capabilities,
							object_storage_transfer_strategy: false,
						},
						fields: descriptor.fields.filter(
							(field) => field.name !== "bucket",
						),
					}
				: descriptor,
		);
		mockState.listStorageDriverDescriptors.mockResolvedValue(descriptors);
		primeAdminStorageDriverDescriptors(descriptors as never);

		render(<AdminPoliciesPage />);

		openCreateWizard("s3");

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Descriptor-gated S3" },
		});

		expect(screen.queryByLabelText("bucket")).not.toBeInTheDocument();
		expect(screen.queryByLabelText("endpoint")).not.toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);

		expect(
			screen.queryByText("policy_wizard_bucket_required"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByLabelText("object_storage_upload_strategy"),
		).not.toBeInTheDocument();
		expect(screen.queryByLabelText("content_dedup")).not.toBeInTheDocument();
	});

	it("renders object storage transfer controls and summary labels in the create wizard", async () => {
		render(<AdminPoliciesPage />);

		openCreateWizard("azure_blob");

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Azure Archive" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "https://acct.blob.core.windows.net" },
		});
		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "archive" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);

		expect(screen.getAllByText("object_storage_upload_strategy")).toHaveLength(
			2,
		);
		expect(
			screen.getAllByText("object_storage_download_strategy"),
		).toHaveLength(2);
		expect(
			screen.getAllByRole("button", { name: "select-item:presigned" }),
		).toHaveLength(2);
	});

	it("uses descriptor application credential fields instead of OneDrive driver type for create validation", async () => {
		const descriptors = createStorageDriverDescriptors().map((descriptor) =>
			descriptor.driver_type === "one_drive"
				? {
						...descriptor,
						fields: descriptor.fields.filter(
							(field) => field.scope !== "application_credential",
						),
					}
				: descriptor,
		);
		mockState.listStorageDriverDescriptors.mockResolvedValue(descriptors);
		primeAdminStorageDriverDescriptors(descriptors as never);

		render(<AdminPoliciesPage />);

		openCreateWizard("one_drive");

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Descriptor-gated OneDrive" },
		});

		expect(
			screen.queryByLabelText("onedrive_client_id"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByLabelText("onedrive_client_secret"),
		).not.toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);

		expect(
			screen.queryByText("onedrive_client_id_required"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByText("onedrive_client_secret_required"),
		).not.toBeInTheDocument();
		expect(screen.getByText("policy_wizard_summary_desc")).toBeInTheDocument();
	});

	it("uses descriptor application credential fields instead of OneDrive driver type in edit credential panel", async () => {
		const descriptors = createStorageDriverDescriptors().map((descriptor) =>
			descriptor.driver_type === "one_drive"
				? {
						...descriptor,
						fields: descriptor.fields.filter(
							(field) => field.scope !== "application_credential",
						),
					}
				: descriptor,
		);
		mockState.listStorageDriverDescriptors.mockResolvedValue(descriptors);
		primeAdminStorageDriverDescriptors(descriptors as never);
		mockState.items = [
			createPolicy({
				driver_type: "one_drive",
				id: 77,
				name: "Saved Descriptor OneDrive",
				options: {
					onedrive_account_mode: "work_or_school",
					onedrive_cloud: "global",
					onedrive_root_item_id: "root",
					onedrive_tenant: "common",
				},
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("Saved Descriptor OneDrive");

		expect(
			await screen.findByText("policy_editor_onedrive_title"),
		).toBeInTheDocument();
		expect(
			screen.queryByLabelText("onedrive_client_id"),
		).not.toBeInTheDocument();
		expect(
			screen.queryByLabelText("onedrive_client_secret"),
		).not.toBeInTheDocument();
		expect(screen.getByLabelText("onedrive_redirect_uri")).toBeInTheDocument();
	});

	it("uses descriptor actions instead of OneDrive driver type for credential panel commands", async () => {
		const descriptors = createStorageDriverDescriptors().map((descriptor) =>
			descriptor.driver_type === "one_drive"
				? {
						...descriptor,
						actions: descriptor.actions.filter(
							(action) =>
								action.kind !== "authorization" &&
								action.kind !== "credential_validation",
						),
					}
				: descriptor,
		);
		mockState.listStorageDriverDescriptors.mockResolvedValue(descriptors);
		primeAdminStorageDriverDescriptors(descriptors as never);
		mockState.items = [
			createPolicy({
				driver_type: "one_drive",
				id: 78,
				name: "Action-gated OneDrive",
				options: {
					onedrive_account_mode: "work_or_school",
					onedrive_cloud: "global",
					onedrive_root_item_id: "root",
					onedrive_tenant: "common",
				},
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("Action-gated OneDrive");

		expect(
			await screen.findByText("policy_editor_onedrive_title"),
		).toBeInTheDocument();
		expect(screen.getByLabelText("onedrive_client_id")).toBeInTheDocument();
		expect(screen.getByLabelText("onedrive_client_secret")).toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: /onedrive_authorize_action/ }),
		).not.toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: /onedrive_validate_action/ }),
		).not.toBeInTheDocument();
	});

	it("requires OneDrive application details at create time and starts authorization directly after save", async () => {
		const openedWindow = { opener: {} } as Window;
		const openSpy = vi.spyOn(window, "open").mockReturnValue(openedWindow);
		try {
			render(<AdminPoliciesPage />);

			openCreateWizard("one_drive");

			fireEvent.change(screen.getByLabelText("core:name"), {
				target: { value: "Team OneDrive" },
			});
			fireEvent.click(
				screen.getByRole("button", { name: "policy_wizard_review" }),
			);

			expect(
				screen.getByText("onedrive_client_id_required"),
			).toBeInTheDocument();
			expect(
				screen.getByText("onedrive_client_secret_required"),
			).toBeInTheDocument();
			expect(mockState.create).not.toHaveBeenCalled();

			fireEvent.change(screen.getByLabelText("onedrive_client_id"), {
				target: { value: "client-id" },
			});
			fireEvent.click(
				screen.getByRole("button", { name: "policy_wizard_review" }),
			);
			expect(
				screen.getByText("onedrive_client_secret_required"),
			).toBeInTheDocument();
			expect(mockState.create).not.toHaveBeenCalled();

			fireEvent.change(screen.getByLabelText("onedrive_client_secret"), {
				target: { value: "client-secret" },
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
					driver_type: "one_drive",
					endpoint: "",
					is_default: false,
					max_file_size: undefined,
					application_config: {
						microsoft_graph: {
							cloud: "global",
							tenant: "common",
							client_id: "client-id",
							client_secret: "client-secret",
							scopes: undefined,
						},
					},
					name: "Team OneDrive",
					options: {
						onedrive_account_mode: "work_or_school",
						onedrive_cloud: "global",
						onedrive_root_item_id: "root",
						onedrive_tenant: "common",
					},
					remote_node_id: undefined,
					secret_key: "",
				});
			});
			expect(mockState.toastSuccess).toHaveBeenCalledWith(
				"policy_onedrive_created_authorize_next",
			);

			fireEvent.click(
				screen.getByRole("button", { name: /onedrive_authorize_action/ }),
			);

			await waitFor(() => {
				expect(mockState.startStorageAuthorization).toHaveBeenCalledWith(99, {
					provider: "microsoft_graph",
				});
			});
			expect(openSpy).toHaveBeenCalledWith(
				"https://login.example.test/authorize",
				"_blank",
			);
		} finally {
			openSpy.mockRestore();
		}
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
		const submitEvent = new Event("submit", {
			bubbles: true,
			cancelable: true,
		});
		form.dispatchEvent(submitEvent);
		expect(submitEvent.defaultPrevented).toBe(true);
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

		const localOption = screen.getByRole("button", {
			name: /^Local\b/,
		});
		fireEvent.click(localOption);

		const reviewButton = screen.getByRole("button", {
			name: "policy_wizard_review",
		});
		expect(reviewButton.parentElement).toHaveClass(
			"ml-auto",
			"flex-nowrap",
			"justify-end",
		);

		const forwardAnimatedPanel = screen.getByTestId("policy-step-panel");
		expect(forwardAnimatedPanel).toHaveClass(
			"animate-in",
			"fade-in",
			"slide-in-from-right-6",
		);
		fireEvent.click(
			screen.getByRole("button", { name: "2policy_wizard_step_local_title" }),
		);

		fireEvent.click(screen.getByRole("button", { name: /core:back/i }));

		const backwardAnimatedPanel = screen.getByTestId("policy-step-panel");
		expect(backwardAnimatedPanel).toHaveClass(
			"animate-in",
			"fade-in",
			"slide-in-from-left-6",
		);
	});

	it("uses the Tencent COS storage image in the create driver picker", () => {
		render(<AdminPoliciesPage />);

		fireEvent.click(screen.getByRole("button", { name: /new_policy/i }));

		const cosOption = screen.getByRole("button", { name: /Tencent COS/ });
		const cosImage = within(cosOption).getByRole("presentation");

		expect(cosImage).toHaveAttribute(
			"src",
			"/static/storage/tencent-cloud-cos.webp",
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
				options: {
					object_storage_download_strategy: "relay_stream",
					object_storage_upload_strategy: "relay_stream",
				},
				remote_node_id: undefined,
				secret_key: undefined,
			});
		});
	});

	it("creates a Tencent COS policy with storage-native thumbnail and media info rules", async () => {
		render(<AdminPoliciesPage />);

		openCreateWizard("tencent_cos");

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "COS Media" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "https://cos.ap-guangzhou.myqcloud.com" },
		});
		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "media-1250000000" },
		});
		fireEvent.change(screen.getByLabelText("Access Key"), {
			target: { value: "AKID" },
		});
		fireEvent.change(screen.getByLabelText("Secret Key"), {
			target: { value: "SECRET" },
		});

		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);

		expect(
			screen.getByText("policy_storage_native_section_title"),
		).toBeInTheDocument();
		fireEvent.click(
			screen.getByRole("button", {
				name: "switch:storage_native_processing_enabled:false",
			}),
		);
		expect(
			screen.getByLabelText("storage_native_thumbnail_extensions"),
		).toHaveDisplayValue("jpg, jpeg, png, webp, gif");

		fireEvent.change(
			screen.getByLabelText("storage_native_thumbnail_extensions"),
			{
				target: { value: " .PNG ,jpg, .png , webp" },
			},
		);
		fireEvent.click(
			screen.getByRole("button", {
				name: "switch:storage_native_media_metadata_enabled:false",
			}),
		);
		fireEvent.change(
			screen.getByLabelText("storage_native_media_metadata_extensions"),
			{
				target: { value: " .MP4 ,mov, mp4" },
			},
		);

		fireEvent.click(screen.getByRole("button", { name: /core:create/i }));

		await waitFor(() => {
			expect(mockState.testParams).toHaveBeenCalledWith({
				access_key: "AKID",
				base_path: undefined,
				bucket: "media-1250000000",
				driver_type: "tencent_cos",
				endpoint: "https://cos.ap-guangzhou.myqcloud.com",
				options: {
					media_metadata_extensions: ["mp4", "mov"],
					object_storage_download_strategy: "relay_stream",
					object_storage_upload_strategy: "relay_stream",
					storage_native_media_metadata_enabled: true,
					storage_native_processing_enabled: true,
					thumbnail_extensions: ["png", "jpg", "webp"],
					thumbnail_processor: "storage_native",
				},
				remote_node_id: undefined,
				secret_key: "SECRET",
			});
		});
		await waitFor(() => {
			expect(mockState.create).toHaveBeenCalledWith({
				access_key: "AKID",
				base_path: "",
				bucket: "media-1250000000",
				chunk_size: 5 * 1024 * 1024,
				driver_type: "tencent_cos",
				endpoint: "https://cos.ap-guangzhou.myqcloud.com",
				is_default: false,
				max_file_size: undefined,
				name: "COS Media",
				options: {
					media_metadata_extensions: ["mp4", "mov"],
					object_storage_download_strategy: "relay_stream",
					object_storage_upload_strategy: "relay_stream",
					storage_native_media_metadata_enabled: true,
					storage_native_processing_enabled: true,
					thumbnail_extensions: ["png", "jpg", "webp"],
					thumbnail_processor: "storage_native",
				},
				remote_node_id: undefined,
				secret_key: "SECRET",
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("policy_created");
	});

	it("configures Tencent COS CORS from create draft fields", async () => {
		render(<AdminPoliciesPage />);

		openCreateWizard("tencent_cos");

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "COS Draft" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "https://cos.ap-guangzhou.myqcloud.com" },
		});
		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "media-1250000000" },
		});
		fireEvent.change(screen.getByLabelText("Access Key"), {
			target: { value: "AKIDEXAMPLE" },
		});
		fireEvent.change(screen.getByLabelText("Secret Key"), {
			target: { value: "SECRETEXAMPLE" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "policy_cos_cors_action_short" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "policy_cos_cors_confirm" }),
		);

		await waitFor(() => {
			expect(mockState.executeDraftPolicyAction).toHaveBeenCalledWith({
				access_key: "AKIDEXAMPLE",
				action: "configure_tencent_cos_cors",
				base_path: undefined,
				bucket: "media-1250000000",
				driver_type: "tencent_cos",
				endpoint: "https://cos.ap-guangzhou.myqcloud.com",
				options: {
					object_storage_download_strategy: "relay_stream",
					object_storage_upload_strategy: "relay_stream",
				},
				remote_node_id: undefined,
				secret_key: "SECRETEXAMPLE",
			});
		});
		expect(mockState.executeSavedPolicyAction).not.toHaveBeenCalled();
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"policy_cos_cors_success",
			{
				description: "policy_cos_cors_success_request_id",
			},
		);
	});

	it("hides Tencent COS CORS controls when descriptor omits the action", async () => {
		const descriptorsWithoutCosAction = createStorageDriverDescriptors().map(
			(descriptor) =>
				descriptor.driver_type === "tencent_cos"
					? {
							...descriptor,
							actions: descriptor.actions.filter(
								(action) => action.kind !== "policy_action",
							),
						}
					: descriptor,
		);
		mockState.listStorageDriverDescriptors.mockResolvedValue(
			descriptorsWithoutCosAction,
		);
		primeAdminStorageDriverDescriptors(descriptorsWithoutCosAction as never);

		render(<AdminPoliciesPage />);

		openCreateWizard("tencent_cos");
		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "COS Draft" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "https://cos.ap-guangzhou.myqcloud.com" },
		});
		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "media-1250000000" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);

		expect(
			screen.queryByRole("button", { name: "policy_cos_cors_action_short" }),
		).not.toBeInTheDocument();
		expect(mockState.executeDraftPolicyAction).not.toHaveBeenCalled();
	});

	it("requires visible confirmation before configuring Tencent COS CORS from create mode", () => {
		render(<AdminPoliciesPage />);

		openCreateWizard("tencent_cos");

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "COS Draft" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "https://cos.ap-guangzhou.myqcloud.com" },
		});
		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "media-1250000000" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);

		const actionButton = screen.getByRole("button", {
			name: "policy_cos_cors_action_short",
		});
		fireEvent.click(actionButton);

		expect(screen.getByText("policy_cos_cors_confirm_title")).toBeVisible();
		expect(
			screen.getByRole("button", { name: "policy_cos_cors_confirm" }),
		).toBeVisible();
		expect(actionButton).toBeDisabled();

		fireEvent.click(screen.getByRole("button", { name: "core:cancel" }));

		expect(
			screen.queryByText("policy_cos_cors_confirm_title"),
		).not.toBeInTheDocument();
		expect(mockState.executeDraftPolicyAction).not.toHaveBeenCalled();
		expect(mockState.executeSavedPolicyAction).not.toHaveBeenCalled();
	});

	it("does not expose Tencent COS CORS action in non-Tencent-COS create mode", () => {
		render(<AdminPoliciesPage />);

		openCreateWizard("s3");

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "S3 Draft" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "https://s3.example.com" },
		});
		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "archive" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);

		expect(
			screen.queryByRole("button", { name: "policy_cos_cors_action_short" }),
		).not.toBeInTheDocument();
		expect(
			screen.queryByText("policy_cos_cors_confirm_title"),
		).not.toBeInTheDocument();
	});

	it("creates an Azure Blob policy with account credentials and object-storage rules", async () => {
		render(<AdminPoliciesPage />);

		openCreateWizard("azure_blob");

		expect(
			screen.getByText("policy_wizard_azure_blob_helper"),
		).toBeInTheDocument();
		expect(screen.getByText("azure_blob_endpoint_hint")).toBeInTheDocument();
		expect(screen.getByLabelText("Account Name")).not.toHaveAttribute(
			"placeholder",
		);
		expect(screen.getByLabelText("Account Key")).not.toHaveAttribute(
			"placeholder",
		);

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Azure Archive" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "https://acct.blob.core.windows.net" },
		});
		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "archive" },
		});
		fireEvent.change(screen.getByLabelText("Account Name"), {
			target: { value: "acct" },
		});
		fireEvent.change(screen.getByLabelText("Account Key"), {
			target: { value: "AZURESECRET" },
		});

		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);

		expect(
			screen.queryByText("policy_storage_native_section_title"),
		).not.toBeInTheDocument();
		expect(screen.getByText("Azure Blob")).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: /core:create/i }));

		await waitFor(() => {
			expect(mockState.testParams).toHaveBeenCalledWith({
				access_key: "acct",
				base_path: undefined,
				bucket: "archive",
				driver_type: "azure_blob",
				endpoint: "https://acct.blob.core.windows.net",
				options: {
					object_storage_download_strategy: "relay_stream",
					object_storage_upload_strategy: "relay_stream",
				},
				remote_node_id: undefined,
				secret_key: "AZURESECRET",
			});
		});
		await waitFor(() => {
			expect(mockState.create).toHaveBeenCalledWith({
				access_key: "acct",
				base_path: "",
				bucket: "archive",
				chunk_size: 5 * 1024 * 1024,
				driver_type: "azure_blob",
				endpoint: "https://acct.blob.core.windows.net",
				is_default: false,
				max_file_size: undefined,
				name: "Azure Archive",
				options: {
					object_storage_download_strategy: "relay_stream",
					object_storage_upload_strategy: "relay_stream",
				},
				remote_node_id: undefined,
				secret_key: "AZURESECRET",
			});
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith("policy_created");
	});

	it("suggests the specialized driver when a generic S3 create uses a known provider endpoint", () => {
		render(<AdminPoliciesPage />);

		openCreateWizard("s3");

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "COS via S3" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: {
				value: "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
			},
		});
		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "media-1250000000" },
		});

		expect(
			screen.getByText("policy_s3_driver_suggestion_title:Tencent COS"),
		).toBeInTheDocument();
		expect(
			screen.getByText("policy_s3_driver_suggestion_desc:Tencent COS"),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", {
				name: /policy_s3_driver_suggestion_action:Tencent COS/,
			}),
		);

		expect(screen.getByText("cos_endpoint_hint")).toBeInTheDocument();
		expect(screen.getByDisplayValue("COS via S3")).toBeInTheDocument();
		expect(
			screen.getByDisplayValue(
				"https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
			),
		).toBeInTheDocument();
		expect(screen.getByDisplayValue("media-1250000000")).toBeInTheDocument();
		expect(
			screen.queryByText("policy_s3_driver_suggestion_title:Tencent COS"),
		).not.toBeInTheDocument();
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

		openCreateWizard("remote");

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
				options: {
					remote_download_strategy: "relay_stream",
					remote_upload_strategy: "relay_stream",
				},
				remote_node_id: 7,
				remote_storage_target_key: "rst_default",
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
				remote_storage_target_key: "rst_default",
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

		openCreateWizard("remote");

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
				remote_storage_target_key: "rst_default",
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

		openCreateWizard("remote");

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
				remote_storage_target_key: "rst_default",
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
		expect(editShell).toHaveClass("space-y-4");
		expect(
			screen.getByText("policy_editor_overview_title"),
		).toBeInTheDocument();
		expect(
			screen.queryByText("policy_editor_storage_title"),
		).not.toBeInTheDocument();
		expect(screen.getByText("policy_editor_rules_title")).toBeInTheDocument();
		expect(screen.queryByTestId("policy-summary-card")).not.toBeInTheDocument();
		expect(screen.getByTestId("policy-edit-context-bar")).toBeInTheDocument();
		expect(screen.getByText("policy_edit_context_title")).toBeInTheDocument();
		expect(
			screen.getByText("policy_edit_default_disabled"),
		).toBeInTheDocument();
		expect(
			screen.getByText("policy_edit_context_local_desc"),
		).toBeInTheDocument();
		expect(screen.getByText("policy_capacity_title")).toBeInTheDocument();
		expect(screen.getByTestId("policy-edit-capacity-summary")).toHaveClass(
			"md:border-l",
			"md:pl-4",
		);
		expect(
			screen.queryByText("policy_wizard_driver_panel_title"),
		).not.toBeInTheDocument();
		expect(screen.getByTestId("policy-edit-driver-badge")).toHaveAttribute(
			"data-variant",
			"outline",
		);
		expect(screen.getByTestId("policy-edit-driver-badge")).toHaveClass(
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

	it("reuses saved OneDrive application settings when authorization fields are left empty", async () => {
		const openedWindow = { opener: {} } as Window;
		const openSpy = vi.spyOn(window, "open").mockReturnValue(openedWindow);
		mockState.items = [
			createPolicy({
				id: 12,
				name: "Saved OneDrive",
				driver_type: "one_drive",
				options: {
					onedrive_account_mode: "work_or_school",
					onedrive_cloud: "global",
					onedrive_root_item_id: "root",
					onedrive_tenant: "common",
				},
			}),
		];
		mockState.listStorageCredentials.mockResolvedValue([
			{
				account_label: "root",
				authorized_at: "2026-06-15T10:20:00Z",
				created_at: "2026-06-15T10:20:00Z",
				credential_kind: "oauth_delegated",
				expires_at: null,
				id: 7,
				last_refreshed_at: "2026-06-15T10:40:00Z",
				last_validated_at: "2026-06-15T10:20:00Z",
				policy_id: 12,
				provider: "microsoft_graph",
				scopes: ["offline_access", "Files.ReadWrite.All"],
				status: "authorized",
				status_reason: null,
				subject: "root-id",
				tenant_id: "common",
				updated_at: "2026-06-15T10:20:00Z",
			},
		]);
		try {
			render(<AdminPoliciesPage />);

			openEditPolicy("Saved OneDrive");
			await screen.findByText("onedrive_credential_status_authorized");

			expect(screen.getByLabelText("onedrive_client_id")).toHaveAttribute(
				"placeholder",
				"onedrive_client_id_keep_placeholder",
			);
			expect(screen.getByLabelText("onedrive_client_secret")).toHaveAttribute(
				"placeholder",
				"onedrive_client_secret_keep_placeholder",
			);
			expect(
				screen.getByText(/onedrive_credential_refreshed_at/),
			).toBeInTheDocument();

			fireEvent.click(
				screen.getByRole("button", { name: /onedrive_reauthorize_action/ }),
			);

			await waitFor(() => {
				expect(mockState.startStorageAuthorization).toHaveBeenCalledWith(12, {
					provider: "microsoft_graph",
				});
			});
			expect(openSpy).toHaveBeenCalledWith(
				"https://login.example.test/authorize",
				"_blank",
			);
		} finally {
			openSpy.mockRestore();
		}
	});

	it("starts OneDrive authorization without draft Microsoft Graph overrides", async () => {
		const openedWindow = { opener: {} } as Window;
		const openSpy = vi.spyOn(window, "open").mockReturnValue(openedWindow);
		mockState.items = [
			createPolicy({
				id: 12,
				name: "Override Guard OneDrive",
				driver_type: "one_drive",
				options: {
					onedrive_account_mode: "work_or_school",
					onedrive_cloud: "global",
					onedrive_root_item_id: "root",
					onedrive_tenant: "common",
				},
			}),
		];
		mockState.listStorageCredentials.mockResolvedValue([]);
		try {
			render(<AdminPoliciesPage />);

			openEditPolicy("Override Guard OneDrive");
			await screen.findByText("onedrive_credential_status_missing");

			fireEvent.change(screen.getByLabelText("onedrive_client_id"), {
				target: { value: "draft-client-id" },
			});
			fireEvent.change(screen.getByLabelText("onedrive_client_secret"), {
				target: { value: "draft-client-secret" },
			});
			fireEvent.click(screen.getByRole("button", { name: /save_changes/i }));

			await waitFor(() => {
				expect(mockState.update).toHaveBeenCalledWith(
					12,
					expect.objectContaining({
						application_config: {
							microsoft_graph: expect.objectContaining({
								client_id: "draft-client-id",
								client_secret: "draft-client-secret",
							}),
						},
					}),
				);
			});

			fireEvent.click(
				screen.getByRole("button", { name: /onedrive_authorize_action/ }),
			);

			await waitFor(() => {
				expect(mockState.startStorageAuthorization).toHaveBeenCalledWith(12, {
					provider: "microsoft_graph",
				});
			});
			expect(
				mockState.startStorageAuthorization.mock.calls[0]?.[1],
			).not.toHaveProperty("microsoft_graph");
			expect(openSpy).toHaveBeenCalledWith(
				"https://login.example.test/authorize",
				"_blank",
			);
		} finally {
			openSpy.mockRestore();
		}
	});

	it("requires saving changed OneDrive authorization context before starting authorization", async () => {
		mockState.items = [
			createPolicy({
				id: 12,
				name: "Changed OneDrive",
				driver_type: "one_drive",
				options: {
					onedrive_account_mode: "work_or_school",
					onedrive_cloud: "global",
					onedrive_root_item_id: "root",
					onedrive_tenant: "common",
				},
			}),
		];
		mockState.listStorageCredentials.mockResolvedValue([]);

		render(<AdminPoliciesPage />);

		openEditPolicy("Changed OneDrive");
		await screen.findByText("onedrive_credential_status_missing");
		fireEvent.click(
			screen.getByRole("button", { name: /onedrive_advanced_target/ }),
		);
		fireEvent.change(screen.getByLabelText("onedrive_drive_id"), {
			target: { value: "drive-2" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: /onedrive_authorize_action/ }),
		);

		expect(mockState.toastError).toHaveBeenCalledWith(
			"onedrive_save_before_authorize",
		);
		expect(mockState.startStorageAuthorization).not.toHaveBeenCalled();
	});

	it("requires saving changed OneDrive settings before validating credentials", async () => {
		mockState.items = [
			createPolicy({
				id: 12,
				name: "Saved OneDrive",
				driver_type: "one_drive",
				options: {
					onedrive_account_mode: "work_or_school",
					onedrive_cloud: "global",
					onedrive_root_item_id: "root",
					onedrive_tenant: "common",
				},
			}),
		];
		mockState.listStorageCredentials.mockResolvedValue([
			{
				account_label: "root",
				authorized_at: "2026-06-15T10:20:00Z",
				created_at: "2026-06-15T10:20:00Z",
				credential_kind: "oauth_delegated",
				expires_at: null,
				id: 7,
				last_refreshed_at: "2026-06-15T10:40:00Z",
				last_validated_at: "2026-06-15T10:20:00Z",
				policy_id: 12,
				provider: "microsoft_graph",
				scopes: ["offline_access", "Files.ReadWrite.All"],
				status: "authorized",
				status_reason: null,
				subject: "root-id",
				tenant_id: "common",
				updated_at: "2026-06-15T10:20:00Z",
			},
		]);

		render(<AdminPoliciesPage />);

		openEditPolicy("Saved OneDrive");
		await screen.findByText("onedrive_credential_status_authorized");
		fireEvent.change(screen.getByLabelText("onedrive_client_id"), {
			target: { value: "new-client-id" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: /onedrive_validate_action/ }),
		);

		expect(mockState.toastError).toHaveBeenCalledWith(
			"onedrive_save_before_validate",
		);
		expect(mockState.validateStorageCredential).not.toHaveBeenCalled();
	});

	it("validates saved OneDrive credentials against the current policy", async () => {
		mockState.items = [
			createPolicy({
				id: 12,
				name: "Saved OneDrive",
				driver_type: "one_drive",
				options: {
					onedrive_account_mode: "work_or_school",
					onedrive_cloud: "global",
					onedrive_root_item_id: "root",
					onedrive_tenant: "common",
				},
			}),
		];
		mockState.listStorageCredentials.mockResolvedValue([
			{
				account_label: "root",
				authorized_at: "2026-06-15T10:20:00Z",
				created_at: "2026-06-15T10:20:00Z",
				credential_kind: "oauth_delegated",
				expires_at: null,
				id: 7,
				last_refreshed_at: "2026-06-15T10:40:00Z",
				last_validated_at: null,
				policy_id: 12,
				provider: "microsoft_graph",
				scopes: ["offline_access", "Files.ReadWrite.All"],
				status: "authorized",
				status_reason: null,
				subject: "root-id",
				tenant_id: "common",
				updated_at: "2026-06-15T10:20:00Z",
			},
		]);
		mockState.validateStorageCredential.mockResolvedValue({
			credential: {
				account_label: "root",
				authorized_at: "2026-06-15T10:20:00Z",
				created_at: "2026-06-15T10:20:00Z",
				credential_kind: "oauth_delegated",
				expires_at: null,
				id: 7,
				last_refreshed_at: "2026-06-15T10:40:00Z",
				last_validated_at: "2026-06-16T10:20:00Z",
				policy_id: 12,
				provider: "microsoft_graph",
				scopes: ["offline_access", "Files.ReadWrite.All"],
				status: "authorized",
				status_reason: null,
				subject: "root-id",
				tenant_id: "common",
				updated_at: "2026-06-16T10:20:00Z",
			},
			root_item_name: "Documents",
		});

		render(<AdminPoliciesPage />);

		openEditPolicy("Saved OneDrive");
		await screen.findByText("onedrive_credential_status_authorized");
		fireEvent.click(
			screen.getByRole("button", { name: /onedrive_validate_action/ }),
		);

		await waitFor(() => {
			expect(mockState.validateStorageCredential).toHaveBeenCalledWith(
				12,
				"microsoft_graph",
			);
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"onedrive_validation_success",
			{
				description: "onedrive_validation_success_root",
			},
		);
	});

	it("shows normalized OneDrive reauthorization reasons without exposing raw provider errors", async () => {
		mockState.items = [
			createPolicy({
				id: 12,
				name: "Saved OneDrive",
				driver_type: "one_drive",
				options: {
					onedrive_account_mode: "work_or_school",
					onedrive_cloud: "global",
					onedrive_root_item_id: "root",
					onedrive_tenant: "common",
				},
			}),
		];
		mockState.listStorageCredentials.mockResolvedValue([
			{
				account_label: "root",
				authorized_at: "2026-06-15T10:20:00Z",
				created_at: "2026-06-15T10:20:00Z",
				credential_kind: "oauth_delegated",
				expires_at: null,
				id: 7,
				last_refreshed_at: null,
				last_validated_at: null,
				policy_id: 12,
				provider: "microsoft_graph",
				scopes: ["offline_access", "Files.ReadWrite.All"],
				status: "reauth_required",
				status_reason: "invalid_grant: raw provider diagnostic",
				subject: "root-id",
				tenant_id: "common",
				updated_at: "2026-06-15T10:20:00Z",
			},
		]);

		render(<AdminPoliciesPage />);

		openEditPolicy("Saved OneDrive");

		expect(
			await screen.findByText("onedrive_credential_reauth_required_title"),
		).toBeInTheDocument();
		expect(
			screen.getByText("onedrive_credential_reason_invalid_grant"),
		).toBeInTheDocument();
		expect(
			screen.getByText("onedrive_credential_reauth_required_desc"),
		).toBeInTheDocument();
		expect(
			screen.queryByText("invalid_grant: raw provider diagnostic"),
		).not.toBeInTheDocument();
	});

	it("saves updated OneDrive application settings as generic application config", async () => {
		mockState.items = [
			createPolicy({
				id: 12,
				name: "Saved OneDrive",
				driver_type: "one_drive",
				options: {
					onedrive_account_mode: "work_or_school",
					onedrive_cloud: "global",
					onedrive_root_item_id: "root",
					onedrive_tenant: "common",
				},
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("Saved OneDrive");
		await screen.findByLabelText("onedrive_client_id");

		fireEvent.change(screen.getByLabelText("onedrive_client_id"), {
			target: { value: "new-client-id" },
		});
		fireEvent.change(screen.getByLabelText("onedrive_client_secret"), {
			target: { value: "new-client-secret" },
		});
		fireEvent.click(screen.getByRole("button", { name: /save_changes/i }));

		await waitFor(() => {
			expect(mockState.update).toHaveBeenCalledWith(
				12,
				expect.objectContaining({
					application_config: {
						microsoft_graph: {
							cloud: "global",
							tenant: "common",
							client_id: "new-client-id",
							client_secret: "new-client-secret",
							scopes: undefined,
						},
					},
				}),
			);
		});
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
				options: { object_storage_upload_strategy: "presigned" },
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("Archive S3");

		expect(screen.getByText("s3_endpoint_hint")).toBeInTheDocument();
		expect(screen.getByTestId("policy-edit-driver-badge")).toHaveAttribute(
			"data-variant",
			"outline",
		);
		expect(screen.getByTestId("policy-edit-driver-badge")).toHaveClass(
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
				options: {
					object_storage_download_strategy: "relay_stream",
					object_storage_upload_strategy: "relay_stream",
				},
				policy_id: 7,
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
					object_storage_download_strategy: "relay_stream",
					object_storage_upload_strategy: "relay_stream",
				},
			}),
		);
		expect(payload).toHaveProperty("access_key", "NEWKEY");
		expect(payload).toHaveProperty("secret_key", "NEWSECRET");
		expect(mockState.toastSuccess).toHaveBeenCalledWith("policy_updated");
	});

	it("tests changed s3 params with saved credentials when credential fields stay blank", async () => {
		mockState.items = [
			createPolicy({
				id: 8,
				name: "Saved Credential S3",
				driver_type: "s3",
				endpoint: "https://s3.example.com",
				bucket: "archive",
				base_path: "tenant-a",
				max_file_size: 4096,
				options: { object_storage_upload_strategy: "presigned" },
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("Saved Credential S3");

		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "https://s3-alt.example.com" },
		});
		fireEvent.click(screen.getByRole("button", { name: /test_connection/i }));

		await waitFor(() => {
			expect(mockState.testParams).toHaveBeenCalledWith({
				access_key: undefined,
				base_path: "tenant-a",
				bucket: "archive",
				driver_type: "s3",
				endpoint: "https://s3-alt.example.com",
				options: {
					object_storage_download_strategy: "relay_stream",
					object_storage_upload_strategy: "presigned",
				},
				policy_id: 8,
				remote_node_id: undefined,
				secret_key: undefined,
			});
		});
		expect(mockState.testConnection).not.toHaveBeenCalled();
	});

	it("configures Tencent COS CORS from a saved policy when connection fields are unchanged", async () => {
		mockState.items = [
			createPolicy({
				id: 31,
				name: "COS Saved",
				driver_type: "tencent_cos",
				endpoint: "https://cos.ap-guangzhou.myqcloud.com",
				bucket: "media-1250000000",
				base_path: "tenant-cos",
				options: {
					object_storage_download_strategy: "presigned",
					object_storage_upload_strategy: "presigned",
				},
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("COS Saved");

		expect(screen.getByText("policy_cos_cors_title")).toBeInTheDocument();
		expect(screen.getByText("policy_cos_cors_uses_saved")).toBeInTheDocument();
		fireEvent.click(
			screen.getByRole("button", { name: "policy_cos_cors_action" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "policy_cos_cors_confirm" }),
		);

		await waitFor(() => {
			expect(mockState.executeSavedPolicyAction).toHaveBeenCalledWith(31, {
				action: "configure_tencent_cos_cors",
			});
		});
		expect(mockState.executeDraftPolicyAction).not.toHaveBeenCalled();
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"policy_cos_cors_success",
			{
				description: "policy_cos_cors_success_request_id",
			},
		);
	});

	it("keeps the Tencent COS CORS confirmation open when saved action fails", async () => {
		const error = new Error("cors failed");
		mockState.executeSavedPolicyAction.mockRejectedValueOnce(error);
		mockState.items = [
			createPolicy({
				id: 35,
				name: "COS Saved Failure",
				driver_type: "tencent_cos",
				endpoint: "https://cos.ap-guangzhou.myqcloud.com",
				bucket: "media-1250000000",
				base_path: "tenant-cos",
				options: {
					object_storage_download_strategy: "presigned",
					object_storage_upload_strategy: "presigned",
				},
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("COS Saved Failure");
		fireEvent.click(
			screen.getByRole("button", { name: "policy_cos_cors_action" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "policy_cos_cors_confirm" }),
		);

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(
			screen.getByText("policy_cos_cors_confirm_title"),
		).toBeInTheDocument();
		expect(mockState.toastSuccess).not.toHaveBeenCalledWith(
			"policy_cos_cors_success",
			expect.anything(),
		);
	});

	it("does not expose Tencent COS CORS action for saved non-Tencent-COS policies", () => {
		mockState.items = [
			createPolicy({
				id: 33,
				name: "Local Saved",
				driver_type: "local",
				base_path: "/tmp/local-saved",
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("Local Saved");

		expect(screen.queryByText("policy_cos_cors_title")).not.toBeInTheDocument();
		expect(
			screen.queryByRole("button", { name: "policy_cos_cors_action" }),
		).not.toBeInTheDocument();
		expect(mockState.executeDraftPolicyAction).not.toHaveBeenCalled();
		expect(mockState.executeSavedPolicyAction).not.toHaveBeenCalled();
	});

	it("configures Tencent COS CORS from draft fields when saved connection fields changed", async () => {
		mockState.items = [
			createPolicy({
				id: 32,
				name: "COS Dirty",
				driver_type: "tencent_cos",
				endpoint: "https://cos.ap-guangzhou.myqcloud.com",
				bucket: "media-1250000000",
				base_path: "tenant-cos",
				options: {
					object_storage_download_strategy: "presigned",
					object_storage_upload_strategy: "presigned",
				},
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("COS Dirty");

		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "media-draft-1250000000" },
		});
		fireEvent.change(screen.getByLabelText("Access Key"), {
			target: { value: "AKIDDIRTY" },
		});
		fireEvent.change(screen.getByLabelText("Secret Key"), {
			target: { value: "SECRETDIRTY" },
		});

		expect(screen.getByText("policy_cos_cors_uses_draft")).toBeInTheDocument();
		fireEvent.click(
			screen.getByRole("button", { name: "policy_cos_cors_action" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "policy_cos_cors_confirm" }),
		);

		await waitFor(() => {
			expect(mockState.executeDraftPolicyAction).toHaveBeenCalledWith({
				access_key: "AKIDDIRTY",
				action: "configure_tencent_cos_cors",
				base_path: "tenant-cos",
				bucket: "media-draft-1250000000",
				driver_type: "tencent_cos",
				endpoint: "https://cos.ap-guangzhou.myqcloud.com",
				options: {
					object_storage_download_strategy: "presigned",
					object_storage_upload_strategy: "presigned",
				},
				policy_id: 32,
				remote_node_id: undefined,
				secret_key: "SECRETDIRTY",
			});
		});
		expect(mockState.executeSavedPolicyAction).not.toHaveBeenCalled();
	});

	it("lets draft Tencent COS CORS actions reuse saved credentials server-side", async () => {
		mockState.items = [
			createPolicy({
				id: 34,
				name: "COS Dirty Credentials Saved",
				driver_type: "tencent_cos",
				endpoint: "https://cos.ap-guangzhou.myqcloud.com",
				bucket: "media-1250000000",
				base_path: "tenant-cos",
				options: {
					object_storage_download_strategy: "presigned",
					object_storage_upload_strategy: "presigned",
				},
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("COS Dirty Credentials Saved");

		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "media-draft-1250000000" },
		});

		fireEvent.click(
			screen.getByRole("button", { name: "policy_cos_cors_action" }),
		);
		fireEvent.click(
			screen.getByRole("button", { name: "policy_cos_cors_confirm" }),
		);

		await waitFor(() => {
			expect(mockState.executeDraftPolicyAction).toHaveBeenCalledWith({
				action: "configure_tencent_cos_cors",
				base_path: "tenant-cos",
				bucket: "media-draft-1250000000",
				driver_type: "tencent_cos",
				endpoint: "https://cos.ap-guangzhou.myqcloud.com",
				options: {
					object_storage_download_strategy: "presigned",
					object_storage_upload_strategy: "presigned",
				},
				policy_id: 34,
				remote_node_id: undefined,
			});
		});
		expect(mockState.executeSavedPolicyAction).not.toHaveBeenCalled();
	});

	it("promotes a saved generic S3 policy to a specialized S3-compatible driver", async () => {
		mockState.items = [
			createPolicy({
				id: 21,
				name: "Legacy COS S3",
				driver_type: "s3",
				endpoint: "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
				bucket: "media-1250000000",
				base_path: "tenant-cos",
				options: {
					object_storage_download_strategy: "presigned",
					object_storage_upload_strategy: "presigned",
					s3_path_style: false,
				},
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("Legacy COS S3");

		expect(
			screen.getByText("policy_s3_driver_promotion_title"),
		).toBeInTheDocument();
		expect(
			screen.getByText("policy_s3_driver_promotion_desc:Tencent COS"),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", {
				name: /policy_s3_driver_promotion_action:Tencent COS/,
			}),
		);
		expect(
			screen.getByText("policy_s3_driver_promotion_confirm_desc:Tencent COS"),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", {
				name: "policy_s3_driver_promotion_confirm",
			}),
		);

		await waitFor(() => {
			expect(mockState.promoteS3CompatibleDriver).toHaveBeenCalledWith(21, {
				target_driver_type: "tencent_cos",
				endpoint: "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
				bucket: "media-1250000000",
			});
		});
		expect(mockState.update).not.toHaveBeenCalled();
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"policy_s3_driver_promotion_success:Tencent COS",
		);
		expect(screen.getByTestId("policy-edit-driver-badge")).toHaveTextContent(
			"Tencent COS",
		);
		expect(
			screen.getByText("policy_storage_native_section_title"),
		).toBeInTheDocument();
		expect(
			screen.queryByText("policy_s3_driver_promotion_title"),
		).not.toBeInTheDocument();
	});

	it("shows the specialized driver promotion hint while editing a matching S3 draft", async () => {
		mockState.items = [
			createPolicy({
				id: 22,
				name: "Draft COS S3",
				driver_type: "s3",
				endpoint: "https://s3.example.com",
				bucket: "draft-bucket",
				base_path: "tenant-draft",
				options: {
					object_storage_download_strategy: "relay_stream",
					object_storage_upload_strategy: "relay_stream",
				},
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("Draft COS S3");

		expect(
			screen.queryByText("policy_s3_driver_promotion_title"),
		).not.toBeInTheDocument();

		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: {
				value: "https://draft-bucket.cos.ap-guangzhou.myqcloud.com",
			},
		});

		expect(
			screen.getByText("policy_s3_driver_promotion_title"),
		).toBeInTheDocument();
		expect(
			screen.getByText("policy_s3_driver_promotion_desc:Tencent COS"),
		).toBeInTheDocument();
		expect(
			screen.getByText("policy_s3_driver_promotion_unsaved_blocked"),
		).toBeInTheDocument();
		expect(
			screen.getByRole("button", {
				name: /policy_s3_driver_promotion_action:Tencent COS/,
			}),
		).toBeDisabled();

		fireEvent.click(screen.getByRole("button", { name: /save_changes/i }));

		await waitFor(() => {
			expect(mockState.update).toHaveBeenCalledWith(
				22,
				expect.objectContaining({
					endpoint: "https://draft-bucket.cos.ap-guangzhou.myqcloud.com",
				}),
			);
		});
		expect(screen.getByDisplayValue("Draft COS S3")).toBeInTheDocument();
		expect(
			screen.getByRole("button", {
				name: /policy_s3_driver_promotion_action:Tencent COS/,
			}),
		).toBeEnabled();
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

	it("trims S3-compatible endpoint and bucket inputs on blur without provider-specific rewriting", () => {
		render(<AdminPoliciesPage />);

		openCreateWizard("s3");

		const endpointInput = screen.getByLabelText("endpoint");
		fireEvent.change(endpointInput, {
			target: {
				value: " https://s3.example.test/custom/path ",
			},
		});
		fireEvent.blur(endpointInput);

		expect(
			screen.getByDisplayValue("https://s3.example.test/custom/path"),
		).toBeInTheDocument();
	});

	it("blocks S3-compatible endpoints that omit the http or https protocol", async () => {
		render(<AdminPoliciesPage />);

		openCreateWizard("s3");

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Archive S3" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "s3.example.com" },
		});
		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "archive" },
		});

		expect(screen.getByLabelText("endpoint")).toHaveAttribute(
			"aria-invalid",
			"true",
		);
		expect(
			screen.getByText("s3_endpoint_protocol_required_error"),
		).toBeInTheDocument();

		fireEvent.click(screen.getByRole("button", { name: /test_connection/i }));
		await waitFor(() => {
			expect(mockState.toastError).toHaveBeenCalledWith(
				"s3_endpoint_protocol_required_error",
			);
		});
		expect(mockState.testParams).not.toHaveBeenCalled();

		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);
		expect(
			screen.queryByText("policy_wizard_summary_desc"),
		).not.toBeInTheDocument();
	});

	it("allows http S3 endpoints because the backend accepts http or https", async () => {
		render(<AdminPoliciesPage />);

		openCreateWizard("s3");

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Archive S3" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "http://s3.example.com" },
		});
		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "archive" },
		});

		fireEvent.click(screen.getByRole("button", { name: /test_connection/i }));

		await waitFor(() => {
			expect(mockState.testParams).toHaveBeenCalledWith({
				access_key: undefined,
				base_path: undefined,
				bucket: "archive",
				driver_type: "s3",
				endpoint: "http://s3.example.com",
				options: {
					object_storage_download_strategy: "relay_stream",
					object_storage_upload_strategy: "relay_stream",
				},
				remote_node_id: undefined,
				secret_key: undefined,
			});
		});
	});

	it("blocks Tencent COS endpoints that omit the http or https protocol", async () => {
		render(<AdminPoliciesPage />);

		openCreateWizard("tencent_cos");

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "COS Media" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "cos.ap-guangzhou.myqcloud.com" },
		});
		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "media-1250000000" },
		});

		expect(screen.getByLabelText("endpoint")).toHaveAttribute(
			"aria-invalid",
			"true",
		);
		expect(
			screen.getByText("s3_endpoint_protocol_required_error"),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);
		expect(
			screen.queryByText("policy_wizard_summary_desc"),
		).not.toBeInTheDocument();
		expect(mockState.testParams).not.toHaveBeenCalled();
	});

	it("blocks Azure Blob endpoints that omit the http or https protocol", async () => {
		render(<AdminPoliciesPage />);

		openCreateWizard("azure_blob");

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Azure Archive" },
		});
		fireEvent.change(screen.getByLabelText("endpoint"), {
			target: { value: "acct.blob.core.windows.net" },
		});
		fireEvent.change(screen.getByLabelText("bucket"), {
			target: { value: "archive" },
		});

		expect(screen.getByLabelText("endpoint")).toHaveAttribute(
			"aria-invalid",
			"true",
		);
		expect(
			screen.getByText("azure_blob_endpoint_protocol_required_error"),
		).toBeInTheDocument();

		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);
		expect(
			screen.queryByText("policy_wizard_summary_desc"),
		).not.toBeInTheDocument();
		expect(mockState.testParams).not.toHaveBeenCalled();
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
				options: { object_storage_upload_strategy: "presigned" },
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
						object_storage_download_strategy: "relay_stream",
						object_storage_upload_strategy: "presigned",
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

	it("renders object storage transfer controls in the edit form", () => {
		mockState.items = [
			createPolicy({
				id: 11,
				name: "Azure Archive",
				driver_type: "azure_blob",
				endpoint: "https://acct.blob.core.windows.net",
				bucket: "archive",
				options: {
					object_storage_download_strategy: "presigned",
					object_storage_upload_strategy: "presigned",
				},
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("Azure Archive");

		expect(
			screen.getByText("object_storage_upload_strategy"),
		).toBeInTheDocument();
		expect(
			screen.getByText("object_storage_download_strategy"),
		).toBeInTheDocument();
		expect(
			screen.getAllByRole("button", { name: "select-item:presigned" }),
		).toHaveLength(2);
		expect(
			screen.getByText("policy_edit_context_object_storage_desc"),
		).toBeInTheDocument();
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
				options: { object_storage_upload_strategy: "relay_stream" },
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
				options: {
					object_storage_download_strategy: "relay_stream",
					object_storage_upload_strategy: "relay_stream",
				},
				policy_id: 9,
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
					object_storage_download_strategy: "relay_stream",
					object_storage_upload_strategy: "relay_stream",
				},
				secret_key: "NEWSECRET",
			}),
		);
		expect(payload).toHaveProperty("secret_key", "NEWSECRET");
		expect(mockState.toastSuccess).toHaveBeenCalledWith("policy_updated");
	});

	it("preserves zero limit inputs in edit mode", () => {
		mockState.items = [
			createPolicy({
				id: 8,
				name: "Direct Put S3",
				driver_type: "s3",
				endpoint: "https://s3.example.com",
				bucket: "direct-put",
				max_file_size: 0,
				chunk_size: 0,
				options: { object_storage_upload_strategy: "presigned" },
			}),
		];

		render(<AdminPoliciesPage />);

		openEditPolicy("Direct Put S3");

		expect(screen.getByDisplayValue("Direct Put S3")).toBeInTheDocument();
		expect(screen.getAllByDisplayValue("0")).toHaveLength(2);
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
				options: {
					object_storage_download_strategy: "relay_stream",
					object_storage_upload_strategy: "relay_stream",
				},
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
					object_storage_download_strategy: "relay_stream",
					object_storage_upload_strategy: "relay_stream",
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

	it("moves back one page after deleting the only policy on a later page", async () => {
		mockState.items = [
			createPolicy({
				id: 18,
				name: "Last Page Policy",
			}),
		];
		mockState.searchParams = "offset=20&pageSize=20";
		mockState.total = 21;

		render(<AdminPoliciesPage />);

		fireEvent.click(screen.getByRole("button", { name: "delete_policy" }));
		fireEvent.click(
			within(
				screen.getByText('delete_policy "Last Page Policy"?')
					.parentElement as HTMLElement,
			).getByRole("button", { name: "core:delete" }),
		);

		await waitFor(() => {
			expect(mockState.deletePolicy).toHaveBeenCalledWith(18);
		});
		await waitFor(() => {
			expect(mockState.setSearchParams).toHaveBeenLastCalledWith(
				new URLSearchParams(""),
				{ replace: true },
			);
		});
		expect(mockState.reload).not.toHaveBeenCalled();
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
				new ApiError(
					ApiErrorCode.PolicyUploadSessionsExist,
					"upload sessions exist",
				),
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
				options: {
					object_storage_download_strategy: "relay_stream",
					object_storage_upload_strategy: "relay_stream",
				},
				remote_node_id: undefined,
				secret_key: undefined,
			});
		});
		expect(mockState.handleApiError).toHaveBeenCalledWith(validationError);
		expect(mockState.testConnection).not.toHaveBeenCalled();
	});
});
