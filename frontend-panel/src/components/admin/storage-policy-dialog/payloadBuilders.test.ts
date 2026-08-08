import { describe, expect, it } from "vitest";
import type {
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
} from "@/types/api";
import { emptyForm, type PolicyFormData } from "./formTypes";
import {
	buildCreatePolicyPayload,
	buildPolicyTestPayload,
	buildStorageConnectorActionPayload,
	buildStorageConnectorConnection,
	buildUpdatePolicyPayload,
} from "./payloadBuilders";

function field(
	name: string,
	scope: StorageConnectorFieldDescriptor["scope"],
	overrides: Partial<StorageConnectorFieldDescriptor> = {},
): StorageConnectorFieldDescriptor {
	return {
		kind: scope === "static_credential" ? "secret" : "text",
		label_key: name,
		name,
		required: false,
		scope,
		secret: scope === "static_credential",
		...overrides,
	};
}

function descriptor(
	credentialMode: StorageConnectorDescriptor["credential_mode"] = "static_secret",
) {
	return {
		config_schema_version: 4,
		connector_id: "plugin.archive",
		credential_mode: credentialMode,
		fields: [
			field("endpoint", "connector_config", {
				required: true,
				trim_on_blur: true,
			}),
			field("optional", "connector_config"),
			field("access_key", "static_credential", { trim_on_blur: true }),
			field("client_id", "authorization_application", { trim_on_blur: true }),
			field("action_only", "action_input"),
		],
	} as StorageConnectorDescriptor;
}

function form(overrides: Partial<PolicyFormData> = {}): PolicyFormData {
	return {
		...emptyForm,
		chunk_size: "8",
		connector_id: "plugin.archive",
		connector_config_values: {
			endpoint: "  https://archive.example.test  ",
			optional: "",
		},
		credential_values: {
			access_key: "  KEY  ",
			client_id: "  CLIENT  ",
		},
		is_default: true,
		max_file_size: "1024",
		storage_native_media_metadata_enabled: true,
		storage_native_media_metadata_extensions: ["mp4"],
		name: "Archive",
		storage_native_thumbnail_extensions: ["jpg"],
		storage_native_thumbnail_enabled: true,
		...overrides,
	};
}

