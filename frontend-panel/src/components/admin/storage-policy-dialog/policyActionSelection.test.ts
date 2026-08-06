import { describe, expect, it } from "vitest";
import type { StorageConnectorDescriptor, StoragePolicy } from "@/types/api";
import { getPolicyConnectionTestKey } from "./connectionNormalization";
import { emptyForm } from "./formTypes";
import {
	selectStorageConnectorCustomActionExecutionMode,
	selectStoragePolicyActionValueSource,
	selectStoragePolicyConnectionTestMode,
	shouldRunPolicyConnectionSaveTest,
} from "./policyActionSelection";

function descriptor(
	connectorId: string,
	actions: StorageConnectorDescriptor["actions"],
): StorageConnectorDescriptor {
	return {
		actions,
		authorization_provider: null,
		capabilities: {
			capacity: true,
			efficient_range: true,
			list: true,
			object_storage_transfer_strategy: false,
			object_naming: "opaque_uuid",
			presigned_download: false,
			remote_node_binding: false,
			storage_native_media_metadata: false,
			storage_native_thumbnail: false,
		},
		config_schema_version: 1,
		connector_id: connectorId,
		credential_mode: "none",
		deployment_scope: "instance_local",
		description: `${connectorId} descriptor`,
		fields: [
			{
				kind: "text",
				label_key: "base_path",
				name: "base_path",
				required: false,
				scope: "connector_config",
				secret: false,
			},
		],
		label: connectorId,
		related_issues: [],
		requires_authorization: false,
		supports_initial_setup: true,
		ui: {
			badge_rgb: { red: 113, green: 113, blue: 122 },
			base_path_empty_display: "core:root",
			base_path_placeholder: "/data/uploads",
			config_step_description_key: "connector_config_desc",
			config_step_title_key: "connector_config_title",
			description_key: `${connectorId}_description`,
			edit_context_key: "connector_edit_context",
			helper_key: "connector_helper",
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
	};
}

const draftAction = {
	action_id: "test_draft_connection",
	description_key: "policy_test_draft_connection_desc",
	endpoints: ["test_policy_params"],
	kind: "connection_test",
	label_key: "policy_test_draft_connection",
	mutates_remote_state: false,
	requires_authorization: false,
	requires_confirmation: false,
	requires_saved_policy: false,
} as const;

const savedAction = {
	action_id: "test_saved_connection",
	description_key: "policy_test_saved_connection_desc",
	endpoints: ["test_policy_connection"],
	kind: "connection_test",
	label_key: "policy_test_saved_connection",
	mutates_remote_state: false,
	requires_authorization: false,
	requires_confirmation: false,
	requires_saved_policy: true,
} as const;

const customAction = {
	action_id: "plugin.repair_path",
	description_key: "plugin_repair_path_desc",
	kind: "custom",
	label_key: "plugin_repair_path",
	mutates_remote_state: true,
	requires_authorization: false,
	requires_confirmation: true,
	requires_saved_policy: false,
} as const;

function policy(overrides: Partial<StoragePolicy> = {}): StoragePolicy {
	return {
		allowed_types: [],
		behavior: {},
		chunk_size: 5 * 1024 * 1024,
		connector_config: {
			connector_id: "asterdrive.storage.local",
			format_version: 1,
			schema_version: 1,
			values: { base_path: "" },
		},
		connector_id: "asterdrive.storage.local",
		created_at: "2026-01-01T00:00:00Z",
		id: 7,
		is_default: false,
		max_file_size: 0,
		name: "Local",
		updated_at: "2026-01-01T00:00:00Z",
		...overrides,
	};
}

const localForm = {
	...emptyForm,
	connector_id: "asterdrive.storage.local",
	connector_config_values: { base_path: "" },
};

describe("policy action selection", () => {
	it("uses draft values for new policies and changed edits", () => {
		const localDescriptor = descriptor("asterdrive.storage.local", [
			draftAction,
			savedAction,
		]);

		expect(
			selectStoragePolicyActionValueSource({
				descriptor: localDescriptor,
				editingId: null,
				editingPolicy: null,
				form: emptyForm,
			}),
		).toBe("draft");

		expect(
			selectStoragePolicyActionValueSource({
				descriptor: localDescriptor,
				editingId: 7,
				editingPolicy: policy(),
				form: {
					...localForm,
					connector_config_values: { base_path: "/data" },
				},
			}),
		).toBe("draft");
	});

	it("uses saved values for unchanged edits", () => {
		const localDescriptor = descriptor("asterdrive.storage.local", [
			draftAction,
			savedAction,
		]);

		expect(
			selectStoragePolicyActionValueSource({
				descriptor: localDescriptor,
				editingId: 7,
				editingPolicy: policy(),
				form: localForm,
			}),
		).toBe("saved");
	});

	it("returns unsupported when the descriptor lacks the selected connection test mode", () => {
		const savedOnlyDescriptor = descriptor("asterdrive.storage.local", [
			savedAction,
		]);
		const draftOnlyDescriptor = descriptor("asterdrive.storage.local", [
			draftAction,
		]);

		expect(
			selectStoragePolicyConnectionTestMode({
				descriptor: savedOnlyDescriptor,
				editingId: null,
				editingPolicy: null,
				form: localForm,
			}),
		).toBe("unsupported");

		expect(
			selectStoragePolicyConnectionTestMode({
				descriptor: draftOnlyDescriptor,
				editingId: 7,
				editingPolicy: policy(),
				form: localForm,
			}),
		).toBe("unsupported");
	});

	it("routes custom actions only through connector-declared endpoints", () => {
		expect(
			selectStorageConnectorCustomActionExecutionMode(
				{
					...customAction,
					endpoints: ["execute_draft_storage_policy_action"],
				},
				"saved",
				7,
			),
		).toBe("draft");
		expect(
			selectStorageConnectorCustomActionExecutionMode(
				{
					...customAction,
					endpoints: ["execute_saved_storage_policy_action"],
					requires_saved_policy: true,
				},
				"saved",
				7,
			),
		).toBe("saved");
		expect(
			selectStorageConnectorCustomActionExecutionMode(
				{
					...customAction,
					endpoints: [
						"execute_draft_storage_policy_action",
						"execute_saved_storage_policy_action",
					],
				},
				"draft",
				7,
			),
		).toBe("draft");
	});

	it("reports save-first and unsupported custom action endpoint boundaries", () => {
		expect(
			selectStorageConnectorCustomActionExecutionMode(
				{
					...customAction,
					endpoints: ["execute_saved_storage_policy_action"],
					requires_saved_policy: true,
				},
				"draft",
				7,
			),
		).toBe("save_first");
		expect(
			selectStorageConnectorCustomActionExecutionMode(
				{ ...customAction, endpoints: [] },
				"saved",
				7,
			),
		).toBe("unsupported");
		expect(
			selectStorageConnectorCustomActionExecutionMode(
				{ ...customAction, endpoints: [] },
				"draft",
				7,
			),
		).toBe("unsupported");
		expect(
			selectStorageConnectorCustomActionExecutionMode(
				{
					...customAction,
					endpoints: ["execute_saved_storage_policy_action"],
					requires_saved_policy: true,
				},
				"saved",
				null,
			),
		).toBe("unsupported");
	});

	it("skips save-time connection tests after the current connection key was validated", () => {
		const localDescriptor = descriptor("asterdrive.storage.local", [
			draftAction,
		]);
		const form = {
			...localForm,
			connector_config_values: { base_path: "/data" },
		};
		const validatedConnectionKey = getPolicyConnectionTestKey(
			form,
			localDescriptor,
		);

		expect(
			shouldRunPolicyConnectionSaveTest({
				descriptor: localDescriptor,
				editingId: null,
				editingPolicy: null,
				form,
				validatedConnectionKey,
			}),
		).toBe(false);
		expect(
			shouldRunPolicyConnectionSaveTest({
				descriptor: localDescriptor,
				editingId: null,
				editingPolicy: null,
				form,
				validatedConnectionKey: null,
			}),
		).toBe(true);
	});
});
