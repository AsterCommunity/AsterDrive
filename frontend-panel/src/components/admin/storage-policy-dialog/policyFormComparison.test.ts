import { describe, expect, it } from "vitest";
import type {
	DriverType,
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
	StoragePolicy,
	StoragePolicyOptions,
} from "@/types/api";
import { emptyForm, type PolicyFormData } from "./formTypes";
import { policyFormHasUnsavedChanges } from "./policyFormComparison";

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

function policy(
	overrides: Partial<StoragePolicy> & { options?: StoragePolicyOptions } = {},
): StoragePolicy {
	return {
		allowed_types: [],
		base_path: "",
		bucket: "",
		chunk_size: 5 * 1024 * 1024,
		created_at: "2026-01-01T00:00:00Z",
		driver_type: "local",
		endpoint: "",
		id: 1,
		is_default: false,
		max_file_size: null,
		name: "",
		options: {},
		updated_at: "2026-01-01T00:00:00Z",
		...overrides,
	} as unknown as StoragePolicy;
}

describe("policyFormComparison", () => {
	it("does not report unsaved changes when no policy is being edited", () => {
		expect(policyFormHasUnsavedChanges(emptyForm, null)).toBe(false);
	});

	it("detects storage-native array value changes", () => {
		const form: PolicyFormData = {
			...emptyForm,
			storage_native_processing_enabled: true,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["jpg"],
		};

		expect(
			policyFormHasUnsavedChanges(
				form,
				policy({
					options: {
						storage_native_processing_enabled: true,
						thumbnail_processor: "storage_native",
						thumbnail_extensions: ["png"],
					},
				}),
			),
		).toBe(true);
	});

	it("ignores empty Microsoft Graph application credentials", () => {
		const oneDriveDescriptor = descriptor("one_drive", {
			fields: [
				field("account_mode", "policy_options"),
				field("client_id", "application_credential"),
			],
		});
		const form: PolicyFormData = {
			...emptyForm,
			base_path: "/files",
			driver_type: "one_drive",
			name: "OneDrive",
			onedrive_tenant: " common ",
			application_credentials: {
				microsoft_graph: {
					cloud: "global",
					tenant: " common ",
					client_id: " ",
					client_secret: " ",
					scopes: " ",
				},
			},
		};

		expect(
			policyFormHasUnsavedChanges(
				form,
				policy({
					base_path: "/files",
					driver_type: "one_drive",
					name: "OneDrive",
					options: {
						onedrive_account_mode: "work_or_school",
						onedrive_cloud: "global",
						onedrive_tenant: "common",
					},
				}),
				oneDriveDescriptor,
			),
		).toBe(false);
	});

	it("keeps non-empty Microsoft Graph application credentials comparable", () => {
		const oneDriveDescriptor = descriptor("one_drive", {
			fields: [field("client_id", "application_credential")],
		});
		const form: PolicyFormData = {
			...emptyForm,
			driver_type: "one_drive",
			application_credentials: {
				microsoft_graph: {
					cloud: "global",
					tenant: "common",
					client_id: "client-id",
					client_secret: "",
					scopes: "",
				},
			},
		};

		expect(
			policyFormHasUnsavedChanges(
				form,
				policy({ driver_type: "one_drive" }),
				oneDriveDescriptor,
			),
		).toBe(true);
	});
});
