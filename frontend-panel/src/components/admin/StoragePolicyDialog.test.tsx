import { fireEvent, render, screen, within } from "@testing-library/react";
import type { ComponentProps } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
	StorageConnectorActionDescriptor,
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
	StorageConnectorTransitionPreview,
	StoragePolicyCapacityInfo,
} from "@/types/api";
import { StoragePolicyDialog } from "./StoragePolicyDialog";
import {
	emptyForm,
	type PolicyFormData,
} from "./storage-policy-dialog/formTypes";

const connectorMessages = vi.hoisted(() => new Map<string, string>());

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
		connectorTransitionConfirmKey: null,
		connectorTransitionSubmittingKey: null,
		connectorTransitions: [],
		connectorTransitionsLoading: false,
		hasUnsavedChanges: false,
		saveAnywayConfirmOpen: false,
		onCancelConnectorAction: vi.fn(),
		onCancelConnectorTransition: vi.fn(),
		onOpenChange: vi.fn(),
		onCancelSaveAnyway: vi.fn(),
		onConfirmSaveAnyway: vi.fn(),
		onConfirmConnectorAction: vi.fn(),
		onConfirmConnectorTransition: vi.fn(),
		onStartStorageAuthorization: vi.fn(),
		onValidateStorageCredential: vi.fn(),
		onCreateRemoteStorageTarget: vi.fn(async () => undefined),
		onSubmit: vi.fn(),
		onRunConnectionTest: vi.fn(async () => true),
		onFieldChange: vi.fn(),
		onConnectorActionValueChange: vi.fn(),
		onRequestConnectorAction: vi.fn(),
		onRequestConnectorTransition: vi.fn(),
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

	it("renders connector-owned credential management messages from the connector namespace", () => {
		for (const [key, value] of Object.entries({
			plugin_credential_loading: "Credential loading",
			plugin_credential_status_authorized: "Credential authorized",
			plugin_credential_status_missing: "Credential missing",
			plugin_credential_title: "Connector credential",
			plugin_redirect_uri: "Connector redirect URI",
			policy_connector_start_authorization: "Authorize connector",
			policy_connector_validate_credential: "Validate connector",
		})) {
			connectorMessages.set(`plugin.example:${key}`, value);
		}
		const plugin = descriptor("plugin.example", {
			credential_management: {
				authorization_started_key: "plugin_authorization_started",
				created_authorize_next_key: "plugin_created_authorize_next",
				loading_key: "plugin_credential_loading",
				redirect_uri_key: "plugin_redirect_uri",
				save_before_authorize_key: "plugin_save_before_authorize",
				save_before_validate_key: "plugin_save_before_validate",
				status_keys: {
					authorized: "plugin_credential_status_authorized",
					missing: "plugin_credential_status_missing",
				},
				title_key: "plugin_credential_title",
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

		render(
			<StoragePolicyDialog
				{...dialogProps({
					mode: "edit",
					storageDriverDescriptor: plugin,
					storageDriverDescriptors: [plugin],
					storageCredentials: [
						{
							created_at: "2026-08-04T00:00:00Z",
							credential_kind: "authorization",
							id: 1,
							policy_id: 7,
							provider: "microsoft_graph",
							scopes: [],
							status: "authorized",
							updated_at: "2026-08-04T00:00:00Z",
						},
					],
				})}
			/>,
		);

		expect(screen.getByText("Connector credential")).toBeVisible();
		expect(screen.getByText("Credential authorized")).toBeVisible();
		expect(screen.getByText("Connector redirect URI")).toBeVisible();
		expect(
			screen.getByRole("button", { name: "Authorize connector" }),
		).toBeVisible();
		expect(
			screen.getByRole("button", { name: "Validate connector" }),
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
			fields: [
				...descriptor("plugin.example").fields,
				field("storage_native_processing_enabled", { kind: "boolean" }),
			],
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
					storage_native_processing_enabled: true,
				},
				media_metadata_extensions: ["mp4"],
				thumbnail_extensions: ["jpg"],
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
						storage_native_processing_enabled: true,
					},
					media_metadata_extensions: ["mp4"],
					thumbnail_extensions: ["jpg"],
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
		expect(props.onFieldChange).toHaveBeenCalledWith("thumbnail_extensions", [
			"jpg",
			"webp",
		]);
		expect(props.onFieldChange).toHaveBeenCalledWith(
			"media_metadata_extensions",
			["mp4", "mov"],
		);
		expect(props.onSubmit).toHaveBeenCalledOnce();
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

	it("presents connector transitions generically and blocks saved execution for dirty forms", () => {
		const transition: StorageConnectorTransitionPreview = {
			transition_id: "from_generic",
			source_connector_id: "plugin.example",
			target_connector_id: "plugin.vendor",
			label_key: "transition_label",
			description_key: "transition_description",
			requires_confirmation: true,
			target_connector_config: {
				format_version: 1,
				connector_id: "plugin.vendor",
				schema_version: 1,
				values: {},
			},
			target_behavior: {
				thumbnail_processor: null,
				thumbnail_extensions: [],
				media_metadata_extensions: [],
			},
		};
		connectorMessages.set(
			"plugin.vendor:transition_label",
			"Use vendor connector",
		);
		connectorMessages.set(
			"plugin.vendor:transition_description",
			"Keep the same object namespace",
		);

		const createProps = dialogProps({
			createStep: 1,
			connectorTransitions: [transition],
		});
		const view = render(<StoragePolicyDialog {...createProps} />);
		expect(screen.getByText("Use vendor connector")).toBeVisible();
		expect(screen.getByText("Keep the same object namespace")).toBeVisible();
		fireEvent.click(
			screen.getByRole("button", {
				name: "policy_connector_transition_apply",
			}),
		);
		expect(createProps.onRequestConnectorTransition).toHaveBeenCalledWith(
			transition,
		);

		view.unmount();
		const editProps = dialogProps({
			mode: "edit",
			connectorTransitions: [transition],
			hasUnsavedChanges: true,
		});
		render(<StoragePolicyDialog {...editProps} />);
		expect(
			screen.getByRole("button", {
				name: "policy_connector_transition_execute",
			}),
		).toBeDisabled();
		expect(
			screen.getByText("policy_connector_transition_save_first"),
		).toBeVisible();
	});
});
