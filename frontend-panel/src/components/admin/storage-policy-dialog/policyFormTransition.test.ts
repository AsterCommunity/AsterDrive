import { describe, expect, it } from "vitest";
import type {
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
} from "@/types/api";
import { emptyForm } from "./formTypes";
import {
	applyPolicyConnectorTransition,
	applyPolicyFormFieldChange,
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
	it("starts a clean connector envelope while retaining dormant core behavior configuration", () => {
		const form = {
			...emptyForm,
			connector_id: "plugin.old",
			connector_config_values: { endpoint: "old", opaque: true },
			credential_values: { token: "secret" },
			storage_native_thumbnail_enabled: true,
			storage_native_thumbnail_extensions: ["jpg"],
			storage_native_media_metadata_enabled: true,
			storage_native_media_metadata_extensions: ["mp4"],
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
			storage_native_thumbnail_enabled: false,
			storage_native_thumbnail_extensions: ["jpg"],
			storage_native_media_metadata_enabled: false,
			storage_native_media_metadata_extensions: ["mp4"],
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

	it("applies ordinary policy-level field changes without connector knowledge", () => {
		const changed = applyPolicyFormFieldChange(emptyForm, "name", "Archive");

		expect(changed.name).toBe("Archive");
		expect(emptyForm.name).toBe("");
	});

	it("enables thumbnails with defaults without replacing an existing extension set", () => {
		const enabled = applyPolicyFormFieldChange(
			emptyForm,
			"storage_native_thumbnail_enabled",
			true,
		);
		expect(enabled.storage_native_thumbnail_extensions).toEqual([
			"jpg",
			"jpeg",
			"png",
			"webp",
			"gif",
		]);
		expect(emptyForm.storage_native_thumbnail_extensions).toEqual([]);

		const existing = applyPolicyFormFieldChange(
			{ ...emptyForm, storage_native_thumbnail_extensions: ["heic"] },
			"storage_native_thumbnail_enabled",
			true,
		);
		expect(existing.storage_native_thumbnail_extensions).toEqual(["heic"]);
	});

	it("disables thumbnails without discarding their matching configuration", () => {
		const original = {
			...emptyForm,
			storage_native_thumbnail_enabled: true,
			storage_native_thumbnail_extensions: ["jpg"],
		};
		const disabled = applyPolicyFormFieldChange(
			original,
			"storage_native_thumbnail_enabled",
			false,
		);
		expect(disabled).toMatchObject({
			storage_native_thumbnail_enabled: false,
			storage_native_thumbnail_extensions: ["jpg"],
		});
		expect(original.storage_native_thumbnail_extensions).toEqual(["jpg"]);
	});

	it("toggles media metadata without touching thumbnail behavior", () => {
		const original = {
			...emptyForm,
			storage_native_thumbnail_enabled: true,
			storage_native_thumbnail_extensions: ["png"],
			storage_native_media_metadata_extensions: ["mp4"],
		};
		const enabled = applyPolicyFormFieldChange(
			original,
			"storage_native_media_metadata_enabled",
			true,
		);
		expect(enabled.storage_native_media_metadata_extensions).toEqual(["mp4"]);
		expect(enabled.storage_native_thumbnail_extensions).toEqual(["png"]);

		const disabled = applyPolicyFormFieldChange(
			enabled,
			"storage_native_media_metadata_enabled",
			false,
		);
		expect(disabled.storage_native_media_metadata_extensions).toEqual(["mp4"]);
		expect(disabled.storage_native_thumbnail_enabled).toBe(true);
		expect(original.storage_native_media_metadata_enabled).toBe(false);

		const reenabled = applyPolicyFormFieldChange(
			disabled,
			"storage_native_media_metadata_enabled",
			true,
		);
		expect(reenabled.storage_native_media_metadata_extensions).toEqual(["mp4"]);
	});
});
