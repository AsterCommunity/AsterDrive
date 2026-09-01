import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import type { ComponentProps } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PolicyFormData } from "@/components/admin/storage-policy-editor/formTypes";
import type { StoragePolicyEditorForm } from "@/components/admin/storage-policy-editor/StoragePolicyEditorForm";
import { invalidateAdminRemoteNodeLookup } from "@/lib/adminRemoteNodeLookup";
import { invalidateAdminStorageConnectorLocalizations } from "@/lib/adminStorageConnectorLocalizations";
import { invalidateAdminStorageDriverDescriptors } from "@/lib/adminStorageDriverDescriptors";
import AdminPoliciesPage from "@/pages/admin/AdminPoliciesPage";
import AdminPolicyEditPage from "@/pages/admin/AdminPolicyEditPage";
import type {
	RemoteNodeInfo,
	RemoteStorageTargetInfo,
	StorageConnectorActionDescriptor,
	StorageConnectorCredentialInfo,
	StorageConnectorCredentialManagementDescriptor,
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
	StoragePolicy,
} from "@/types/api";

type FormProps = ComponentProps<typeof StoragePolicyEditorForm>;

const mockState = vi.hoisted(() => ({
	create: vi.fn(),
	formProps: null as unknown,
	navigate: vi.fn(),
	policyId: "new" as string | undefined,
	executeDraftPolicyAction: vi.fn(),
	executeSavedPolicyAction: vi.fn(),
	getCapacity: vi.fn(),
	getPolicy: vi.fn(),
	handleApiError: vi.fn(),
	listPolicies: vi.fn(),
	listRemoteNodes: vi.fn(),
	listStorageCredentials: vi.fn(),
	listStorageDriverDescriptors: vi.fn(),
	listStorageDriverLocalizations: vi.fn(),
	listStorageTargetDrivers: vi.fn(),
	listStorageTargets: vi.fn(),
	logout: vi.fn(),
	manageDescriptors: [] as unknown[],
	createDescriptors: [] as unknown[],
	setupDescriptors: [] as unknown[],
	policies: [] as unknown[],
	promoteConnector: vi.fn(),
	remoteNodes: [] as unknown[],
	searchParams: new URLSearchParams(),
	setSearchParams: vi.fn(),
	setupRefresh: vi.fn(),
	startStorageAuthorization: vi.fn(),
	testConnection: vi.fn(),
	testParams: vi.fn(),
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
	update: vi.fn(),
	validateStorageCredential: vi.fn(),
}));

const translate = vi.hoisted(
	() => (key: string, values?: Record<string, unknown>) => {
		if (typeof values?.defaultValue === "string") return values.defaultValue;
		return values?.field ? `${key}:${String(values.field)}` : key;
	},
);

const testI18n = vi.hoisted(() => ({
	addResourceBundle: vi.fn(),
	language: "en",
	resolvedLanguage: "en",
}));

vi.mock("react-router-dom", () => ({
	Navigate: ({ to }: { to: string }) => <div data-testid="navigate">{to}</div>,
	useNavigate: () => mockState.navigate,
	useParams: () => ({ policyId: mockState.policyId }),
	useSearchParams: () => [mockState.searchParams, mockState.setSearchParams],
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({ i18n: testI18n, t: translate }),
}));

vi.mock("sonner", () => ({
	toast: {
		error: (...args: unknown[]) => mockState.toastError(...args),
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/hooks/usePageTitle", () => ({ usePageTitle: vi.fn() }));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: (
		selector: (state: { logout: typeof mockState.logout }) => unknown,
	) => selector({ logout: mockState.logout }),
}));

vi.mock("@/stores/systemSetupStore", () => ({
	useSystemSetupStore: (
		selector: (state: { refresh: typeof mockState.setupRefresh }) => unknown,
	) => selector({ refresh: mockState.setupRefresh }),
}));

