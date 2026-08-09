import {
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import type { ComponentProps } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
	StorageConnectorActionDescriptor,
	StorageConnectorCredentialInfo,
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
	StoragePolicyCapacityInfo,
} from "@/types/api";
import { StoragePolicyDialog } from "./StoragePolicyDialog";
import {
	emptyForm,
	type PolicyFormData,
} from "./storage-policy-dialog/formTypes";

const connectorMessages = vi.hoisted(() => new Map<string, string>());
const interactionMocks = vi.hoisted(() => ({
	clipboard: vi.fn(),
	toastError: vi.fn(),
	toastSuccess: vi.fn(),
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, values?: Record<string, number | string>) => {
			const namespace = typeof values?.ns === "string" ? values.ns : null;
			if (namespace) {
				const translated = connectorMessages.get(`${namespace}:${key}`);
				if (translated) return translated;
			}
			if (typeof values?.defaultValue === "string") return values.defaultValue;
			return values
				? `${key}:${Object.values(values).map(String).join(":")}`
				: key;
		},
	}),
}));

vi.mock("sonner", () => ({
	toast: {
		error: (...args: unknown[]) => interactionMocks.toastError(...args),
		success: (...args: unknown[]) => interactionMocks.toastSuccess(...args),
	},
}));

vi.mock("@/lib/clipboard", () => ({
	writeTextToClipboard: (...args: unknown[]) =>
		interactionMocks.clipboard(...args),
}));

vi.mock(
	"@/components/admin/admin-remote-nodes-page/RemoteNodeRemoteStorageTargetSection",
	() => ({
		RemoteNodeRemoteStorageTargetSection: (props: {
			errorMessage?: string | null;
			loading?: boolean;
			onCreateTarget?: () => void;
		}) => (
			<div data-testid="remote-targets">
				<span>{props.errorMessage}</span>
				<span>{String(props.loading)}</span>
				<button type="button" onClick={props.onCreateTarget}>
					create-target
				</button>
			</div>
		),
	}),
);

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

