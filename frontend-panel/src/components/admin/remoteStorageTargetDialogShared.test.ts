import { describe, expect, it } from "vitest";
import {
	buildCreateRemoteStorageTargetPayload,
	buildUpdateRemoteStorageTargetPayload,
	getRemoteStorageTargetForm,
} from "@/components/admin/remoteStorageTargetDialogShared";
import { emptyForm } from "@/components/admin/storage-policy-dialog/formTypes";
import type {
	RemoteStorageTargetInfo,
	StorageConnectorDescriptor,
} from "@/types/api";

function descriptor(
	connectorId: string,
	credentialMode: "none" | "static_secret",
	fields: Array<{
		name: string;
		scope: "connector_config" | "static_credential";
		required?: boolean;
	}>,
): StorageConnectorDescriptor {
	return {
		connector_id: connectorId,
		config_schema_version: 1,
		credential_mode: credentialMode,
		credential_schema_version: credentialMode === "static_secret" ? 1 : null,
		fields: fields.map((field) => ({
			kind: field.scope === "static_credential" ? "secret" : "text",
			label_key: field.name,
			name: field.name,
			required: field.required ?? false,
			scope: field.scope,
			secret: field.scope === "static_credential",
		})),
	} as StorageConnectorDescriptor;
}

const localDescriptor = descriptor("asterdrive.storage.local", "none", [
	{ name: "base_path", scope: "connector_config", required: true },
]);
const s3Descriptor = descriptor("asterdrive.storage.s3", "static_secret", [
	{ name: "endpoint", scope: "connector_config", required: true },
	{ name: "bucket", scope: "connector_config", required: true },
	{ name: "base_path", scope: "connector_config" },
	{ name: "s3_access_key_id", scope: "static_credential", required: true },
	{ name: "s3_secret_access_key", scope: "static_credential", required: true },
]);

describe("remoteStorageTargetDialogShared", () => {
	it("maps connector-owned target data into the shared policy form", () => {
		const form = getRemoteStorageTargetForm({
			target_key: "rst_demo",
			name: "Follower Cache",
			connector_id: "asterdrive.storage.local",
			connector_config: {
				format_version: 1,
				connector_id: "asterdrive.storage.local",
				schema_version: 1,
				values: { base_path: "cache/inbox" },
			},
			is_default: true,
			desired_revision: 3,
			applied_revision: 3,
			last_error: "",
			created_at: "",
			updated_at: "",
		} as RemoteStorageTargetInfo);

		expect(form.connector_id).toBe("asterdrive.storage.local");
		expect(form.connector_config_values).toEqual({ base_path: "cache/inbox" });
		expect(form.credential_values).toEqual({});
		expect(form.is_default).toBe(true);
	});

	it("builds create payloads with the shared storage connection contract", () => {
		const payload = buildCreateRemoteStorageTargetPayload(
			{
				...emptyForm,
				name: " Archive ",
				connector_id: "asterdrive.storage.s3",
				connector_config_values: {
					endpoint: " https://s3.example.test/uploads ",
					bucket: " uploads ",
					base_path: "tenant-a/incoming",
				},
				credential_values: {
					s3_access_key_id: "ACCESS",
					s3_secret_access_key: "SECRET",
				},
			},
			s3Descriptor,
		);

		expect(payload).toEqual({
			name: "Archive",
			connection: {
				connector_config: expect.objectContaining({
					connector_id: "asterdrive.storage.s3",
					values: expect.objectContaining({
						endpoint: " https://s3.example.test/uploads ",
						bucket: " uploads ",
						base_path: "tenant-a/incoming",
					}),
				}),
				credential: {
					mode: "static",
					values: {
						s3_access_key_id: "ACCESS",
						s3_secret_access_key: "SECRET",
					},
				},
			},
			is_default: false,
		});
	});

	it("uses the same connection shape for updates and omits blank saved secrets", () => {
		const payload = buildUpdateRemoteStorageTargetPayload(
			{
				...emptyForm,
				name: "Archive",
				connector_id: "asterdrive.storage.s3",
				connector_config_values: {
					endpoint: "https://s3.example.test",
					bucket: "uploads",
					base_path: "tenant-a/incoming",
				},
				credential_values: {},
				is_default: true,
			},
			s3Descriptor,
			{
				connector_id: "asterdrive.storage.s3",
			} as RemoteStorageTargetInfo,
		);

		expect(payload.connection?.connector_config.connector_id).toBe(
			"asterdrive.storage.s3",
		);
		expect(payload.connection?.credential).toEqual({ mode: "none" });
	});

	it("does not project unrelated provider fields into local connections", () => {
		const payload = buildCreateRemoteStorageTargetPayload(
			{
				...emptyForm,
				name: "Local",
				connector_id: "asterdrive.storage.local",
				connector_config_values: {
					base_path: "tenant-a/local",
					endpoint: "https://unused.example.com",
				},
				credential_values: { token: "unused" },
				is_default: true,
			},
			localDescriptor,
		);

		expect(payload.connection).toEqual({
			connector_config: expect.objectContaining({
				connector_id: "asterdrive.storage.local",
				values: { base_path: "tenant-a/local" },
			}),
			credential: { mode: "none" },
		});
	});
});
