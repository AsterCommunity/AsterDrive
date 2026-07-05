import { describe, expect, it } from "vitest";
import type {
	DriverType,
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
} from "@/types/api";
import { emptyForm } from "./formTypes";
import {
	applyPolicyDriverTransition,
	applyPolicyFormFieldChange,
} from "./policyFormTransition";

function field(
	name: string,
	scope: StorageConnectorFieldDescriptor["scope"],
): StorageConnectorFieldDescriptor {
	return {
		kind: "text",
		name,
		required: false,
		scope,
		secret: false,
	};
}

function descriptor(
	driverType: DriverType,
	overrides: Partial<StorageConnectorDescriptor> = {},
): StorageConnectorDescriptor {
	return {
		actions: [],
		authorization_provider: null,
		capabilities: {
			capacity: true,
			efficient_range: true,
			list: true,
			object_storage_transfer_strategy: false,
			presigned_download: false,
			remote_node_binding: false,
			storage_native_media_metadata: false,
			storage_native_thumbnail: false,
		},
		credential_mode: "none",
		description: `${driverType} descriptor`,
		driver_recommendations: [],
		driver_type: driverType,
		enabled: true,
		fields: [],
		label: driverType,
		related_issues: [],
		requires_authorization: false,
		ui: {
			description_key: `${driverType}_description`,
			icon: null,
			label_key: driverType,
		},
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

function objectStorageDescriptor(
	driverType: DriverType,
	overrides: Partial<StorageConnectorDescriptor> = {},
) {
	const base = descriptor(driverType);
	return descriptor(driverType, {
		...overrides,
		capabilities: {
			...base.capabilities,
			object_storage_transfer_strategy: true,
			...overrides.capabilities,
		},
		fields: [
			field("endpoint", "connection"),
			field("bucket", "connection"),
			field("access_key", "connection"),
			field("secret_key", "connection"),
			...(overrides.fields ?? []),
		],
		upload_workflows: {
			...base.upload_workflows,
			object_multipart_upload: true,
			...overrides.upload_workflows,
		},
	});
}

describe("policy form transitions", () => {
	it("resets remote binding and preserves object-storage strategy defaults when switching to object storage", () => {
		const form = {
			...emptyForm,
			driver_type: "remote" as const,
			remote_node_id: "7",
			remote_storage_target_key: "target-a",
			s3_path_style: false,
		};

		const next = applyPolicyDriverTransition(
			form,
			"s3",
			objectStorageDescriptor("s3"),
		);

		expect(next.driver_type).toBe("s3");
		expect(next.remote_node_id).toBe("");
		expect(next.remote_storage_target_key).toBe("");
		expect(next.s3_path_style).toBe(false);
	});

	it("clears object-storage credentials when switching to remote storage", () => {
		const next = applyPolicyDriverTransition(
			{
				...emptyForm,
				driver_type: "s3",
				endpoint: "https://s3.example.com",
				bucket: "files",
				access_key: "access",
				secret_key: "secret",
				content_dedup: true,
				s3_path_style: false,
			},
			"remote",
			descriptor("remote", {
				capabilities: {
					...descriptor("remote").capabilities,
					remote_node_binding: true,
				},
			}),
		);

		expect(next).toMatchObject({
			driver_type: "remote",
			endpoint: "",
			bucket: "",
			access_key: "",
			secret_key: "",
			content_dedup: false,
			remote_storage_target_key: "",
		});
		expect(next).not.toHaveProperty("s3_path_style");
	});

	it("seeds OneDrive defaults and clears application secrets on OneDrive transition", () => {
		const next = applyPolicyDriverTransition(
			{
				...emptyForm,
				driver_type: "s3",
				endpoint: "https://s3.example.com",
				bucket: "files",
				onedrive_cloud: "china",
				onedrive_tenant: "tenant-a",
			},
			"one_drive",
			descriptor("one_drive", {
				fields: [field("account_mode", "policy_options")],
			}),
		);

		expect(next).toMatchObject({
			driver_type: "one_drive",
			endpoint: "",
			bucket: "",
			content_dedup: false,
			onedrive_cloud: "china",
			onedrive_tenant: "tenant-a",
			object_storage_upload_strategy: "relay_stream",
		});
		expect(next.application_credentials.microsoft_graph).toMatchObject({
			cloud: "china",
			tenant: "tenant-a",
			client_id: "",
			client_secret: "",
			scopes: "",
		});
		expect(next).not.toHaveProperty("s3_path_style");
	});

	it("preserves storage-native extension choices only when the target descriptor supports them", () => {
		const form = {
			...emptyForm,
			storage_native_processing_enabled: true,
			thumbnail_processor: "storage_native" as const,
			thumbnail_extensions: ["png"],
			storage_native_media_metadata_enabled: true,
			media_metadata_extensions: ["mp4"],
		};

		const supported = applyPolicyDriverTransition(
			form,
			"tencent_cos",
			objectStorageDescriptor("tencent_cos", {
				capabilities: {
					...descriptor("tencent_cos").capabilities,
					object_storage_transfer_strategy: true,
					storage_native_media_metadata: true,
					storage_native_thumbnail: true,
				},
			}),
		);
		const unsupported = applyPolicyDriverTransition(
			form,
			"s3",
			objectStorageDescriptor("s3"),
		);

		expect(supported.thumbnail_extensions).toEqual(["png"]);
		expect(supported.media_metadata_extensions).toEqual(["mp4"]);
		expect(unsupported.storage_native_processing_enabled).toBe(false);
		expect(unsupported.thumbnail_extensions).toEqual([]);
		expect(unsupported.media_metadata_extensions).toEqual([]);
	});

	it("updates dependent fields for storage native processing and remote node changes", () => {
		const enabled = applyPolicyFormFieldChange(
			emptyForm,
			"storage_native_processing_enabled",
			true,
		);
		expect(enabled.thumbnail_processor).toBe("storage_native");
		expect(enabled.thumbnail_extensions).toEqual([
			"jpg",
			"jpeg",
			"png",
			"webp",
			"gif",
		]);

		const remoteNodeChanged = applyPolicyFormFieldChange(
			{
				...emptyForm,
				remote_storage_target_key: "old-target",
			},
			"remote_node_id",
			"12",
		);
		expect(remoteNodeChanged.remote_node_id).toBe("12");
		expect(remoteNodeChanged.remote_storage_target_key).toBe("");
	});
});