function descriptor(
	connectorId: string,
	overrides: Partial<StorageConnectorDescriptor> = {},
): StorageConnectorDescriptor {
	return {
		actions: [
			action({
				action_id: "test_draft_connection",
				kind: "connection_test",
				endpoints: ["test_policy_params"],
			}),
		],
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
		credential_mode: "static_secret",
		deployment_scope: "shared_across_primary_instances",
		description: `${connectorId} description`,
		fields: [
			field("base_path", {
				default_mode: "missing_or_empty_text",
				default_value: "./data/uploads",
			}),
			field("plugin_endpoint", { required: true }),
			field("plugin_token", {
				kind: "secret",
				scope: "static_credential",
				secret: true,
			}),
		],
		label: connectorId,
		related_issues: [],
		requires_authorization: false,
		supports_initial_setup: true,
		ui: {
			badge_rgb: { red: 16, green: 185, blue: 129 },
			base_path_empty_display: "plugin_root",
			base_path_placeholder: "plugin_path_placeholder",
			config_step_description_key: "plugin_config_desc",
			config_step_title_key: "plugin_config_title",
			description_key: "plugin_description",
			edit_context_key: "plugin_edit_context",
			helper_key: "plugin_helper",
			icon_name: null,
			icon_src: "/plugin-icon.png",
			label_key: "plugin_label",
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

function policyForm(overrides: Partial<PolicyFormData> = {}): PolicyFormData {
	return {
		...emptyForm,
		name: "Plugin policy",
		connector_id: "plugin.example",
		connector_config_values: {
			base_path: "plugin-root",
			plugin_endpoint: "https://plugin.example.test",
		},
		credential_values: { plugin_token: "SECRET_TOKEN" },
		...overrides,
	};
}

function policyCapacity(
	overrides: Partial<StoragePolicyCapacityInfo> = {},
): StoragePolicyCapacityInfo {
	return {
		blob_count: 3,
		blob_total_bytes: 300,
		capacity: {
			available_bytes: 600,
			observed_at: "2026-08-08T00:00:00Z",
			source: "local_filesystem",
			status: "supported",
			total_bytes: 1000,
			used_bytes: 400,
		},
		connector_id: "plugin.example",
		policy_id: 1,
		...overrides,
	};
}

function dialogProps(
	overrides: Partial<ComponentProps<typeof StoragePolicyDialog>> = {},
): ComponentProps<typeof StoragePolicyDialog> {
	const plugin = descriptor("plugin.example");
	return {
		open: true,
		mode: "create",
		form: policyForm(),
		storageDriverDescriptor: plugin,
		storageDriverDescriptors: [plugin],
		storageDriverDescriptorsError: null,
		storageDriverDescriptorsLoading: false,
		policyCapacity: null,
		policyCapacityLoading: false,
		storageCredentials: [],
		storageCredentialsLoading: false,
		storageAuthorizationSubmitting: false,
		storageCredentialValidationSubmitting: false,
		storageAuthorizationRedirectUri: "https://app.example.test/callback",
		remoteNodes: [],
		remoteStorageTargetDriverDescriptors: [],
		remoteStorageTargetDriverDescriptorsError: null,
		remoteStorageTargetDriverDescriptorsLoading: false,
		remoteStorageTargets: [],
		remoteStorageTargetsError: null,
		remoteStorageTargetsLoading: false,
		submitting: false,
		createStep: 0,
		createStepTouched: false,
		endpointValidationMessage: null,
		connectorActionConfirmId: null,
		connectorActionSubmittingId: null,
		connectorActionValues: {},
		saveAnywayConfirmOpen: false,
		onCancelConnectorAction: vi.fn(),
		onOpenChange: vi.fn(),
		onCancelSaveAnyway: vi.fn(),
		onConfirmSaveAnyway: vi.fn(),
		onConfirmConnectorAction: vi.fn(),
		onStartStorageAuthorization: vi.fn(),
		onValidateStorageCredential: vi.fn(),
		onCreateRemoteStorageTarget: vi.fn(async () => undefined),
		onSubmit: vi.fn(),
		onRunConnectionTest: vi.fn(async () => true),
		onFieldChange: vi.fn(),
		onConnectorActionValueChange: vi.fn(),
		onRequestConnectorAction: vi.fn(),
		onConnectorIdChange: vi.fn(),
		onCreateBack: vi.fn(),
		onCreateStepChange: vi.fn(),
		onCreateNext: vi.fn(),
		...overrides,
	};
}

describe("StoragePolicyDialog", () => {
	beforeEach(() => {
		connectorMessages.clear();
		interactionMocks.clipboard.mockReset();
		interactionMocks.clipboard.mockResolvedValue(undefined);
		interactionMocks.toastError.mockReset();
		interactionMocks.toastSuccess.mockReset();
	});

	it("keeps the previous two-column connector selection and advances directly from a descriptor card", () => {
		const available = descriptor("plugin.example", {
			ui: {
				...descriptor("plugin.example").ui,
				icon_name: "not-an-icon",
				icon_src: null,
			},
		});
		const postSetup = descriptor("plugin.post-setup", {
			supports_initial_setup: false,
			ui: {
				...descriptor("plugin.post-setup").ui,
				label_key: "post_setup_label",
			},
		});
		const props = dialogProps({
			form: policyForm({ connector_id: "" }),
			presentation: "setup",
			storageDriverDescriptor: null,
			storageDriverDescriptors: [available, postSetup],
		});
		render(<StoragePolicyDialog {...props} />);

		const options = screen.getByTestId("storage-driver-options");
		expect(options).toHaveClass("md:grid-cols-2");
		expect(
			document.querySelector('img[src="/plugin-icon.png"]'),
		).not.toBeNull();
		expect(
			screen.getByRole("button", { name: /post_setup_label/ }),
		).toBeDisabled();
		expect(
			screen.queryByRole("button", { name: "policy_wizard_next" }),
		).toBeNull();

		fireEvent.click(screen.getByRole("button", { name: /plugin_label/ }));
		expect(props.onConnectorIdChange).toHaveBeenCalledWith("plugin.example");
		expect(props.onCreateStepChange).toHaveBeenCalledWith(1);
	});

	it("keeps the configuration helper sidebar and descriptor-driven connection controls", () => {
		const props = dialogProps({ createStep: 1 });
		render(<StoragePolicyDialog {...props} />);

		expect(screen.getByText("plugin_helper")).toBeVisible();
		expect(screen.getByLabelText("core:name")).toHaveValue("Plugin policy");
		expect(screen.getByLabelText("core:name")).toHaveAttribute("id", "name");
		expect(screen.getByLabelText("base_path")).toHaveValue("plugin-root");
		expect(screen.getByLabelText("base_path")).toHaveAttribute(
			"id",
			"base_path",
		);
		expect(screen.getByLabelText("plugin_endpoint")).toHaveValue(
			"https://plugin.example.test",
		);
		expect(screen.getByLabelText("plugin_token")).toHaveAttribute(
			"type",
			"password",
		);
		expect(
			screen.getByRole("button", { name: "test_connection" }),
		).toBeVisible();
		expect(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		).toBeVisible();
	});

	it("keeps the review summary while excluding connector-owned secrets", () => {
		const props = dialogProps({ createStep: 2 });
		render(<StoragePolicyDialog {...props} />);

		const summary = screen.getByTestId("policy-summary-card");
		expect(within(summary).getByText("Plugin policy")).toBeVisible();
		expect(
			within(summary).getByText("https://plugin.example.test"),
		).toBeVisible();
		expect(within(summary).queryByText("SECRET_TOKEN")).toBeNull();
		expect(screen.getByRole("button", { name: "core:create" })).toBeVisible();
	});

	it("falls back to the persisted id for an unavailable remote node", () => {
		const remotePlugin = descriptor("plugin.example", {
			fields: [
				field("remote_node_id", {
					kind: "select",
					select: { data_source: "remote_nodes", value_kind: "integer" },
				}),
			],
		});

		const view = render(
			<StoragePolicyDialog
				{...dialogProps({
					createStep: 2,
					form: policyForm({
						connector_config_values: { remote_node_id: 404 },
					}),
					remoteNodes: [],
					storageDriverDescriptor: remotePlugin,
					storageDriverDescriptors: [remotePlugin],
				})}
			/>,
		);

		expect(
			within(screen.getByTestId("policy-summary-card")).getByText("404"),
		).toBeVisible();

		view.rerender(
			<StoragePolicyDialog
				{...dialogProps({
					createStep: 2,
					form: policyForm({ connector_config_values: {} }),
					remoteNodes: [],
					storageDriverDescriptor: remotePlugin,
					storageDriverDescriptors: [remotePlugin],
				})}
			/>,
		);
		expect(
			within(screen.getByTestId("policy-summary-card")).getByText("—"),
		).toBeVisible();
	});

	it("restores the edit context, capacity summary, and separate overview, connection, and rules sections", () => {
		const props = dialogProps({ mode: "edit" });
		render(<StoragePolicyDialog {...props} />);

		expect(screen.getByTestId("policy-edit-shell")).toBeVisible();
		expect(screen.getByTestId("policy-edit-context-bar")).toBeVisible();
		expect(screen.getByTestId("policy-edit-driver-badge")).toHaveTextContent(
			"plugin_label",
		);
		expect(screen.getByTestId("policy-edit-driver-badge")).toHaveClass(
			"border-[var(--storage-connector-badge-border)]",
		);
		expect(screen.getByTestId("policy-edit-driver-badge")).toHaveStyle(
			"--storage-connector-badge-border: rgb(16 185 129 / 0.55)",
		);
		expect(screen.getByTestId("policy-edit-capacity-summary")).toBeVisible();
		expect(screen.getByText("plugin_edit_context")).toBeVisible();
		expect(screen.getByText("policy_editor_overview_title")).toBeVisible();
		expect(screen.getByText("plugin_config_title")).toBeVisible();
		expect(screen.getByText("policy_editor_rules_title")).toBeVisible();
		expect(screen.getByLabelText("base_path")).toHaveValue("plugin-root");
		expect(screen.getByLabelText("plugin_endpoint")).toHaveValue(
			"https://plugin.example.test",
		);
		expect(screen.getByLabelText("plugin_token")).toHaveAttribute(
			"placeholder",
			"policy_editor_credentials_keep_placeholder",
		);
		expect(screen.getByRole("button", { name: "save_changes" })).toBeVisible();
	});

	it("renders schema-valid supported capacity values and an accessible segmented progress bar", () => {
		render(
			<StoragePolicyDialog
				{...dialogProps({
					mode: "edit",
					policyCapacity: policyCapacity(),
				})}
			/>,
		);

		const summary = screen.getByTestId("policy-edit-capacity-summary");
		expect(
			within(summary).getByText("policy_capacity_status_supported"),
		).toBeVisible();
		expect(
			within(summary).getByTestId("policy-capacity-blob-used"),
		).toHaveTextContent("300.0 B");
		expect(
			within(summary).getByTestId("policy-capacity-system-used"),
		).toHaveTextContent("400.0 B");
		expect(
			within(summary).getByTestId("policy-capacity-available"),
		).toHaveTextContent("600.0 B");
		expect(
			within(summary).getByTestId("policy-capacity-total"),
		).toHaveTextContent("1000.0 B");

		const progress = within(summary).getByRole("progressbar", {
			name: "policy_capacity_occupied_progress",
		});
		expect(progress).toHaveAttribute("aria-valuemin", "0");
		expect(progress).toHaveAttribute("aria-valuemax", "100");
		expect(progress).toHaveAttribute("aria-valuenow", "40");
		expect(progress).toHaveAttribute(
			"aria-valuetext",
			"policy_capacity_occupied_value:40:1000.0 B:400.0 B",
		);
		expect(
			within(summary).getByTestId("policy-capacity-other-segment"),
		).toHaveStyle({ width: "10%" });
		expect(
			within(summary).getByTestId("policy-capacity-blob-segment"),
		).toHaveStyle({ width: "30%" });
		expect(
			within(summary).getByText("policy_capacity_other_system_used"),
		).toBeVisible();
	});

	it("clamps inconsistent provider capacity before formatting and sizing segments", () => {
		render(
			<StoragePolicyDialog
				{...dialogProps({
					mode: "edit",
					policyCapacity: policyCapacity({
						blob_total_bytes: 2000,
						capacity: {
							available_bytes: -100,
							observed_at: "2026-08-08T00:00:00Z",
							source: "inconsistent_provider",
							status: "supported",
							total_bytes: 1000,
							used_bytes: 1500,
						},
					}),
				})}
			/>,
		);

		const summary = screen.getByTestId("policy-edit-capacity-summary");
		expect(
			within(summary).getByTestId("policy-capacity-system-used"),
		).toHaveTextContent("1000.0 B");
		expect(
			within(summary).getByTestId("policy-capacity-available"),
		).toHaveTextContent("0 B");
		expect(within(summary).getByRole("progressbar")).toHaveAttribute(
			"aria-valuenow",
			"100",
		);
		expect(
			within(summary).getByTestId("policy-capacity-other-segment"),
		).toHaveStyle({ width: "0%" });
		expect(
			within(summary).getByTestId("policy-capacity-blob-segment"),
		).toHaveStyle({ width: "100%" });
	});

	it("renders zero-total metrics without dividing or exposing progress semantics", () => {
		render(
			<StoragePolicyDialog
				{...dialogProps({
					mode: "edit",
					policyCapacity: policyCapacity({
						capacity: {
							available_bytes: 10,
							observed_at: "2026-08-08T00:00:00Z",
							source: "zero_total_provider",
							status: "supported",
							total_bytes: 0,
							used_bytes: 100,
						},
					}),
				})}
			/>,
		);

		const summary = screen.getByTestId("policy-edit-capacity-summary");
		expect(
			within(summary).getByTestId("policy-capacity-system-used"),
		).toHaveTextContent("0 B");
		expect(
			within(summary).getByTestId("policy-capacity-available"),
		).toHaveTextContent("0 B");
		expect(within(summary).queryByRole("progressbar")).toBeNull();
		expect(
			within(summary).getByText("policy_capacity_zero_total_desc"),
		).toBeVisible();
	});

	it("normalizes a negative total to the zero-total presentation", () => {
		render(
			<StoragePolicyDialog
				{...dialogProps({
					mode: "edit",
					policyCapacity: policyCapacity({
						capacity: {
							available_bytes: 10,
							observed_at: "2026-08-08T00:00:00Z",
							source: "negative_total_provider",
							status: "supported",
							total_bytes: -1,
							used_bytes: 100,
						},
					}),
				})}
			/>,
		);

		const summary = screen.getByTestId("policy-edit-capacity-summary");
		expect(
			within(summary).getByTestId("policy-capacity-system-used"),
		).toHaveTextContent("0 B");
		expect(
			within(summary).getByTestId("policy-capacity-available"),
		).toHaveTextContent("0 B");
		expect(
			within(summary).getByTestId("policy-capacity-total"),
		).toHaveTextContent("0 B");
		expect(within(summary).queryByRole("progressbar")).toBeNull();
		expect(
			within(summary).getByText("policy_capacity_zero_total_desc"),
		).toBeVisible();
	});

	it("keeps loading, unsupported, unavailable, and null-field capacity fallbacks explicit", () => {
		const view = render(
			<StoragePolicyDialog
				{...dialogProps({
					mode: "edit",
					policyCapacity: policyCapacity(),
					policyCapacityLoading: true,
				})}
			/>,
		);
		let summary = screen.getByTestId("policy-edit-capacity-summary");
		expect(within(summary).getByText("policy_capacity_loading")).toBeVisible();
		expect(within(summary).queryByRole("progressbar")).toBeNull();
		expect(
			within(summary).queryByTestId("policy-capacity-system-used"),
		).toBeNull();

		view.rerender(
			<StoragePolicyDialog
				{...dialogProps({
					mode: "edit",
					policyCapacity: policyCapacity({
						capacity: {
							available_bytes: null,
							observed_at: "2026-08-08T00:00:00Z",
							source: "unsupported_provider",
							status: "unsupported",
							total_bytes: null,
							used_bytes: null,
						},
					}),
				})}
			/>,
		);
		summary = screen.getByTestId("policy-edit-capacity-summary");
		expect(
			within(summary).getByText("policy_capacity_unsupported_desc"),
		).toBeVisible();
		expect(within(summary).queryByRole("progressbar")).toBeNull();

		view.rerender(
			<StoragePolicyDialog
				{...dialogProps({
					mode: "edit",
					policyCapacity: policyCapacity({
						capacity: {
							available_bytes: null,
							observed_at: "2026-08-08T00:00:00Z",
							source: "temporarily_unavailable",
							status: "unavailable",
							total_bytes: null,
							used_bytes: null,
						},
					}),
				})}
			/>,
		);
		summary = screen.getByTestId("policy-edit-capacity-summary");
		expect(
			within(summary).getByText("policy_capacity_unavailable_desc"),
		).toBeVisible();

		view.rerender(
			<StoragePolicyDialog
				{...dialogProps({
					mode: "edit",
					policyCapacity: policyCapacity({
						capacity: {
							available_bytes: null,
							observed_at: "2026-08-08T00:00:00Z",
							source: "partial_provider",
							status: "supported",
							total_bytes: 1000,
							used_bytes: 400,
						},
					}),
				})}
			/>,
		);
		summary = screen.getByTestId("policy-edit-capacity-summary");
		expect(
			within(summary).getByText("policy_capacity_unavailable_desc"),
		).toBeVisible();
		expect(within(summary).queryByRole("progressbar")).toBeNull();
		expect(within(summary).queryByTestId("policy-capacity-total")).toBeNull();

		view.rerender(
			<StoragePolicyDialog
				{...dialogProps({ mode: "edit", policyCapacity: null })}
			/>,
		);
		summary = screen.getByTestId("policy-edit-capacity-summary");
		expect(
			within(summary).getByText("policy_capacity_status_unavailable"),
		).toBeVisible();
		expect(
			within(summary).getByText("policy_capacity_unavailable_desc"),
		).toBeVisible();
	});

	it("renders the full connector-owned credential lifecycle and copy feedback", async () => {
		for (const [key, value] of Object.entries({
			plugin_authorized_at: "Authorized 2026-08-01",
			plugin_copy_redirect_uri: "Copy redirect URI",
			plugin_credential_desc_authorized: "Credential is saved",
			plugin_credential_loading: "Credential loading",
			plugin_credential_status_authorized: "Credential authorized",
			plugin_credential_status_missing: "Credential missing",
			plugin_credential_status_reauth: "Reauthorization required",
			plugin_credential_title: "Connector credential",
			plugin_reauth_desc: "Authorize again after checking the app",
			plugin_reauth_reason: "Application credentials were rejected",
			plugin_reauth_title: "Authorization expired",
			plugin_reauthorize: "Reauthorize connector",
			plugin_redirect_uri: "Connector redirect URI",
			plugin_redirect_uri_help: "Register this redirect URI",
			plugin_refreshed_at: "Refreshed 2026-08-02",
			plugin_validated_at: "Validated 2026-08-03",
			policy_connector_start_authorization: "Authorize connector",
			policy_connector_validate_credential: "Validate connector",
		})) {
			connectorMessages.set(`plugin.example:${key}`, value);
		}
		const plugin = descriptor("plugin.example", {
			credential_management: {
				authorization_started_key: "plugin_authorization_started",
				authorized_at_key: "plugin_authorized_at",
				created_authorize_next_key: "plugin_created_authorize_next",
				loading_key: "plugin_credential_loading",
				reauthorize_action_key: "plugin_reauthorize",
				redirect_uri_copy_key: "plugin_copy_redirect_uri",
				redirect_uri_help_key: "plugin_redirect_uri_help",
				redirect_uri_key: "plugin_redirect_uri",
				refreshed_at_key: "plugin_refreshed_at",
				save_before_authorize_key: "plugin_save_before_authorize",
				save_before_validate_key: "plugin_save_before_validate",
				status_presentations: {
					authorized: {
						description_key: "plugin_credential_desc_authorized",
						label_key: "plugin_credential_status_authorized",
						tone: "success",
					},
					missing: {
						label_key: "plugin_credential_status_missing",
						tone: "neutral",
					},
					reauth_required: {
						attention_guidance_key: "plugin_reauth_desc",
						attention_title_key: "plugin_reauth_title",
						label_key: "plugin_credential_status_reauth",
						reason_fallback_key: "plugin_reauth_reason",
						reason_rules: [
							{
								contains_any: ["invalid_client"],
								message_key: "plugin_reauth_reason",
							},
						],
						tone: "warning",
					},
				},
				title_key: "plugin_credential_title",
				validated_at_key: "plugin_validated_at",
				validation_success_detail_key: "plugin_validation_success_detail",
				validation_success_key: "plugin_validation_success",
			},
			actions: [
				action({
					action_id: "start_authorization",
					kind: "authorization",
					label_key: "policy_connector_start_authorization",
				}),
				action({
					action_id: "validate_credential",
					kind: "credential_validation",
					label_key: "policy_connector_validate_credential",
				}),
			],
		});
		const credential = {
			account_label: "Admin account",
			authorized_at: "2026-08-01T00:00:00Z",
			created_at: "2026-08-04T00:00:00Z",
			credential_kind: "authorization",
			id: 1,
			last_refreshed_at: "2026-08-02T00:00:00Z",
			last_validated_at: "2026-08-03T00:00:00Z",
			policy_id: 7,
			provider: "microsoft_graph",
			scopes: [],
			status: "authorized",
			updated_at: "2026-08-04T00:00:00Z",
		} satisfies StorageConnectorCredentialInfo;

		const view = render(
			<StoragePolicyDialog
				{...dialogProps({
					mode: "edit",
					storageDriverDescriptor: plugin,
					storageDriverDescriptors: [plugin],
					storageCredentials: [credential],
				})}
			/>,
		);

		expect(screen.getByText("Connector credential")).toBeVisible();
		expect(screen.getByText("Credential authorized")).toBeVisible();
		expect(screen.getByText("Credential is saved")).toBeVisible();
		expect(screen.getByText("Admin account")).toBeVisible();
		expect(
			screen.getByText(
				/Authorized 2026-08-01 · Refreshed 2026-08-02 · Validated 2026-08-03/,
			),
		).toBeVisible();
		expect(screen.getByText("Connector redirect URI")).toBeVisible();
		expect(screen.getByText("Register this redirect URI")).toBeVisible();
		expect(
			screen.getByRole("button", { name: "Reauthorize connector" }),
		).toBeVisible();
		expect(
			screen.getByRole("button", { name: "Validate connector" }),
		).toBeVisible();
		fireEvent.click(screen.getByRole("button", { name: "Copy redirect URI" }));
		await waitFor(() =>
			expect(interactionMocks.clipboard).toHaveBeenCalledWith(
				"https://app.example.test/callback",
			),
		);
		expect(interactionMocks.toastSuccess).toHaveBeenCalledWith(
			"core:copied_to_clipboard",
		);

		interactionMocks.clipboard.mockRejectedValueOnce(
			new Error("clipboard denied"),
		);
		fireEvent.click(screen.getByRole("button", { name: "Copy redirect URI" }));
		await waitFor(() =>
			expect(interactionMocks.toastError).toHaveBeenCalledWith(
				"clipboard denied",
			),
		);

		view.rerender(
			<StoragePolicyDialog
				{...dialogProps({
					mode: "edit",
					storageDriverDescriptor: plugin,
					storageDriverDescriptors: [plugin],
					storageCredentials: [
						{
							...credential,
							status: "reauth_required",
							status_reason: "INVALID_CLIENT: provider detail",
						},
					],
				})}
			/>,
		);
		expect(screen.getByText("Reauthorization required")).toBeVisible();
		expect(screen.getByText("Authorization expired")).toBeVisible();
		expect(
			screen.getByText("Application credentials were rejected"),
		).toBeVisible();
		expect(
			screen.getByText("Authorize again after checking the app"),
		).toBeVisible();
		expect(screen.queryByText("INVALID_CLIENT: provider detail")).toBeNull();

		view.rerender(
			<StoragePolicyDialog
				{...dialogProps({
					mode: "edit",
					storageDriverDescriptor: plugin,
					storageDriverDescriptors: [plugin],
					storageCredentials: [],
				})}
			/>,
		);
		expect(screen.getByText("Credential missing")).toBeVisible();
		expect(
			screen.getByRole("button", { name: "Authorize connector" }),
		).toBeVisible();
	});

	it("covers connector loading and error states before selection", () => {
		const loading = render(
			<StoragePolicyDialog
				{...dialogProps({
					form: policyForm({ connector_id: "" }),
					storageDriverDescriptor: null,
					storageDriverDescriptors: [],
					storageDriverDescriptorsLoading: true,
				})}
			/>,
		);
		expect(screen.getByText("core:loading")).toBeVisible();
		loading.unmount();

		render(
			<StoragePolicyDialog
				{...dialogProps({
					form: policyForm({ connector_id: "" }),
					storageDriverDescriptor: null,
					storageDriverDescriptors: [],
					storageDriverDescriptorsError: "catalog failed",
				})}
			/>,
		);
		expect(screen.getByText("catalog failed")).toBeVisible();
	});

	it("handles wizard navigation, validation, rules, confirmation, and submission callbacks", () => {
		const plugin = descriptor("plugin.example", {
			capabilities: {
				...descriptor("plugin.example").capabilities,
				storage_native_media_metadata: true,
				storage_native_thumbnail: true,
			},
		});
		const props = dialogProps({
			createStep: 1,
			createStepTouched: true,
			endpointValidationMessage: "endpoint invalid",
			form: policyForm({
				name: "",
				connector_config_values: {
					base_path: "plugin-root",
					plugin_endpoint: "https://plugin.example.test",
				},
				storage_native_media_metadata_enabled: true,
				storage_native_media_metadata_extensions: ["mp4"],
				storage_native_thumbnail_extensions: ["jpg"],
				storage_native_thumbnail_enabled: true,
			}),
			saveAnywayConfirmOpen: true,
			storageDriverDescriptor: plugin,
			storageDriverDescriptors: [plugin],
		});
		const view = render(<StoragePolicyDialog {...props} />);

		const formElement = screen.getByTestId("policy-step-panel").closest("form");
		expect(formElement).not.toBeNull();
		if (formElement) fireEvent.submit(formElement);
		expect(screen.getByText("policy_wizard_name_required")).toBeVisible();
		expect(screen.getByText("endpoint invalid")).toBeVisible();
		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Renamed" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "policy_wizard_review" }),
		);
		fireEvent.click(screen.getByRole("button", { name: "core:back" }));
		fireEvent.click(screen.getByRole("button", { name: "core:cancel" }));
		fireEvent.click(screen.getByRole("button", { name: "save_anyway" }));
		expect(props.onFieldChange).toHaveBeenCalledWith("name", "Renamed");
		expect(props.onCreateNext).toHaveBeenCalledOnce();
		expect(props.onCreateBack).toHaveBeenCalledOnce();
		expect(props.onCancelSaveAnyway).toHaveBeenCalledOnce();
		expect(props.onConfirmSaveAnyway).toHaveBeenCalledOnce();

		view.rerender(
			<StoragePolicyDialog
				{...props}
				createStep={2}
				form={policyForm({
					connector_config_values: {
						base_path: "plugin-root",
						plugin_endpoint: "https://plugin.example.test",
					},
					storage_native_media_metadata_enabled: true,
					storage_native_media_metadata_extensions: ["mp4"],
					storage_native_thumbnail_extensions: ["jpg"],
					storage_native_thumbnail_enabled: true,
				})}
			/>,
		);
		fireEvent.click(
			screen.getByRole("button", { name: /policy_wizard_step_storage_title/ }),
		);
		fireEvent.change(screen.getByLabelText("max_file_size"), {
			target: { value: "2048" },
		});
		fireEvent.change(screen.getByLabelText("chunk_size"), {
			target: { value: "16" },
		});
		fireEvent.click(screen.getByRole("switch", { name: "set_as_default" }));
		fireEvent.change(
			screen.getByLabelText("storage_native_thumbnail_extensions"),
			{
				target: { value: "jpg, webp" },
			},
		);
		fireEvent.change(
			screen.getByLabelText("storage_native_media_metadata_extensions"),
			{ target: { value: "mp4, mov" } },
		);
		fireEvent.click(screen.getByRole("button", { name: "core:create" }));
		expect(props.onFieldChange).toHaveBeenCalledWith("max_file_size", "2048");
		expect(props.onFieldChange).toHaveBeenCalledWith("chunk_size", "16");
		expect(props.onFieldChange).toHaveBeenCalledWith("is_default", true);
		expect(props.onFieldChange).toHaveBeenCalledWith(
			"storage_native_thumbnail_extensions",
			["jpg", "webp"],
		);
		expect(props.onFieldChange).toHaveBeenCalledWith(
			"storage_native_media_metadata_extensions",
			["mp4", "mov"],
		);
		expect(props.onSubmit).toHaveBeenCalledOnce();
	});

	it("renders independent core behavior controls strictly from descriptor capabilities", () => {
		const baseCapabilities = descriptor("plugin.example").capabilities;
		const cases = [
			{ thumbnail: false, metadata: false },
			{ thumbnail: true, metadata: false },
			{ thumbnail: false, metadata: true },
			{ thumbnail: true, metadata: true },
		];

		for (const item of cases) {
			const plugin = descriptor("plugin.example", {
				capabilities: {
					...baseCapabilities,
					storage_native_thumbnail: item.thumbnail,
					storage_native_media_metadata: item.metadata,
				},
			});
			const props = dialogProps({
				createStep: 2,
				form: policyForm({
					storage_native_thumbnail_enabled: false,
					storage_native_thumbnail_extensions: ["jpg"],
					storage_native_media_metadata_enabled: false,
					storage_native_media_metadata_extensions: ["mp4"],
				}),
				storageDriverDescriptor: plugin,
				storageDriverDescriptors: [plugin],
			});
			const view = render(<StoragePolicyDialog {...props} />);

			expect(
				screen.queryByRole("switch", {
					name: "storage_native_thumbnail_enabled",
				}) !== null,
			).toBe(item.thumbnail);
			if (item.thumbnail) {
				expect(
					screen.getByLabelText("storage_native_thumbnail_extensions"),
				).toHaveValue("jpg");
			}
			expect(
				screen.queryByLabelText("storage_native_thumbnail_extensions") !== null,
			).toBe(item.thumbnail);
			expect(
				screen.queryByRole("switch", {
					name: "storage_native_media_metadata_enabled",
				}) !== null,
			).toBe(item.metadata);
			expect(
				screen.queryByLabelText("storage_native_media_metadata_extensions") !==
					null,
			).toBe(item.metadata);
			if (item.metadata) {
				expect(
					screen.getByLabelText("storage_native_media_metadata_extensions"),
				).toHaveValue("mp4");
			}

			view.unmount();
		}
	});

	it("keeps dormant extension controls visible, ordered, described, and editable", () => {
		connectorMessages.set(
			"plugin.example:storage_native_thumbnail_enabled_desc",
			"Plugin image thumbnail billing help",
		);
		connectorMessages.set(
			"plugin.example:storage_native_thumbnail_extensions_desc",
			"Plugin image extension matching help",
		);
		connectorMessages.set(
			"plugin.example:storage_native_media_metadata_enabled_desc",
			"Plugin audio and video billing help",
		);
		connectorMessages.set(
			"plugin.example:storage_native_media_metadata_extensions_desc",
			"Plugin audio and video extension matching help",
		);
		const plugin = descriptor("plugin.example", {
			capabilities: {
				...descriptor("plugin.example").capabilities,
				storage_native_thumbnail: true,
				storage_native_media_metadata: true,
			},
		});
		const props = dialogProps({
			createStep: 2,
			form: policyForm({
				storage_native_thumbnail_enabled: false,
				storage_native_thumbnail_extensions: ["jpg", "webp"],
				storage_native_media_metadata_enabled: false,
				storage_native_media_metadata_extensions: ["mp4", "flac"],
			}),
			storageDriverDescriptor: plugin,
			storageDriverDescriptors: [plugin],
		});
		render(<StoragePolicyDialog {...props} />);

		const thumbnailGroup = screen.getByRole("group", {
			name: "storage_native_thumbnail_enabled",
		});
		const thumbnailSwitch = within(thumbnailGroup).getByRole("switch", {
			name: "storage_native_thumbnail_enabled",
		});
		const thumbnailExtensions = within(thumbnailGroup).getByLabelText(
			"storage_native_thumbnail_extensions",
		);
		expect(thumbnailGroup).toHaveAttribute("data-enabled", "false");
		expect(thumbnailSwitch).toHaveAccessibleDescription(
			"Plugin image thumbnail billing help",
		);
		expect(thumbnailExtensions).toHaveAccessibleDescription(
			"Plugin image extension matching help",
		);
		expect(thumbnailExtensions).toHaveValue("jpg, webp");
		expect(
			thumbnailSwitch.compareDocumentPosition(thumbnailExtensions) &
				Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();

		const mediaGroup = screen.getByRole("group", {
			name: "storage_native_media_metadata_enabled",
		});
		const mediaSwitch = within(mediaGroup).getByRole("switch", {
			name: "storage_native_media_metadata_enabled",
		});
		const mediaExtensions = within(mediaGroup).getByLabelText(
			"storage_native_media_metadata_extensions",
		);
		expect(mediaGroup).toHaveAttribute("data-enabled", "false");
		expect(mediaSwitch).toHaveAccessibleDescription(
			"Plugin audio and video billing help",
		);
		expect(mediaExtensions).toHaveAccessibleDescription(
			"Plugin audio and video extension matching help",
		);
		expect(mediaExtensions).toHaveValue("mp4, flac");
		expect(
			mediaSwitch.compareDocumentPosition(mediaExtensions) &
				Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();

		fireEvent.change(thumbnailExtensions, {
			target: { value: "heic, avif" },
		});
		fireEvent.change(mediaExtensions, {
			target: { value: "mkv, opus" },
		});
		fireEvent.click(thumbnailSwitch);
		fireEvent.click(mediaSwitch);
		expect(props.onFieldChange).toHaveBeenCalledWith(
			"storage_native_thumbnail_extensions",
			["heic", "avif"],
		);
		expect(props.onFieldChange).toHaveBeenCalledWith(
			"storage_native_media_metadata_extensions",
			["mkv", "opus"],
		);
		expect(props.onFieldChange).toHaveBeenCalledWith(
			"storage_native_thumbnail_enabled",
			true,
		);
		expect(props.onFieldChange).toHaveBeenCalledWith(
			"storage_native_media_metadata_enabled",
			true,
		);
	});

	it("renders empty extension inputs while supported native behaviors are disabled", () => {
		const plugin = descriptor("plugin.example", {
			capabilities: {
				...descriptor("plugin.example").capabilities,
				storage_native_thumbnail: true,
				storage_native_media_metadata: true,
			},
		});
		render(
			<StoragePolicyDialog
				{...dialogProps({
					createStep: 2,
					form: policyForm({
						storage_native_thumbnail_enabled: false,
						storage_native_thumbnail_extensions: [],
						storage_native_media_metadata_enabled: false,
						storage_native_media_metadata_extensions: [],
					}),
					storageDriverDescriptor: plugin,
					storageDriverDescriptors: [plugin],
				})}
			/>,
		);

		expect(
			screen.getByRole("switch", {
				name: "storage_native_thumbnail_enabled",
			}),
		).not.toBeChecked();
		expect(
			screen.getByLabelText("storage_native_thumbnail_extensions"),
		).toHaveValue("");
		expect(
			screen.getByRole("switch", {
				name: "storage_native_media_metadata_enabled",
			}),
		).not.toBeChecked();
		expect(
			screen.getByLabelText("storage_native_media_metadata_extensions"),
		).toHaveValue("");
	});

	it("submits native enablement through core form keys", () => {
		const plugin = descriptor("plugin.example", {
			capabilities: {
				...descriptor("plugin.example").capabilities,
				storage_native_thumbnail: true,
				storage_native_media_metadata: true,
			},
		});
		const props = dialogProps({
			createStep: 2,
			storageDriverDescriptor: plugin,
			storageDriverDescriptors: [plugin],
		});
		render(<StoragePolicyDialog {...props} />);

		fireEvent.click(
			screen.getByRole("switch", { name: "storage_native_thumbnail_enabled" }),
		);
		fireEvent.click(
			screen.getByRole("switch", {
				name: "storage_native_media_metadata_enabled",
			}),
		);
		expect(props.onFieldChange).toHaveBeenCalledWith(
			"storage_native_thumbnail_enabled",
			true,
		);
		expect(props.onFieldChange).toHaveBeenCalledWith(
			"storage_native_media_metadata_enabled",
			true,
		);
	});

	it("renders descriptor fallbacks, remote summaries, capacity, actions, and submitting states", () => {
		const plugin = descriptor("plugin.example", {
			actions: [
				action({
					action_id: "test_saved_connection",
					kind: "connection_test",
					endpoints: ["test_policy_connection"],
				}),
				action({
					action_id: "plugin.repair",
					kind: "custom",
					requires_confirmation: true,
				}),
			],
			fields: [
				field("base_path", { default_value: "" }),
				field("remote_node_id", {
					kind: "select",
					select: { data_source: "remote_nodes", value_kind: "integer" },
				}),
				field("remote_storage_target_key", {
					kind: "select",
					select: {
						data_source: "remote_storage_targets",
						value_kind: "string",
					},
				}),
				field("mode", {
					kind: "select",
					select: {
						options: [{ label_key: "mode_relay", value: "relay" }],
						value_kind: "string",
					},
				}),
				field("enabled", { kind: "boolean" }),
			],
			ui: {
				...descriptor("plugin.example").ui,
				icon_name: "not-an-icon",
				icon_src: null,
			},
		});
		const props = dialogProps({
			connectorActionConfirmId: "plugin.repair",
			connectorActionSubmittingId: "plugin.repair",
			form: policyForm({
				connector_config_values: {
					base_path: "",
					enabled: false,
					mode: "relay",
					remote_node_id: 7,
					remote_storage_target_key: "archive",
				},
				is_default: false,
			}),
			mode: "edit",
			policyCapacity: policyCapacity({
				blob_count: 3,
				blob_total_bytes: 2048,
			}),
			remoteNodes: [{ id: 7, name: "Node seven" } as never],
			remoteStorageTargets: [
				{ name: "Archive", target_key: "archive" } as never,
			],
			remoteStorageTargetsError: "targets failed",
			remoteStorageTargetsLoading: true,
			storageDriverDescriptor: plugin,
			storageDriverDescriptors: [plugin],
			submitting: true,
		});
		render(<StoragePolicyDialog {...props} />);

		fireEvent.change(screen.getByLabelText("core:name"), {
			target: { value: "Edited policy" },
		});
		expect(screen.getByTestId("remote-targets")).toHaveTextContent(
			"targets failed",
		);
		expect(screen.getByTestId("remote-targets")).toHaveTextContent("true");
		expect(screen.getByText("policy_capacity_status_supported")).toBeVisible();
		expect(
			screen.getByRole("button", { name: "plugin.repair" }),
		).toBeDisabled();
		expect(screen.getByRole("button", { name: "save_changes" })).toBeDisabled();
		expect(props.onFieldChange).toHaveBeenCalledWith("name", "Edited policy");
	});
});