describe("storage policy payload builders", () => {
	it("builds a versioned create connection and keeps config, credential, and behavior isolated", () => {
		const payload = buildCreatePolicyPayload(form(), descriptor());

		expect(payload).toEqual({
			chunk_size: 8 * 1024 * 1024,
			connection: {
				behavior: {
					storage_native_media_metadata_extensions: ["mp4"],
					storage_native_media_metadata_enabled: true,
					storage_native_thumbnail_extensions: ["jpg"],
					storage_native_thumbnail_enabled: true,
				},
				connector_config: {
					connector_id: "plugin.archive",
					format_version: 1,
					schema_version: 4,
					values: { endpoint: "https://archive.example.test" },
				},
				credential: { mode: "static", values: { access_key: "KEY" } },
			},
			is_default: true,
			max_file_size: 1024,
			name: "Archive",
		});
	});

	it("uses authorization application credentials only for OAuth connectors", () => {
		const payload = buildCreatePolicyPayload(
			form(),
			descriptor("oauth_delegated"),
		);

		expect(payload.connection.credential).toEqual({
			mode: "authorization_application",
			values: { client_id: "CLIENT" },
		});
	});

	it("uses the none credential fallback when optional credentials are absent", () => {
		const staticSchema = descriptor();
		const oauthSchema = descriptor("oauth_delegated");
		const input = form({ credential_values: {} });

		expect(
			buildStorageConnectorConnection(input, staticSchema, false).credential,
		).toEqual({ mode: "none" });
		expect(buildUpdatePolicyPayload(input, oauthSchema)).not.toHaveProperty(
			"credential",
		);
		expect(
			buildCreatePolicyPayload(input, staticSchema).connection.credential,
		).toEqual({ mode: "static", values: {} });
	});

	it("omits blank update credentials and keeps required empty connector values", () => {
		const schema = descriptor();
		const payload = buildUpdatePolicyPayload(
			form({
				connector_config_values: { endpoint: "" },
				credential_values: { access_key: "" },
			}),
			schema,
		);

		expect(payload).not.toHaveProperty("credential");
		expect(payload.connector_config.values).toEqual({ endpoint: "" });
	});

	it("defensively omits null optional connector values", () => {
		const payload = buildUpdatePolicyPayload(
			form({
				connector_config_values: {
					endpoint: "https://archive.example.test",
					optional: null,
				},
			}),
			descriptor(),
		);

		expect(payload.connector_config.values).toEqual({
			endpoint: "https://archive.example.test",
		});
	});

	it("builds draft connection tests and custom action inputs without persisting action values", () => {
		const schema = descriptor();
		const testPayload = buildPolicyTestPayload(form(), schema, 7);
		const actionPayload = buildStorageConnectorActionPayload(
			form(),
			7,
			schema,
			"plugin.reindex",
			{ action_only: "full" },
		);

		expect(testPayload.policy_id).toBe(7);
		expect(actionPayload).toMatchObject({
			action_id: "plugin.reindex",
			policy_id: 7,
			values: { action_only: "full" },
		});
		expect(actionPayload.connection.connector_config.values).not.toHaveProperty(
			"action_only",
		);
		for (const behavior of [
			testPayload.connection.behavior,
			actionPayload.connection.behavior,
		]) {
			expect(behavior).toEqual({
				storage_native_media_metadata_extensions: ["mp4"],
				storage_native_media_metadata_enabled: true,
				storage_native_thumbnail_extensions: ["jpg"],
				storage_native_thumbnail_enabled: true,
			});
		}
	});

	it("preserves active and dormant native configuration in every payload shape", () => {
		const schema = descriptor();
		for (const [enabled, extensions] of [
			[false, ["mp4"]],
			[true, []],
		] as const) {
			const input = form({
				storage_native_thumbnail_enabled: enabled,
				storage_native_thumbnail_extensions: ["jpg"],
				storage_native_media_metadata_enabled: enabled,
				storage_native_media_metadata_extensions: [...extensions],
			});
			const behaviors = [
				buildCreatePolicyPayload(input, schema).connection.behavior,
				buildUpdatePolicyPayload(input, schema).behavior,
				buildPolicyTestPayload(input, schema).connection.behavior,
				buildStorageConnectorActionPayload(
					input,
					7,
					schema,
					"plugin.reindex",
					{},
				).connection.behavior,
			];
			for (const behavior of behaviors) {
				expect(behavior.storage_native_thumbnail_enabled).toBe(enabled);
				expect(behavior.storage_native_thumbnail_extensions).toEqual(["jpg"]);
				expect(behavior.storage_native_media_metadata_enabled).toBe(enabled);
				expect(behavior.storage_native_media_metadata_extensions).toEqual(
					extensions,
				);
			}
		}
	});

	it("never copies legacy native enablement fields into connector config", () => {
		const input = form({
			connector_config_values: {
				endpoint: "https://archive.example.test",
				storage_native_processing_enabled: true,
				storage_native_media_metadata_enabled: true,
			},
		});
		const connection = buildStorageConnectorConnection(
			input,
			descriptor(),
			false,
		);
		expect(connection.connector_config.values).toEqual({
			endpoint: "https://archive.example.test",
		});
		expect(connection.behavior.storage_native_media_metadata_enabled).toBe(
			true,
		);
	});

	it("handles empty, zero, and invalid numeric inputs deterministically", () => {
		const schema = descriptor("none");
		const zero = buildCreatePolicyPayload(
			form({ chunk_size: "0", max_file_size: "0", credential_values: {} }),
			schema,
		);
		const invalid = buildCreatePolicyPayload(
			form({ chunk_size: "bad", max_file_size: "", credential_values: {} }),
			schema,
		);

		expect(zero.chunk_size).toBe(0);
		expect(zero.max_file_size).toBe(0);
		expect(zero.connection.credential).toEqual({ mode: "none" });
		expect(invalid.chunk_size).toBe(0);
		expect(invalid.max_file_size).toBeUndefined();
	});

	it("uses descriptor default modes consistently for create, update, and connection tests", () => {
		const schema = descriptor("none");
		schema.fields.push(
			field("base_path", "connector_config", {
				default_mode: "missing_or_empty_text",
				default_value: "./data/uploads",
				trim_on_blur: true,
			}),
			field("ordinary_prefix", "connector_config", {
				default_value: "objects",
			}),
		);
		const input = form({
			connector_config_values: {
				base_path: "   ",
				endpoint: "https://archive.example.test",
				ordinary_prefix: "",
			},
			credential_values: {},
		});

		const create = buildCreatePolicyPayload(input, schema);
		const update = buildUpdatePolicyPayload(input, schema);
		const connectionTest = buildPolicyTestPayload(input, schema);
		for (const values of [
			create.connection.connector_config.values,
			update.connector_config.values,
			connectionTest.connection.connector_config.values,
		]) {
			expect(values).toEqual({
				base_path: "./data/uploads",
				endpoint: "https://archive.example.test",
			});
		}
	});
});
