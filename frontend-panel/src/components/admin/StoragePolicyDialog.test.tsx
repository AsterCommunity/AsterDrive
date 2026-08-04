import { fireEvent, render, screen, within } from "@testing-library/react";
import type { ComponentProps } from "react";
import { describe, expect, it, vi } from "vitest";
import type {
	StorageConnectorActionDescriptor,
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
} from "@/types/api";
import { StoragePolicyDialog } from "./StoragePolicyDialog";
import {
	emptyForm,
	type PolicyFormData,
} from "./storage-policy-dialog/formTypes";

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string, values?: Record<string, number | string>) =>
			values ? `${key}:${Object.values(values).map(String).join(":")}` : key,
	}),
}));

vi.mock(
	"@/components/admin/admin-remote-nodes-page/RemoteNodeRemoteStorageTargetSection",
	() => ({ RemoteNodeRemoteStorageTargetSection: () => null }),
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
	it("keeps the previous two-column connector selection and advances directly from a descriptor card", () => {
		const available = descriptor("plugin.example");
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

	it("restores the edit context, capacity summary, and separate overview, connection, and rules sections", () => {
		const props = dialogProps({ mode: "edit" });
		render(<StoragePolicyDialog {...props} />);

		expect(screen.getByTestId("policy-edit-shell")).toBeVisible();
		expect(screen.getByTestId("policy-edit-context-bar")).toBeVisible();
		expect(screen.getByTestId("policy-edit-driver-badge")).toHaveTextContent(
			"plugin_label",
		);
		expect(screen.getByTestId("policy-edit-driver-badge")).toHaveClass(
			"border-emerald-500/60",
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
});