vi.mock("@/components/layout/AdminLayout", () => ({
	AdminLayout: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));

vi.mock("@/components/layout/AdminPageShell", () => ({
	AdminPageShell: ({ children }: { children: React.ReactNode }) => (
		<>{children}</>
	),
}));

vi.mock("@/components/layout/AdminPageHeader", () => ({
	AdminPageHeader: ({
		actions,
		title,
	}: {
		actions?: React.ReactNode;
		title: string;
	}) => (
		<header>
			<h1>{title}</h1>
			{actions}
		</header>
	),
}));

vi.mock("@/components/ui/button", () => ({
	Button: ({
		children,
		disabled,
		onClick,
	}: {
		children: React.ReactNode;
		disabled?: boolean;
		onClick?: () => void;
	}) => (
		<button type="button" disabled={disabled} onClick={onClick}>
			{children}
		</button>
	),
}));

vi.mock("@/components/ui/icon", () => ({
	Icon: ({ name }: { name: string }) => <span>{name}</span>,
}));

vi.mock("@/components/common/AsterDriveWordmark", () => ({
	AsterDriveWordmark: () => <span>wordmark</span>,
}));

vi.mock(
	"@/components/admin/storage-policy-editor/StoragePolicyEditorForm",
	() => ({
		StoragePolicyEditorForm: (props: FormProps) => {
			mockState.formProps = props;
			return (
				<div data-testid="policy-editor-form">
					<span data-testid="connector-id">{props.form.connector_id}</span>
					<span data-testid="create-step">{props.createStep}</span>
				</div>
			);
		},
	}),
);

vi.mock("@/services/adminService", () => ({
	adminPolicyService: {
		create: (...args: unknown[]) => mockState.create(...args),
		createMigration: vi.fn(),
		delete: vi.fn(),
		dryRunMigration: vi.fn(),
		executeDraftPolicyAction: (...args: unknown[]) =>
			mockState.executeDraftPolicyAction(...args),
		executeSavedPolicyAction: (...args: unknown[]) =>
			mockState.executeSavedPolicyAction(...args),
		promoteConnector: (...args: unknown[]) =>
			mockState.promoteConnector(...args),
		get: (...args: unknown[]) => mockState.getPolicy(...args),
		getCapacity: (...args: unknown[]) => mockState.getCapacity(...args),
		list: (...args: unknown[]) => mockState.listPolicies(...args),
		listAll: vi.fn(async () => []),
		listStorageCredentials: (...args: unknown[]) =>
			mockState.listStorageCredentials(...args),
		listStorageDriverDescriptors: (query?: { context?: string }) =>
			mockState.listStorageDriverDescriptors(query),
		listStorageDriverLocalizations: (query?: {
			context?: string;
			locale?: string;
		}) => mockState.listStorageDriverLocalizations(query),
		startStorageAuthorization: (...args: unknown[]) =>
			mockState.startStorageAuthorization(...args),
		testConnection: (...args: unknown[]) => mockState.testConnection(...args),
		testParams: (...args: unknown[]) => mockState.testParams(...args),
		update: (...args: unknown[]) => mockState.update(...args),
		validateStorageCredential: (...args: unknown[]) =>
			mockState.validateStorageCredential(...args),
	},
	adminRemoteNodeService: {
		createStorageTarget: vi.fn(),
		list: (...args: unknown[]) => mockState.listRemoteNodes(...args),
		listStorageTargetDrivers: (...args: unknown[]) =>
			mockState.listStorageTargetDrivers(...args),
		listStorageTargets: (...args: unknown[]) =>
			mockState.listStorageTargets(...args),
	},
}));

const draftTestAction = action({
	action_id: "test_draft_connection",
	kind: "connection_test",
	endpoints: ["test_policy_params"],
});
const savedTestAction = action({
	action_id: "test_saved_connection",
	kind: "connection_test",
	endpoints: ["test_policy_connection"],
	requires_saved_policy: true,
});
const authorizationAction = action({
	action_id: "start_authorization",
	kind: "authorization",
	endpoints: ["start_storage_authorization"],
	requires_saved_policy: true,
});
const credentialValidationAction = action({
	action_id: "validate_credential",
	kind: "credential_validation",
	endpoints: ["validate_storage_credential"],
	requires_saved_policy: true,
});

function credentialManagement(): StorageConnectorCredentialManagementDescriptor {
	return {
		authorization_started_key: "plugin_authorization_started",
		created_authorize_next_key: "plugin_created_authorize_next",
		loading_key: "plugin_credential_loading",
		redirect_uri_key: "plugin_redirect_uri",
		save_before_authorize_key: "plugin_save_before_authorize",
		save_before_validate_key: "plugin_save_before_validate",
		status_presentations: {
			authorized: {
				label_key: "plugin_credential_authorized",
				tone: "success",
			},
			missing: {
				label_key: "plugin_credential_missing",
				tone: "neutral",
			},
		},
		title_key: "plugin_credential_title",
		validation_success_detail_key: "plugin_validation_success_detail",
		validation_success_key: "plugin_validation_success",
	};
}

function action(
	overrides: Partial<StorageConnectorActionDescriptor> &
		Pick<StorageConnectorActionDescriptor, "action_id" | "kind">,
): StorageConnectorActionDescriptor {
	return {
		action_id: overrides.action_id,
		description_key: `${overrides.action_id}_desc`,
		endpoints: overrides.endpoints ?? [],
		fields: overrides.fields ?? [],
		kind: overrides.kind,
		label_key: overrides.action_id,
		mutates_remote_state: false,
		requires_authorization: false,
		requires_confirmation: false,
		requires_saved_policy: false,
		...overrides,
	};
}

function field(
	name: string,
	overrides: Partial<StorageConnectorFieldDescriptor> = {},
): StorageConnectorFieldDescriptor {
	return {
		kind: "text",
		label_key: name,
		name,
		required: false,
		scope: "connector_config",
		secret: false,
		...overrides,
	};
}

function descriptor(
	connectorId: string,
	overrides: Partial<StorageConnectorDescriptor> = {},
): StorageConnectorDescriptor {
	return {
		actions: [],
		authorization_provider: null,
		capabilities: {
			capacity: true,
			efficient_range: true,
			list: true,
			object_naming: "opaque_uuid",
			object_storage_transfer_strategy: false,
			presigned_download: false,
			remote_node_binding: false,
			storage_native_media_metadata: false,
			storage_native_thumbnail: false,
		},
		config_schema_version: 1,
		connector_id: connectorId,
		credential_mode: "none",
		deployment_scope: "shared_across_primary_instances",
		description: `${connectorId} description`,
		fields: [],
		label: connectorId,
		related_issues: [],
		requires_authorization: false,
		supports_initial_setup: true,
		ui: {
			badge_rgb: { red: 113, green: 113, blue: 122 },
			base_path_empty_display: "core:root",
			base_path_placeholder: "path",
			config_step_description_key: "config_desc",
			config_step_title_key: "config_title",
			description_key: "connector_desc",
			edit_context_key: "edit_context",
			helper_key: "helper",
			icon_name: null,
			icon_src: null,
			label_key: connectorId,
		},
		upload_workflows: {
			frontend_direct_provider_resumable_upload: false,
			object_multipart_upload: false,
			object_multipart_upload_capabilities: null,
			presigned_upload: false,
			provider_resumable_upload: false,
			provider_resumable_upload_capabilities: null,
			simple_upload: true,
			simple_upload_capabilities: {
				max_provider_single_request_size: null,
				policy_limited: true,
				server_side_relay: true,
			},
			stream_upload: true,
		},
		...overrides,
	};
}

function promotionDescriptors() {
	const source = descriptor("asterdrive.storage.s3", {
		credential_mode: "static_secret",
		credential_schema_version: 1,
		fields: [
			field("endpoint", { required: true }),
			field("bucket", { required: true }),
			field("base_path", { default_value: "" }),
			field("s3_access_key_id", {
				required: true,
				scope: "static_credential",
			}),
			field("s3_secret_access_key", {
				kind: "secret",
				required: true,
				scope: "static_credential",
				secret: true,
			}),
		],
	});
	const target = descriptor("asterdrive.storage.tencent_cos", {
		credential_mode: "static_secret",
		credential_schema_version: 1,
		fields: [
			field("endpoint", { required: true }),
			field("bucket", { required: true }),
			field("base_path", { default_value: "" }),
			field("tencent_cos_secret_id", {
				required: true,
				scope: "static_credential",
			}),
			field("tencent_cos_secret_key", {
				kind: "secret",
				required: true,
				scope: "static_credential",
				secret: true,
			}),
		],
		promotions: [
			{
				config_mappings: [
					{ source_field: "endpoint", target_field: "endpoint" },
					{
						preserve_value: true,
						source_field: "bucket",
						target_field: "bucket",
					},
					{
						preserve_value: true,
						source_field: "base_path",
						target_field: "base_path",
					},
				],
				confirmation_key: "promotion_confirm",
				credential_mappings: [
					{
						source_field: "s3_access_key_id",
						target_field: "tencent_cos_secret_id",
					},
					{
						source_field: "s3_secret_access_key",
						target_field: "tencent_cos_secret_key",
					},
				],
				description_key: "promotion_desc",
				promotion_id: "promote_from_s3",
				requirements: [
					{
						matcher: {
							kind: "url_host_suffix",
							suffix: ".myqcloud.com",
						},
						source_field: "endpoint",
					},
				],
				source_connector_id: source.connector_id,
			},
		],
	});
	return { source, target };
}

function policy(
	connectorId: string,
	values: Record<string, boolean | number | string> = {},
	overrides: Partial<StoragePolicy> = {},
): StoragePolicy {
	return {
		allowed_types: [],
		behavior: {},
		chunk_size: 5 * 1024 * 1024,
		connector_config: {
			connector_id: connectorId,
			format_version: 1,
			schema_version: 1,
			values,
		},
		connector_id: connectorId,
		created_at: "2026-08-04T00:00:00Z",
		id: 7,
		is_default: false,
		max_file_size: 0,
		name: "Policy",
		updated_at: "2026-08-04T00:00:00Z",
		...overrides,
	};
}

function credential(
	status: StorageConnectorCredentialInfo["status"],
	updatedAt = "2026-08-04T00:00:00Z",
): StorageConnectorCredentialInfo {
	return {
		account_label: "Admin account",
		authorized_at: "2026-08-01T00:00:00Z",
		created_at: "2026-08-01T00:00:00Z",
		credential_kind: "authorization",
		id: 1,
		last_refreshed_at: "2026-08-02T00:00:00Z",
		last_validated_at: "2026-08-03T00:00:00Z",
		policy_id: 7,
		provider: "microsoft_graph",
		scopes: [],
		status,
		updated_at: updatedAt,
	};
}

function remoteNode(id: number, name: string): RemoteNodeInfo {
	return {
		base_url: `https://node-${id}.example.com`,
		capabilities: {},
		created_at: "2026-08-04T00:00:00Z",
		enrollment_status: "completed",
		id,
		is_enabled: true,
		last_probe_at: null,
		last_probe_error: "",
		name,
		transport_mode: "direct",
		tunnel: { runtime_error: "", last_handshake_at: null, status: "offline" },
		updated_at: "2026-08-04T00:00:00Z",
	};
}

function remoteTarget(
	targetKey: string,
	overrides: Partial<RemoteStorageTargetInfo> = {},
): RemoteStorageTargetInfo {
	return {
		applied_revision: 1,
		base_path: "",
		bucket: "",
		created_at: "2026-08-04T00:00:00Z",
		desired_revision: 1,
		driver_type: "local",
		endpoint: "",
		is_default: true,
		last_probe_error: "",
		name: targetKey,
		target_key: targetKey,
		updated_at: "2026-08-04T00:00:00Z",
		...overrides,
	};
}

function currentForm(): FormProps {
	return mockState.formProps as FormProps;
}

async function waitForCatalog(connectorId?: string) {
	await waitFor(() => {
		expect(currentForm().storageDriverDescriptorsLoading).toBe(false);
		if (connectorId) {
			expect(
				currentForm().storageDriverDescriptors.some(
					(item) => item.connector_id === connectorId,
				),
			).toBe(true);
		}
	});
}

async function setField<K extends keyof PolicyFormData>(
	key: K,
	value: PolicyFormData[K],
) {
	await act(async () => currentForm().onFieldChange(key, value));
}

async function submitEditorForm() {
	const form = document.querySelector("form");
	expect(form).not.toBeNull();
	await act(async () => {
		fireEvent.submit(form as HTMLFormElement);
	});
}

async function runConnectionTest() {
	await act(async () => {
		fireEvent.click(screen.getByRole("button", { name: /test_connection/ }));
	});
}

function deferred<T>() {
	let resolve!: (value: T) => void;
	let reject!: (reason?: unknown) => void;
	const promise = new Promise<T>((resolvePromise, rejectPromise) => {
		resolve = resolvePromise;
		reject = rejectPromise;
	});
	return { promise, reject, resolve };
}

describe("AdminPolicyEditPage editor orchestration", () => {
	beforeEach(() => {
		invalidateAdminRemoteNodeLookup();
		invalidateAdminStorageConnectorLocalizations();
		invalidateAdminStorageDriverDescriptors();
		testI18n.addResourceBundle.mockReset();
		testI18n.language = "en";
		testI18n.resolvedLanguage = "en";
		mockState.create.mockReset();
		mockState.formProps = null;
		mockState.executeDraftPolicyAction.mockReset();
		mockState.executeSavedPolicyAction.mockReset();
		mockState.getPolicy.mockReset();
		mockState.getPolicy.mockImplementation(async (id: number) => {
			const found = (mockState.policies as Array<{ id: number }>).find(
				(candidate) => candidate.id === id,
			);
			if (!found) throw new Error(`Policy ${id} not found`);
			return found;
		});
		mockState.handleApiError.mockReset();
		mockState.listPolicies.mockReset();
		mockState.listRemoteNodes.mockReset();
		mockState.listStorageCredentials.mockReset();
		mockState.listStorageDriverDescriptors.mockReset();
		mockState.listStorageDriverLocalizations.mockReset();
		mockState.listStorageTargetDrivers.mockReset();
		mockState.listStorageTargets.mockReset();
		mockState.logout.mockReset();
		mockState.navigate.mockReset();
		mockState.policyId = "new";
		mockState.searchParams = new URLSearchParams();
		mockState.setSearchParams.mockReset();
		mockState.setupRefresh.mockReset();
		mockState.startStorageAuthorization.mockReset();
		mockState.tableProps = null;
		mockState.testConnection.mockReset();
		mockState.testParams.mockReset();
		mockState.toastError.mockReset();
		mockState.toastSuccess.mockReset();
		mockState.update.mockReset();
		mockState.validateStorageCredential.mockReset();

		mockState.manageDescriptors = [];
		mockState.createDescriptors = [];
		mockState.setupDescriptors = [];
		mockState.policies = [];
		mockState.promoteConnector.mockReset();
		mockState.remoteNodes = [];
		mockState.listPolicies.mockImplementation(async () => ({
			items: mockState.policies,
			total: mockState.policies.length,
		}));
		mockState.listStorageDriverDescriptors.mockImplementation(
			async (query?: { context?: string }) => {
				switch (query?.context) {
					case "create":
						return mockState.createDescriptors;
					case "setup":
						return mockState.setupDescriptors;
					default:
						return mockState.manageDescriptors;
				}
			},
		);
		mockState.listStorageDriverLocalizations.mockResolvedValue({
			requested_locale: "en",
			resources: [],
		});
		mockState.listRemoteNodes.mockImplementation(async () => ({
			items: mockState.remoteNodes,
			total: mockState.remoteNodes.length,
		}));
		mockState.listStorageTargetDrivers.mockResolvedValue([]);
		mockState.listStorageTargets.mockResolvedValue([]);
		mockState.listStorageCredentials.mockResolvedValue([]);
		mockState.getCapacity.mockResolvedValue({
			blob_count: 0,
			blob_total_bytes: 0,
			capacity: { status: "unsupported" },
			connector_id: "connector",
			policy_id: 7,
		});
		mockState.setupRefresh.mockResolvedValue(undefined);
		vi.spyOn(window, "open").mockReturnValue({ opener: null } as Window);
	});

	it("loads manage/create catalogs and selects the first creatable connector", async () => {
		const existingOnly = descriptor("plugin.existing");
		const firstCreatable = descriptor("plugin.first", {
			fields: [field("prefix", { default_value: "uploads" })],
		});
		mockState.manageDescriptors = [existingOnly, firstCreatable];
		mockState.createDescriptors = [firstCreatable];

		render(<AdminPolicyEditPage />);
		await waitForCatalog("plugin.first");

		await waitFor(() => {
			expect(currentForm().form.connector_id).toBe("plugin.first");
			expect(currentForm().form.connector_config_values).toEqual({
				prefix: "uploads",
			});
		});
		expect(currentForm().storageDriverDescriptors).toEqual([firstCreatable]);
		expect(mockState.listStorageDriverDescriptors).toHaveBeenCalledWith(
			undefined,
		);
		expect(mockState.listStorageDriverDescriptors).toHaveBeenCalledWith({
			context: "create",
		});
	});

	it("initializes and submits supported thumbnail defaults without enabling native processing", async () => {
		const connector = descriptor("plugin.native", {
			capabilities: {
				...descriptor("unused").capabilities,
				storage_native_thumbnail: true,
			},
		});
		mockState.manageDescriptors = [connector];
		mockState.createDescriptors = [connector];
		mockState.create.mockResolvedValue(policy(connector.connector_id));

		render(<AdminPolicyEditPage />);
		await waitForCatalog(connector.connector_id);

		await waitFor(() => {
			expect(currentForm().form).toMatchObject({
				connector_id: connector.connector_id,
				storage_native_thumbnail_enabled: false,
				storage_native_thumbnail_extensions: [
					"jpg",
					"jpeg",
					"png",
					"webp",
					"gif",
				],
			});
		});

		await setField("name", "Native thumbnails");
		await act(async () => currentForm().onCreateStepChange(2));
		await submitEditorForm();

		await waitFor(() => expect(mockState.create).toHaveBeenCalledTimes(1));
		expect(mockState.create).toHaveBeenCalledWith(
			expect.objectContaining({
				connection: expect.objectContaining({
					behavior: {
						storage_native_media_metadata_enabled: false,
						storage_native_media_metadata_extensions: [],
						storage_native_thumbnail_enabled: false,
						storage_native_thumbnail_extensions: [
							"jpg",
							"jpeg",
							"png",
							"webp",
							"gif",
						],
					},
				}),
			}),
		);
	});

	it("does not install connector resources from a stale language request", async () => {
		const enManage = deferred<{
			requested_locale: string;
			resources: never[];
		}>();
		const enCreate = deferred<{
			requested_locale: string;
			resources: never[];
		}>();
		mockState.listStorageDriverLocalizations.mockImplementation(
			(query?: { context?: string; locale?: string }) => {
				if (query?.locale === "en") {
					return query.context === "create"
						? enCreate.promise
						: enManage.promise;
				}
				return Promise.resolve({
					requested_locale: "zh",
					resources: [
						{
							connector_id: `plugin.${query?.context ?? "manage"}`,
							messages: { title: "插件" },
							namespace: `plugin.${query?.context ?? "manage"}`,
							requested_locale: "zh",
							resolved_locale: "zh",
							revision: "zh-revision",
						},
					],
				});
			},
		);

		const view = render(<AdminPolicyEditPage />);
		await waitFor(() =>
			expect(mockState.listStorageDriverLocalizations).toHaveBeenCalledWith({
				context: "manage",
				locale: "en",
			}),
		);

		testI18n.language = "zh";
		testI18n.resolvedLanguage = "zh";
		view.rerender(<AdminPolicyEditPage />);
		await waitFor(() =>
			expect(testI18n.addResourceBundle).toHaveBeenCalledWith(
				"zh",
				expect.stringMatching(/^plugin\./),
				{ title: "插件" },
				true,
				true,
			),
		);

		testI18n.addResourceBundle.mockClear();
		await act(async () => {
			enManage.resolve({ requested_locale: "en", resources: [] });
			enCreate.resolve({ requested_locale: "en", resources: [] });
			await Promise.all([enManage.promise, enCreate.promise]);
		});

		expect(testI18n.addResourceBundle).not.toHaveBeenCalled();
	});

	it("switches connectors using only target descriptor defaults", async () => {
		const first = descriptor("plugin.first", {
			fields: [field("first_path", { default_value: "first" })],
		});
		const second = descriptor("plugin.second", {
			fields: [
				field("region", { default_value: "region-b" }),
				field("enabled", { default_value: true, kind: "boolean" }),
			],
		});
		mockState.manageDescriptors = [first, second];
		mockState.createDescriptors = [first, second];

		render(<AdminPolicyEditPage />);
		await waitForCatalog("plugin.second");
		await waitFor(() =>
			expect(currentForm().form.connector_id).toBe("plugin.first"),
		);
		await setField("credential_values", { stale_secret: "secret" });
		await setField("storage_native_thumbnail_enabled", true);
		await setField("storage_native_thumbnail_extensions", ["jpg"]);
		await setField("storage_native_media_metadata_enabled", true);
		await setField("storage_native_media_metadata_extensions", ["mp4"]);

		await act(async () => currentForm().onConnectorIdChange("plugin.second"));

		expect(currentForm().form).toMatchObject({
			connector_id: "plugin.second",
			connector_config_values: { enabled: true, region: "region-b" },
			credential_values: {},
			storage_native_media_metadata_extensions: ["mp4"],
			storage_native_media_metadata_enabled: false,
			storage_native_thumbnail_extensions: ["jpg"],
			storage_native_thumbnail_enabled: false,
		});
	});

	it("applies target-owned promotion mappings to a create draft", async () => {
		const { source, target } = promotionDescriptors();
		mockState.manageDescriptors = [source, target];
		mockState.createDescriptors = [source, target];

		render(<AdminPolicyEditPage />);
		await waitForCatalog(source.connector_id);
		await setField("connector_config_values", {
			endpoint: "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
			bucket: "media-1250000000",
			base_path: "tenant/files",
		});
		await setField("credential_values", {
			s3_access_key_id: "AKIDEXAMPLE",
			s3_secret_access_key: "SECRETEXAMPLE",
		});

		await waitFor(() =>
			expect(currentForm().connectorPromotionCandidates).toHaveLength(1),
		);
		await act(async () =>
			currentForm().onApplyDraftConnectorPromotion(
				currentForm().connectorPromotionCandidates[0],
			),
		);

		expect(currentForm().form.connector_id).toBe(target.connector_id);
		expect(currentForm().form.connector_config_values).toMatchObject({
			endpoint: "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
			bucket: "media-1250000000",
			base_path: "tenant/files",
		});
		expect(currentForm().form.credential_values).toEqual({
			tencent_cos_secret_id: "AKIDEXAMPLE",
			tencent_cos_secret_key: "SECRETEXAMPLE",
		});
	});

	it("blocks dirty saved promotions and refreshes the editor after success", async () => {
		const { source, target } = promotionDescriptors();
		const saved = policy(source.connector_id, {
			endpoint: "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
			bucket: "media-1250000000",
			base_path: "tenant/files",
		});
		const promoted = policy(
			target.connector_id,
			{
				endpoint: "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
				bucket: "media-1250000000",
				base_path: "tenant/files",
			},
			{ updated_at: "2026-08-17T00:00:00Z" },
		);
		const untouched = policy(source.connector_id, {}, { id: 8, name: "Other" });
		mockState.manageDescriptors = [source, target];
		mockState.createDescriptors = [source, target];
		mockState.policies = [saved, untouched];
		mockState.promoteConnector.mockResolvedValue(promoted);

		mockState.policyId = "7";
		render(<AdminPolicyEditPage />);
		await waitForCatalog(source.connector_id);

		await waitFor(() =>
			expect(currentForm().connectorPromotionCandidates).toHaveLength(1),
		);
		const candidate = currentForm().connectorPromotionCandidates[0];

		await setField("name", "Dirty policy");
		expect(currentForm().connectorPromotionBlocked).toBe(true);
		await act(async () => currentForm().onRequestConnectorPromotion(candidate));
		expect(currentForm().connectorPromotionConfirmKey).toBeNull();

		await setField("name", saved.name);
		await waitFor(() =>
			expect(currentForm().connectorPromotionBlocked).toBe(false),
		);
		await act(async () => currentForm().onRequestConnectorPromotion(candidate));
		expect(currentForm().connectorPromotionConfirmKey).toBe(
			`${target.connector_id}:promote_from_s3`,
		);
		await act(async () => currentForm().onConfirmConnectorPromotion(candidate));

		await waitFor(() =>
			expect(mockState.promoteConnector).toHaveBeenCalledWith(7, {
				target_connector_id: target.connector_id,
				promotion_id: "promote_from_s3",
			}),
		);
		expect(currentForm().form.connector_id).toBe(target.connector_id);
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"policy_connector_promotion_success",
		);
	});

	it("does not recommend promotion targets excluded from the create catalog", async () => {
		const { source, target } = promotionDescriptors();
		mockState.manageDescriptors = [source, target];
		mockState.createDescriptors = [source];

		render(<AdminPolicyEditPage />);
		await waitForCatalog(source.connector_id);
		await setField("connector_config_values", {
			endpoint: "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
			bucket: "media-1250000000",
			base_path: "tenant/files",
		});

		await waitFor(() =>
			expect(currentForm().connectorPromotionCandidates).toEqual([]),
		);
	});

	it("blocks connector promotion while policy edits are dirty", async () => {
		const { source, target } = promotionDescriptors();
		const nonMatchingSaved = policy(source.connector_id, {
			endpoint: "https://s3.example.test",
			bucket: "archive",
			base_path: "",
		});
		mockState.manageDescriptors = [source, target];
		mockState.createDescriptors = [source, target];
		mockState.policies = [nonMatchingSaved];

		mockState.policyId = "7";
		const view = render(<AdminPolicyEditPage />);
		await waitForCatalog(source.connector_id);

		await setField("connector_config_values", {
			endpoint: "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
			bucket: "archive",
			base_path: "",
		});
		await waitFor(() =>
			expect(currentForm().connectorPromotionCandidates).toHaveLength(1),
		);
		expect(currentForm().connectorPromotionBlocked).toBe(true);
		await act(async () =>
			currentForm().onRequestConnectorPromotion(
				currentForm().connectorPromotionCandidates[0],
			),
		);
		expect(currentForm().connectorPromotionConfirmKey).toBeNull();

		const matchingSaved = policy(source.connector_id, {
			endpoint: "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
			bucket: "archive",
			base_path: "",
		});
		mockState.getPolicy.mockResolvedValue(matchingSaved);
		view.unmount();
		render(<AdminPolicyEditPage />);
		await waitFor(() =>
			expect(currentForm().form.connector_config_values).toEqual({
				endpoint: "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
				bucket: "archive",
				base_path: "",
			}),
		);
		await setField("connector_config_values", {
			endpoint: "https://s3.example.test",
			bucket: "archive",
			base_path: "",
		});
		await waitFor(() =>
			expect(currentForm().connectorPromotionCandidates).toHaveLength(1),
		);
		expect(currentForm().connectorPromotionBlocked).toBe(true);
	});

	it("keeps the source editor retryable when promotion fails", async () => {
		const { source, target } = promotionDescriptors();
		const saved = policy(source.connector_id, {
			endpoint: "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
			bucket: "media-1250000000",
			base_path: "tenant/files",
		});
		const promotionError = new Error("promotion failed");
		mockState.manageDescriptors = [source, target];
		mockState.createDescriptors = [source, target];
		mockState.policies = [saved];
		mockState.promoteConnector.mockRejectedValue(promotionError);

		mockState.policyId = "7";
		render(<AdminPolicyEditPage />);
		await waitForCatalog(source.connector_id);

		await waitFor(() =>
			expect(currentForm().connectorPromotionCandidates).toHaveLength(1),
		);
		const candidate = currentForm().connectorPromotionCandidates[0];
		await act(async () => currentForm().onRequestConnectorPromotion(candidate));
		await act(async () => currentForm().onConfirmConnectorPromotion(candidate));

		await waitFor(() =>
			expect(mockState.handleApiError).toHaveBeenCalledWith(promotionError),
		);
		expect(currentForm().form.connector_id).toBe(source.connector_id);
		expect(currentForm().connectorPromotionSubmittingKey).toBeNull();
		expect(currentForm().connectorPromotionConfirmKey).toBe(
			`${target.connector_id}:promote_from_s3`,
		);
	});

	it("creates a policy from generic config and static credential fields", async () => {
		const connector = descriptor("plugin.static", {
			actions: [],
			credential_mode: "static_secret",
			fields: [
				field("endpoint", { required: true, trim_on_blur: true }),
				field("plugin_access_key", {
					required: true,
					scope: "static_credential",
				}),
				field("plugin_secret_key", {
					kind: "secret",
					required: true,
					scope: "static_credential",
					secret: true,
				}),
			],
		});
		mockState.manageDescriptors = [connector];
		mockState.createDescriptors = [connector];
		mockState.create.mockResolvedValue(
			policy("plugin.static", { endpoint: "https://storage.example.com" }),
		);

		render(<AdminPolicyEditPage />);
		await waitForCatalog("plugin.static");
		await waitFor(() =>
			expect(currentForm().form.connector_id).toBe("plugin.static"),
		);
		await setField("name", "Plugin Policy");
		await setField("connector_config_values", {
			endpoint: " https://storage.example.com ",
		});
		await setField("credential_values", {
			plugin_access_key: "ACCESS",
			plugin_secret_key: "SECRET",
		});
		await act(async () => currentForm().onCreateStepChange(2));
		await submitEditorForm();

		await waitFor(() => expect(mockState.create).toHaveBeenCalledTimes(1));
		expect(mockState.create).toHaveBeenCalledWith({
			chunk_size: 5 * 1024 * 1024,
			connection: {
				behavior: {
					storage_native_media_metadata_extensions: [],
					storage_native_media_metadata_enabled: false,
					storage_native_thumbnail_extensions: [],
					storage_native_thumbnail_enabled: false,
				},
				connector_config: {
					connector_id: "plugin.static",
					format_version: 1,
					schema_version: 1,
					values: { endpoint: "https://storage.example.com" },
				},
				credential: {
					mode: "static",
					values: {
						plugin_access_key: "ACCESS",
						plugin_secret_key: "SECRET",
					},
				},
			},
			is_default: false,
			max_file_size: undefined,
			name: "Plugin Policy",
		});
	});

	it("returns to the config step when a required descriptor field is missing", async () => {
		const connector = descriptor("plugin.required", {
			fields: [field("bucket", { required: true })],
		});
		mockState.manageDescriptors = [connector];
		mockState.createDescriptors = [connector];

		render(<AdminPolicyEditPage />);
		await waitForCatalog("plugin.required");
		await setField("name", "Incomplete");
		await act(async () => currentForm().onCreateStepChange(2));
		await submitEditorForm();

		await waitFor(() => expect(currentForm().createStep).toBe(1));
		expect(currentForm().createStepTouched).toBe(true);
		expect(mockState.create).not.toHaveBeenCalled();
	});

	it("uses draft connection tests for new or changed values and saved tests otherwise", async () => {
		const connector = descriptor("plugin.testable", {
			actions: [draftTestAction, savedTestAction],
			fields: [field("path")],
		});
		const saved = policy("plugin.testable", { path: "saved" });
		mockState.manageDescriptors = [connector];
		mockState.createDescriptors = [connector];
		mockState.policies = [saved];
		mockState.testConnection.mockResolvedValue({ ok: true });
		mockState.testParams.mockResolvedValue({ ok: true });

		const view = render(<AdminPolicyEditPage />);
		await waitForCatalog("plugin.testable");
		await waitFor(() =>
			expect(currentForm().form.connector_id).toBe("plugin.testable"),
		);
		await setField("connector_config_values", { path: "draft" });
		await act(async () => currentForm().onCreateStepChange(1));
		await runConnectionTest();
		expect(mockState.testParams).toHaveBeenLastCalledWith(
			expect.objectContaining({
				connection: expect.objectContaining({
					connector_config: expect.objectContaining({
						values: { path: "draft" },
					}),
				}),
			}),
		);

		mockState.getPolicy.mockResolvedValue(saved);
		mockState.policyId = "7";
		view.unmount();
		render(<AdminPolicyEditPage />);
		await waitFor(() => expect(currentForm().mode).toBe("edit"));
		await runConnectionTest();
		expect(mockState.testConnection).toHaveBeenCalledWith(7);

		await setField("connector_config_values", { path: "changed" });
		await runConnectionTest();
		expect(mockState.testParams).toHaveBeenLastCalledWith(
			expect.objectContaining({ policy_id: 7 }),
		);
	});

	it("initializes edit state from the connector-owned envelope", async () => {
		const connector = descriptor("plugin.envelope", {
			fields: [field("prefix"), field("replicas", { kind: "number" })],
		});
		mockState.manageDescriptors = [connector];
		mockState.createDescriptors = [connector];
		mockState.policies = [
			policy(
				"plugin.envelope",
				{ prefix: "tenant-a", replicas: 3 },
				{
					behavior: {
						storage_native_media_metadata_extensions: ["mp4"],
						storage_native_media_metadata_enabled: true,
						storage_native_thumbnail_extensions: ["jpg"],
						storage_native_thumbnail_enabled: true,
					},
					chunk_size: 8 * 1024 * 1024,
					is_default: true,
					max_file_size: 4096,
					name: "Envelope Policy",
				},
			),
		];
		mockState.getPolicy.mockResolvedValue(mockState.policies[0]);

		mockState.policyId = "7";
		render(<AdminPolicyEditPage />);
		await waitForCatalog("plugin.envelope");

		await waitFor(() => {
			expect(currentForm().form).toEqual({
				chunk_size: "8",
				connector_config_values: { prefix: "tenant-a", replicas: 3 },
				connector_id: "plugin.envelope",
				credential_values: {},
				is_default: true,
				max_file_size: "4096",
				storage_native_media_metadata_extensions: ["mp4"],
				storage_native_media_metadata_enabled: true,
				name: "Envelope Policy",
				storage_native_thumbnail_extensions: ["jpg"],
				storage_native_thumbnail_enabled: true,
			});
		});
	});

	it("preserves dormant native configuration on a no-op Tencent COS edit", async () => {
		const connector = descriptor("asterdrive.storage.tencent_cos", {
			actions: [],
			capabilities: {
				...descriptor("unused").capabilities,
				storage_native_thumbnail: true,
				storage_native_media_metadata: true,
			},
			config_schema_version: 1,
			fields: [field("endpoint"), field("bucket"), field("base_path")],
		});
		const saved = policy(
			"asterdrive.storage.tencent_cos",
			{
				endpoint: "https://bucket.cos.example.test",
				bucket: "bucket-1250000000",
				base_path: "",
			},
			{
				behavior: {
					storage_native_thumbnail_enabled: false,
					storage_native_thumbnail_extensions: ["jpg"],
					storage_native_media_metadata_enabled: false,
					storage_native_media_metadata_extensions: ["mp4"],
				},
				connector_config: {
					connector_id: "asterdrive.storage.tencent_cos",
					format_version: 1,
					schema_version: 1,
					values: {
						endpoint: "https://bucket.cos.example.test",
						bucket: "bucket-1250000000",
						base_path: "",
					},
				},
			},
		);
		mockState.manageDescriptors = [connector];
		mockState.createDescriptors = [connector];
		mockState.policies = [saved];
		mockState.update.mockResolvedValue(saved);

		mockState.policyId = "7";
		render(<AdminPolicyEditPage />);
		await waitForCatalog("asterdrive.storage.tencent_cos");

		await waitFor(() => expect(currentForm().mode).toBe("edit"));
		expect(currentForm().form).toMatchObject({
			storage_native_thumbnail_enabled: false,
			storage_native_thumbnail_extensions: ["jpg"],
			storage_native_media_metadata_enabled: false,
			storage_native_media_metadata_extensions: ["mp4"],
		});

		await submitEditorForm();
		await waitFor(() => expect(mockState.update).toHaveBeenCalledTimes(1));
		expect(mockState.update).toHaveBeenCalledWith(7, {
			behavior: {
				storage_native_thumbnail_enabled: false,
				storage_native_thumbnail_extensions: ["jpg"],
				storage_native_media_metadata_enabled: false,
				storage_native_media_metadata_extensions: ["mp4"],
			},
			chunk_size: 5 * 1024 * 1024,
			connector_config: {
				connector_id: "asterdrive.storage.tencent_cos",
				format_version: 1,
				schema_version: 1,
				values: {
					endpoint: "https://bucket.cos.example.test",
					bucket: "bucket-1250000000",
				},
			},
			is_default: false,
			max_file_size: 0,
			name: "Policy",
		});
	});

	it("preserves an explicitly empty native extension set on a no-op edit", async () => {
		const connector = descriptor("asterdrive.storage.tencent_cos", {
			actions: [],
			capabilities: {
				...descriptor("unused").capabilities,
				storage_native_thumbnail: true,
				storage_native_media_metadata: true,
			},
		});
		const saved = policy(
			connector.connector_id,
			{},
			{
				behavior: {
					storage_native_thumbnail_enabled: false,
					storage_native_thumbnail_extensions: [],
					storage_native_media_metadata_enabled: false,
					storage_native_media_metadata_extensions: [],
				},
			},
		);
		mockState.manageDescriptors = [connector];
		mockState.createDescriptors = [connector];
		mockState.policies = [saved];
		mockState.update.mockResolvedValue(saved);

		mockState.policyId = "7";
		render(<AdminPolicyEditPage />);
		await waitForCatalog(connector.connector_id);

		await waitFor(() => expect(currentForm().mode).toBe("edit"));
		expect(currentForm().form).toMatchObject({
			storage_native_thumbnail_enabled: false,
			storage_native_thumbnail_extensions: [],
			storage_native_media_metadata_enabled: false,
			storage_native_media_metadata_extensions: [],
		});

		await submitEditorForm();
		await waitFor(() => expect(mockState.update).toHaveBeenCalledTimes(1));
		expect(mockState.update).toHaveBeenCalledWith(
			7,
			expect.objectContaining({
				behavior: {
					storage_native_thumbnail_enabled: false,
					storage_native_thumbnail_extensions: [],
					storage_native_media_metadata_enabled: false,
					storage_native_media_metadata_extensions: [],
				},
			}),
		);
	});

	it("ignores stale remote target responses after the dependency changes", async () => {
		const connector = descriptor("plugin.remote", {
			capabilities: {
				...descriptor("unused").capabilities,
				remote_node_binding: true,
			},
			credential_mode: "remote_node",
			fields: [
				field("node", {
					kind: "select",
					required: true,
					select: {
						data_source: "remote_nodes",
						value_kind: "integer",
					},
				}),
				field("target", {
					kind: "select",
					required: true,
					select: {
						data_source: "remote_storage_targets",
						depends_on: "node",
						value_kind: "string",
					},
				}),
			],
		});
		const first = deferred<RemoteStorageTargetInfo[]>();
		const second = deferred<RemoteStorageTargetInfo[]>();
		mockState.manageDescriptors = [connector];
		mockState.createDescriptors = [connector];
		mockState.remoteNodes = [remoteNode(1, "First"), remoteNode(2, "Second")];
		mockState.listStorageTargets.mockImplementation((nodeId: number) =>
			nodeId === 1 ? first.promise : second.promise,
		);

		render(<AdminPolicyEditPage />);
		await waitForCatalog("plugin.remote");
		await waitFor(() =>
			expect(currentForm().form.connector_id).toBe("plugin.remote"),
		);
		await setField("connector_config_values", { node: 1, target: "" });
		await waitFor(() =>
			expect(mockState.listStorageTargets).toHaveBeenCalledWith(1),
		);
		await setField("connector_config_values", { node: 2, target: "" });
		await waitFor(() =>
			expect(mockState.listStorageTargets).toHaveBeenCalledWith(2),
		);

		await act(async () => second.resolve([remoteTarget("second-target")]));
		await waitFor(() =>
			expect(currentForm().form.connector_config_values.target).toBe(
				"second-target",
			),
		);
		await act(async () => first.resolve([remoteTarget("first-target")]));

		expect(currentForm().form.connector_config_values).toEqual({
			node: 2,
			target: "second-target",
		});
	});

	it("starts authorization only for an unchanged saved connector action", async () => {
		const connector = descriptor("plugin.oauth", {
			actions: [authorizationAction],
			authorization_provider: "plugin_oauth",
			credential_management: credentialManagement(),
			credential_mode: "oauth_delegated",
			fields: [field("tenant", { scope: "authorization_application" })],
			requires_authorization: true,
			supports_initial_setup: false,
		});
		const saved = policy("plugin.oauth");
		mockState.manageDescriptors = [connector];
		mockState.createDescriptors = [connector];
		mockState.policies = [saved];
		mockState.startStorageAuthorization.mockResolvedValue({
			authorization_url: "https://provider.example.com/authorize",
		});

		mockState.policyId = "7";
		render(<AdminPolicyEditPage />);
		await waitForCatalog("plugin.oauth");

		await waitFor(() => expect(currentForm().mode).toBe("edit"));
		await act(async () => currentForm().onStartStorageAuthorization());

		await waitFor(() =>
			expect(mockState.startStorageAuthorization).toHaveBeenCalledWith(7),
		);
		expect(window.open).toHaveBeenCalledWith(
			"https://provider.example.com/authorize",
			"_blank",
		);
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"plugin_authorization_started",
		);

		mockState.startStorageAuthorization.mockClear();
		await setField("name", "Changed policy name");
		await act(async () => currentForm().onStartStorageAuthorization());

		expect(mockState.startStorageAuthorization).not.toHaveBeenCalled();
		expect(mockState.toastError).toHaveBeenCalledWith(
			"plugin_save_before_authorize",
		);
	});

	it("reloads the credential session when reopening the same policy", async () => {
		const connector = descriptor("plugin.oauth", {
			actions: [credentialValidationAction],
			authorization_provider: "plugin_oauth",
			credential_management: credentialManagement(),
			credential_mode: "oauth_delegated",
			requires_authorization: true,
			supports_initial_setup: false,
		});
		const saved = policy("plugin.oauth");
		const initiallyAuthorized = credential("authorized");
		const validated = credential("authorized", "2026-08-05T00:00:00Z");
		mockState.manageDescriptors = [connector];
		mockState.createDescriptors = [connector];
		mockState.policies = [saved];
		mockState.listStorageCredentials.mockResolvedValue([initiallyAuthorized]);
		mockState.validateStorageCredential.mockResolvedValue({
			credential: validated,
			root_item_id: "root",
			root_item_name: "Drive",
		});

		mockState.policyId = "7";
		const view = render(<AdminPolicyEditPage />);
		await waitForCatalog("plugin.oauth");

		await waitFor(() => {
			expect(mockState.listStorageCredentials).toHaveBeenCalledTimes(1);
			expect(currentForm().storageCredentials).toEqual([initiallyAuthorized]);
		});

		await act(async () => currentForm().onValidateStorageCredential());
		await waitFor(() => {
			expect(mockState.validateStorageCredential).toHaveBeenCalledWith(7);
			expect(currentForm().storageCredentials).toEqual([validated]);
		});

		view.unmount();
		render(<AdminPolicyEditPage />);
		await waitFor(() => {
			expect(mockState.listStorageCredentials).toHaveBeenCalledTimes(2);
			expect(currentForm().storageCredentials).toEqual([initiallyAuthorized]);
		});
	});

	it("keeps a reopened credential session isolated from its stale list response", async () => {
		const connector = descriptor("plugin.oauth", {
			actions: [credentialValidationAction],
			authorization_provider: "plugin_oauth",
			credential_management: credentialManagement(),
			credential_mode: "oauth_delegated",
			requires_authorization: true,
			supports_initial_setup: false,
		});
		const first = deferred<StorageConnectorCredentialInfo[]>();
		const second = deferred<StorageConnectorCredentialInfo[]>();
		const authorized = credential("authorized", "2026-08-06T00:00:00Z");
		mockState.manageDescriptors = [connector];
		mockState.createDescriptors = [connector];
		mockState.policies = [policy("plugin.oauth")];
		mockState.listStorageCredentials
			.mockImplementationOnce(() => first.promise)
			.mockImplementationOnce(() => second.promise);

		mockState.policyId = "7";
		const view = render(<AdminPolicyEditPage />);
		await waitForCatalog("plugin.oauth");

		await waitFor(() =>
			expect(mockState.listStorageCredentials).toHaveBeenCalledTimes(1),
		);
		view.unmount();
		render(<AdminPolicyEditPage />);
		await waitFor(() =>
			expect(mockState.listStorageCredentials).toHaveBeenCalledTimes(2),
		);

		await act(async () => second.resolve([authorized]));
		await waitFor(() =>
			expect(currentForm().storageCredentials).toEqual([authorized]),
		);
		await act(async () => first.resolve([]));
		expect(currentForm().storageCredentials).toEqual([authorized]);
	});

	it("ignores validation completion from a closed credential session", async () => {
		const connector = descriptor("plugin.oauth", {
			actions: [credentialValidationAction],
			authorization_provider: "plugin_oauth",
			credential_management: credentialManagement(),
			credential_mode: "oauth_delegated",
			requires_authorization: true,
			supports_initial_setup: false,
		});
		const validation = deferred<{
			credential: StorageConnectorCredentialInfo;
			root_item_id: string;
			root_item_name: string | null;
		}>();
		const initial = credential("missing");
		const reopened = credential("authorized", "2026-08-06T00:00:00Z");
		const staleValidation = credential("expired", "2026-08-05T00:00:00Z");
		mockState.manageDescriptors = [connector];
		mockState.createDescriptors = [connector];
		mockState.policies = [policy("plugin.oauth")];
		mockState.listStorageCredentials
			.mockResolvedValueOnce([initial])
			.mockResolvedValueOnce([reopened]);
		mockState.validateStorageCredential.mockImplementation(
			() => validation.promise,
		);

		mockState.policyId = "7";
		const view = render(<AdminPolicyEditPage />);
		await waitForCatalog("plugin.oauth");

		await waitFor(() =>
			expect(currentForm().storageCredentials).toEqual([initial]),
		);
		await act(async () => currentForm().onValidateStorageCredential());
		await waitFor(() =>
			expect(currentForm().storageCredentialValidationSubmitting).toBe(true),
		);

		view.unmount();
		render(<AdminPolicyEditPage />);
		await waitFor(() =>
			expect(currentForm().storageCredentials).toEqual([reopened]),
		);
		await act(async () =>
			validation.resolve({
				credential: staleValidation,
				root_item_id: "stale-root",
				root_item_name: null,
			}),
		);

		expect(currentForm().storageCredentials).toEqual([reopened]);
		expect(currentForm().storageCredentialValidationSubmitting).toBe(false);
		expect(mockState.toastSuccess).not.toHaveBeenCalled();
	});

	it("reloads persisted credential status after validation fails", async () => {
		const connector = descriptor("plugin.oauth", {
			actions: [credentialValidationAction],
			authorization_provider: "plugin_oauth",
			credential_management: credentialManagement(),
			credential_mode: "oauth_delegated",
			requires_authorization: true,
			supports_initial_setup: false,
		});
		const initial = credential("authorized");
		const expired = credential("expired", "2026-08-07T00:00:00Z");
		const validationError = new Error("credential expired");
		mockState.manageDescriptors = [connector];
		mockState.createDescriptors = [connector];
		mockState.policies = [policy("plugin.oauth")];
		mockState.listStorageCredentials
			.mockResolvedValueOnce([initial])
			.mockResolvedValueOnce([expired]);
		mockState.validateStorageCredential.mockRejectedValue(validationError);

		mockState.policyId = "7";
		render(<AdminPolicyEditPage />);
		await waitForCatalog("plugin.oauth");

		await waitFor(() =>
			expect(currentForm().storageCredentials).toEqual([initial]),
		);
		await act(async () => currentForm().onValidateStorageCredential());

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(validationError);
			expect(mockState.listStorageCredentials).toHaveBeenCalledTimes(2);
			expect(currentForm().storageCredentials).toEqual([expired]);
			expect(currentForm().storageCredentialValidationSubmitting).toBe(false);
		});
	});

	it("executes connector-defined custom actions with their own field mapping", async () => {
		const repairAction = action({
			action_id: "plugin.repair_index",
			endpoints: ["execute_draft_storage_policy_action"],
			fields: [
				field("depth", {
					kind: "number",
					required: true,
					scope: "action_input",
				}),
			],
			kind: "custom",
			mutates_remote_state: true,
			output_fields: [
				{
					label_key: "plugin_request_id",
					name: "request_id",
					value_kind: "text",
				},
			],
			requires_confirmation: true,
		});
		const connector = descriptor("plugin.actions", {
			actions: [repairAction],
			fields: [field("path", { default_value: "data" })],
		});
		mockState.manageDescriptors = [connector];
		mockState.createDescriptors = [connector];
		mockState.executeDraftPolicyAction.mockResolvedValue({
			action_id: "plugin.repair_index",
			ok: true,
			output: { request_id: "draft-request-1", private_value: "ignored" },
		});

		render(<AdminPolicyEditPage />);
		await waitForCatalog("plugin.actions");
		await waitFor(() =>
			expect(currentForm().form.connector_id).toBe("plugin.actions"),
		);
		await act(async () =>
			currentForm().onConnectorActionValueChange(
				"plugin.repair_index",
				"depth",
				4,
			),
		);
		await act(async () =>
			currentForm().onRequestConnectorAction("plugin.repair_index"),
		);
		expect(currentForm().connectorActionConfirmId).toBe("plugin.repair_index");
		await act(async () =>
			currentForm().onConfirmConnectorAction("plugin.repair_index"),
		);

		await waitFor(() =>
			expect(mockState.executeDraftPolicyAction).toHaveBeenCalledTimes(1),
		);
		expect(mockState.executeDraftPolicyAction).toHaveBeenCalledWith({
			action_id: "plugin.repair_index",
			connection: expect.objectContaining({
				connector_config: expect.objectContaining({
					connector_id: "plugin.actions",
					values: { path: "data" },
				}),
			}),
			policy_id: undefined,
			values: { depth: 4 },
		});
		expect(mockState.toastSuccess).toHaveBeenLastCalledWith(
			"policy_connector_action_success",
			{ description: "plugin_request_id: draft-request-1" },
		);
	});

	it("presents saved action output and keeps missing output on the generic fallback", async () => {
		const inspectAction = action({
			action_id: "plugin.inspect_saved",
			endpoints: ["execute_saved_storage_policy_action"],
			kind: "custom",
			output_fields: [
				{
					label_key: "plugin_request_id",
					name: "request_id",
					value_kind: "text",
				},
			],
			requires_saved_policy: true,
		});
		const connector = descriptor("plugin.actions", {
			actions: [inspectAction],
		});
		mockState.manageDescriptors = [connector];
		mockState.createDescriptors = [connector];
		mockState.policies = [policy("plugin.actions")];
		mockState.executeSavedPolicyAction.mockResolvedValueOnce({
			action_id: "plugin.inspect_saved",
			ok: true,
			output: { request_id: "saved-request-1" },
		});

		mockState.policyId = "7";
		render(<AdminPolicyEditPage />);
		await waitForCatalog("plugin.actions");

		await waitFor(() => expect(currentForm().mode).toBe("edit"));
		await act(async () =>
			currentForm().onRequestConnectorAction("plugin.inspect_saved"),
		);

		await waitFor(() =>
			expect(mockState.executeSavedPolicyAction).toHaveBeenCalledWith(7, {
				action_id: "plugin.inspect_saved",
				values: {},
			}),
		);
		expect(mockState.toastSuccess).toHaveBeenLastCalledWith(
			"policy_connector_action_success",
			{ description: "plugin_request_id: saved-request-1" },
		);

		mockState.executeSavedPolicyAction.mockResolvedValueOnce({
			action_id: "plugin.inspect_saved",
			ok: true,
		});
		await act(async () =>
			currentForm().onRequestConnectorAction("plugin.inspect_saved"),
		);
		await waitFor(() =>
			expect(mockState.executeSavedPolicyAction).toHaveBeenCalledTimes(2),
		);
		expect(mockState.toastSuccess).toHaveBeenLastCalledWith(
			"policy_connector_action_success",
		);
	});

	it("loads action target options without mutating connector policy config", async () => {
		const lookupAction = action({
			action_id: "plugin.inspect_target",
			endpoints: ["execute_draft_storage_policy_action"],
			fields: [
				field("node", {
					kind: "select",
					scope: "action_input",
					select: {
						data_source: "remote_nodes",
						value_kind: "integer",
					},
				}),
				field("target", {
					kind: "select",
					scope: "action_input",
					select: {
						data_source: "remote_storage_targets",
						depends_on: "node",
						value_kind: "string",
					},
				}),
			],
			kind: "custom",
		});
		const connector = descriptor("plugin.actions", {
			actions: [lookupAction],
			fields: [field("path", { default_value: "policy-data" })],
		});
		mockState.manageDescriptors = [connector];
		mockState.createDescriptors = [connector];
		mockState.remoteNodes = [remoteNode(7, "Node seven")];
		mockState.listStorageTargets.mockResolvedValue([
			remoteTarget("action-target"),
		]);

		render(<AdminPolicyEditPage />);
		await waitForCatalog("plugin.actions");
		await waitFor(() =>
			expect(currentForm().form.connector_config_values).toEqual({
				path: "policy-data",
			}),
		);
		await act(async () =>
			currentForm().onConnectorActionValueChange(
				"plugin.inspect_target",
				"node",
				7,
			),
		);

		await waitFor(() =>
			expect(mockState.listStorageTargets).toHaveBeenCalledWith(7),
		);
		await waitFor(() =>
			expect(currentForm().remoteStorageTargets).toEqual([
				remoteTarget("action-target"),
			]),
		);
		expect(currentForm().form.connector_config_values).toEqual({
			path: "policy-data",
		});
	});

	it("keeps a newly created authorization connector open with connector-owned guidance", async () => {
		const connector = descriptor("plugin.oauth", {
			actions: [authorizationAction],
			authorization_provider: "plugin_oauth",
			credential_management: credentialManagement(),
			credential_mode: "oauth_delegated",
			requires_authorization: true,
			supports_initial_setup: false,
		});
		const created = policy("plugin.oauth", {}, { name: "OAuth policy" });
		mockState.manageDescriptors = [connector];
		mockState.createDescriptors = [connector];
		mockState.create.mockResolvedValue(created);

		render(<AdminPolicyEditPage />);
		await waitForCatalog("plugin.oauth");
		await setField("name", "OAuth policy");
		await act(async () => currentForm().onCreateStepChange(2));
		await submitEditorForm();

		await waitFor(() => expect(mockState.create).toHaveBeenCalledTimes(1));
		expect(mockState.navigate).toHaveBeenCalledWith("/admin/policies/7", {
			replace: true,
			viewTransition: false,
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"plugin_created_authorize_next",
		);
	});

	it("offers save-anyway after a draft connection test fails", async () => {
		const connector = descriptor("plugin.failing-test", {
			actions: [draftTestAction],
			fields: [field("path", { required: true })],
		});
		mockState.manageDescriptors = [connector];
		mockState.createDescriptors = [connector];
		mockState.testParams.mockRejectedValue(new Error("connection failed"));
		mockState.create.mockResolvedValue(
			policy("plugin.failing-test", { path: "data" }),
		);

		render(<AdminPolicyEditPage />);
		await waitForCatalog("plugin.failing-test");
		await setField("name", "Fallback Policy");
		await setField("connector_config_values", { path: "data" });
		await act(async () => currentForm().onCreateStepChange(2));
		await submitEditorForm();

		await waitFor(() => expect(currentForm().saveAnywayConfirmOpen).toBe(true));
		expect(mockState.create).not.toHaveBeenCalled();
		await act(async () => currentForm().onConfirmSaveAnyway());
		await waitFor(() => expect(mockState.create).toHaveBeenCalledTimes(1));
	});

	it("forces setup policies to default and refreshes setup state", async () => {
		const connector = descriptor("plugin.setup", {
			fields: [field("path", { default_value: "uploads", required: true })],
		});
		mockState.setupDescriptors = [connector];
		mockState.create.mockResolvedValue(
			policy("plugin.setup", { path: "uploads" }, { is_default: true }),
		);

		render(<AdminPoliciesPage variant="setup" />);
		await waitForCatalog("plugin.setup");
		await waitFor(() =>
			expect(currentForm().form.connector_id).toBe("plugin.setup"),
		);
		expect(currentForm().forceDefaultPolicy).toBe(true);
		await setField("name", "Setup Policy");
		await setField("is_default", false);
		await act(async () => currentForm().onCreateStepChange(2));
		await submitEditorForm();

		await waitFor(() => expect(mockState.create).toHaveBeenCalledTimes(1));
		expect(mockState.create.mock.calls[0]?.[0]).toMatchObject({
			is_default: true,
			name: "Setup Policy",
		});
		expect(mockState.setupRefresh).toHaveBeenCalledTimes(1);
	});
});
