import { describe, expect, it } from "vitest";
import type {
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
	StorageConnectorTransitionPreview,
} from "@/types/api";
import { emptyForm } from "./formTypes";
import {
	applyPolicyConnectorTransition,
	applyPolicyFormFieldChange,
	applyRecommendedPolicyConnectorTransition,
} from "./policyFormTransition";

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
	fields: StorageConnectorFieldDescriptor[] = [],
): StorageConnectorDescriptor {
	return {
		actions: [],
		capabilities: {
			capacity: false,
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
		description: `${connectorId} descriptor`,
		fields,
		label: connectorId,
		requires_authorization: false,
		supports_initial_setup: true,
		ui: {
			badge_rgb: { red: 113, green: 113, blue: 122 },
			base_path_empty_display: "root",
			base_path_placeholder: "prefix",
			config_step_description_key: "config_desc",
			config_step_title_key: "config_title",
			description_key: "connector_desc",
			edit_context_key: "edit_context",
			helper_key: "helper",
			icon_name: "hard-drive",
			label_key: "connector_label",
		},
		upload_workflows: {
			frontend_direct_provider_resumable_upload: false,
			object_multipart_upload: false,
			presigned_upload: false,
			provider_resumable_upload: false,
			simple_upload: true,
			simple_upload_capabilities: {
				policy_limited: true,
				server_side_relay: true,
			},
			stream_upload: true,
		},
	};
}

describe("policy form transitions", () => {
	it("starts a clean connector-owned envelope and applies only target defaults", () => {
		const form = {
			...emptyForm,
			connector_id: "plugin.old",
			connector_config_values: { endpoint: "old", opaque: true },
			credential_values: { token: "secret" },
			thumbnail_processor: "storage_native" as const,
			thumbnail_extensions: ["jpg"],
			media_metadata_extensions: ["mp4"],
		};
		const target = descriptor("plugin.new", [
			field("region", { default_value: "auto" }),
			field("enabled", { kind: "boolean", default_value: false }),
			field("credential", {
				default_value: "must-not-leak",
				scope: "static_credential",
			}),
		]);

		expect(
			applyPolicyConnectorTransition(form, target.connector_id, target),
		).toEqual({
			...form,
			connector_id: "plugin.new",
			connector_config_values: { region: "auto", enabled: false },
			credential_values: {},
			thumbnail_processor: null,
			thumbnail_extensions: [],
			media_metadata_extensions: [],
		});
	});

	it("uses an empty config when the descriptor is missing or declares no defaults", () => {
		const form = {
			...emptyForm,
			connector_config_values: { stale: "value" },
			credential_values: { stale_secret: "value" },
		};

		expect(
			applyPolicyConnectorTransition(form, "plugin.missing", null),
		).toMatchObject({
			connector_id: "plugin.missing",
			connector_config_values: {},
			credential_values: {},
		});
		expect(
			applyPolicyConnectorTransition(
				form,
				"plugin.empty",
				descriptor("plugin.empty"),
			).connector_config_values,
		).toEqual({});
	});

	it("does not mutate the original form or share default value maps", () => {
		const form = { ...emptyForm };
		const target = descriptor("plugin.new", [
			field("region", { default_value: "auto" }),
		]);
		const first = applyPolicyConnectorTransition(
			form,
			target.connector_id,
			target,
		);
		first.connector_config_values.region = "changed";
		const second = applyPolicyConnectorTransition(
			form,
			target.connector_id,
			target,
		);

		expect(form).toEqual(emptyForm);
		expect(second.connector_config_values).toEqual({ region: "auto" });
	});

	it("applies a backend-resolved transition and maps browser-held credentials", () => {
		const source = {
			...emptyForm,
			name: "Archive",
			connector_id: "plugin.s3",
			connector_config_values: {
				endpoint: "https://bucket.provider.test",
				legacy_only: true,
			},
			credential_values: {
				access_key: "browser-secret-id",
				secret_key: "browser-secret-key",
				unmapped_secret: "drop-me",
			},
			thumbnail_processor: "storage_native" as const,
			thumbnail_extensions: ["jpg"],
			media_metadata_extensions: ["mp4"],
		};
		const target = descriptor("plugin.vendor", [
			field("endpoint", { required: true }),
			field("enabled", { kind: "boolean", default_value: true }),
			field("vendor_id", {
				scope: "static_credential",
				kind: "secret",
				secret: true,
			}),
			field("vendor_key", {
				scope: "static_credential",
				kind: "secret",
				secret: true,
			}),
		]);
		const transition: StorageConnectorTransitionPreview = {
			transition_id: "from_generic",
			source_connector_id: "plugin.s3",
			target_connector_id: "plugin.vendor",
			label_key: "transition_label",
			description_key: "transition_desc",
			requires_confirmation: true,
			target_connector_config: {
				format_version: 1,
				connector_id: "plugin.vendor",
				schema_version: 1,
				values: { endpoint: "https://bucket.provider.test", enabled: false },
			},
			target_behavior: {
				thumbnail_processor: null,
				thumbnail_extensions: [],
				media_metadata_extensions: [],
			},
			field_mappings: [
				{
					source_scope: "static_credential",
					source_name: "access_key",
					target_scope: "static_credential",
					target_name: "vendor_id",
				},
				{
					source_scope: "static_credential",
					source_name: "secret_key",
					target_scope: "static_credential",
					target_name: "vendor_key",
				},
			],
		};

		expect(
			applyRecommendedPolicyConnectorTransition(source, transition, target),
		).toEqual({
			...source,
			connector_id: "plugin.vendor",
			connector_config_values: {
				endpoint: "https://bucket.provider.test",
				enabled: false,
			},
			credential_values: {
				vendor_id: "browser-secret-id",
				vendor_key: "browser-secret-key",
			},
			thumbnail_processor: null,
			thumbnail_extensions: [],
			media_metadata_extensions: [],
		});
		expect(source.credential_values).toHaveProperty(
			"unmapped_secret",
			"drop-me",
		);
	});

	it("applies ordinary policy-level field changes without connector knowledge", () => {
		const changed = applyPolicyFormFieldChange(emptyForm, "name", "Archive");

		expect(changed.name).toBe("Archive");
		expect(emptyForm.name).toBe("");
	});
});
