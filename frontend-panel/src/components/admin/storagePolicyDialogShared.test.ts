import { describe, expect, it } from "vitest";
import {
	buildCreatePolicyPayload,
	buildPolicyTestPayload,
	buildTencentCosCorsPayload,
	buildUpdatePolicyPayload,
	getEndpointValidationMessage,
	getPolicyConnectionTestKey,
	getPolicyForm,
	getS3CompatibleDriverPromotionTarget,
	hasConnectionFieldChanges,
	isObjectStorageDriver,
	isS3CompatibleDriver,
	isTencentCosEndpoint,
	normalizePolicyForm,
} from "@/components/admin/storagePolicyDialogShared";
import type { StoragePolicy } from "@/types/api";

describe("storagePolicyDialogShared", () => {
	const t = (key: string) => key;

	it("detects Tencent COS endpoints for generic S3 driver promotion", () => {
		expect(isTencentCosEndpoint("https://cos.ap-guangzhou.myqcloud.com")).toBe(
			true,
		);
		expect(
			isTencentCosEndpoint(
				"https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com",
			),
		).toBe(true);
		expect(isTencentCosEndpoint("https://s3.amazonaws.com")).toBe(false);
		expect(isTencentCosEndpoint("not a url")).toBe(false);

		const labelFor = (driverType: "tencent_cos") =>
			driverType === "tencent_cos" ? "Tencent COS" : driverType;
		expect(
			getS3CompatibleDriverPromotionTarget(
				{
					driver_type: "s3",
					endpoint: "https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com",
				},
				labelFor,
			),
		).toEqual({
			driverLabel: "Tencent COS",
			driverType: "tencent_cos",
		});
		expect(
			getS3CompatibleDriverPromotionTarget(
				{
					driver_type: "s3",
					endpoint: "https://s3.amazonaws.com",
				},
				labelFor,
			),
		).toBeNull();
		expect(
			getS3CompatibleDriverPromotionTarget(
				{
					driver_type: "tencent_cos",
					endpoint: "https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com",
				},
				labelFor,
			),
		).toBeNull();
		expect(getS3CompatibleDriverPromotionTarget(null, labelFor)).toBeNull();
		expect(isS3CompatibleDriver("s3")).toBe(true);
		expect(isS3CompatibleDriver("tencent_cos")).toBe(true);
		expect(isS3CompatibleDriver("azure_blob")).toBe(false);
		expect(isObjectStorageDriver("azure_blob")).toBe(true);
	});

	it("maps an existing policy into form state", () => {
		expect(
			getPolicyForm({
				id: 3,
				name: "Archive",
				driver_type: "local",
				endpoint: "",
				bucket: "",
				access_key: "",
				secret_key: "",
				base_path: "/data/archive",
				remote_node_id: null,
				max_file_size: 1024,
				allowed_types: [],
				options: {
					content_dedup: true,
					storage_native_processing_enabled: true,
					thumbnail_processor: "storage_native",
					thumbnail_extensions: ["png", "jpg"],
				},
				is_default: true,
				chunk_size: 10 * 1024 * 1024,
				created_at: "",
				updated_at: "",
			} as StoragePolicy),
		).toEqual({
			name: "Archive",
			driver_type: "local",
			endpoint: "",
			bucket: "",
			access_key: "",
			secret_key: "",
			base_path: "/data/archive",
			remote_node_id: "",
			max_file_size: "1024",
			chunk_size: "10",
			is_default: true,
			content_dedup: true,
			remote_download_strategy: "relay_stream",
			remote_upload_strategy: "relay_stream",
			s3_upload_strategy: "relay_stream",
			s3_download_strategy: "relay_stream",
			storage_native_processing_enabled: true,
			storage_native_media_metadata_enabled: false,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["png", "jpg"],
			media_metadata_extensions: [],
		});
	});

	it("builds create payloads with trimmed S3 fields", () => {
		expect(
			buildCreatePolicyPayload({
				name: "Media",
				driver_type: "s3",
				endpoint: " https://s3.example.test/custom/path ",
				bucket: " photos ",
				access_key: "AKIA",
				secret_key: "SECRET",
				base_path: "videos",
				remote_node_id: "",
				max_file_size: "2048",
				chunk_size: "6",
				is_default: false,
				content_dedup: false,
				remote_download_strategy: "relay_stream",
				remote_upload_strategy: "relay_stream",
				s3_upload_strategy: "presigned",
				s3_download_strategy: "relay_stream",
				storage_native_processing_enabled: false,
				thumbnail_processor: null,
				thumbnail_extensions: [],
			}),
		).toEqual({
			name: "Media",
			driver_type: "s3",
			endpoint: "https://s3.example.test/custom/path",
			bucket: "photos",
			access_key: "AKIA",
			secret_key: "SECRET",
			base_path: "videos",
			max_file_size: 2048,
			chunk_size: 6 * 1024 * 1024,
			is_default: false,
			options: {
				s3_upload_strategy: "presigned",
				s3_download_strategy: "relay_stream",
			},
		});

		expect(
			buildCreatePolicyPayload({
				name: "Virtual Hosted S3",
				driver_type: "s3",
				endpoint: "https://s3.example.test",
				bucket: "photos",
				access_key: "AKIA",
				secret_key: "SECRET",
				base_path: "",
				remote_node_id: "",
				max_file_size: "",
				chunk_size: "5",
				is_default: false,
				content_dedup: false,
				remote_download_strategy: "relay_stream",
				remote_upload_strategy: "relay_stream",
				s3_upload_strategy: "relay_stream",
				s3_download_strategy: "relay_stream",
				s3_path_style: false,
				storage_native_processing_enabled: false,
				thumbnail_processor: null,
				thumbnail_extensions: [],
			}).options,
		).toEqual({
			s3_upload_strategy: "relay_stream",
			s3_download_strategy: "relay_stream",
			s3_path_style: false,
		});
	});

	it("validates S3-compatible endpoint protocols without blocking remote policies", () => {
		const baseForm = {
			name: "Media",
			driver_type: "s3" as const,
			endpoint: "s3.example.com",
			bucket: "bucket-a",
			access_key: "",
			secret_key: "",
			base_path: "",
			remote_node_id: "",
			max_file_size: "",
			chunk_size: "5",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "relay_stream" as const,
			remote_upload_strategy: "relay_stream" as const,
			s3_upload_strategy: "relay_stream" as const,
			s3_download_strategy: "relay_stream" as const,
			storage_native_processing_enabled: false,
			thumbnail_processor: null,
			thumbnail_extensions: [],
			storage_native_media_metadata_enabled: false,
			media_metadata_extensions: [],
		};

		expect(getEndpointValidationMessage(baseForm, t)).toBe(
			"s3_endpoint_protocol_required_error",
		);
		expect(
			getEndpointValidationMessage(
				{ ...baseForm, endpoint: "http://s3.example.com" },
				t,
			),
		).toBeNull();
		expect(
			getEndpointValidationMessage(
				{ ...baseForm, endpoint: "https://s3.example.com" },
				t,
			),
		).toBeNull();
		expect(
			getEndpointValidationMessage(
				{
					...baseForm,
					driver_type: "tencent_cos",
					endpoint: "cos.ap-guangzhou.myqcloud.com",
				},
				t,
			),
		).toBe("s3_endpoint_protocol_required_error");
		expect(
			getEndpointValidationMessage(
				{
					...baseForm,
					driver_type: "remote",
					endpoint: "edge-node-without-protocol",
				},
				t,
			),
		).toBeNull();
	});

	it("validates Azure Blob endpoints separately from S3-compatible drivers", () => {
		const baseForm = {
			name: "Azure Blob",
			driver_type: "azure_blob" as const,
			endpoint: "acct.blob.core.windows.net",
			bucket: "container-a",
			access_key: "account-name",
			secret_key: "account-key",
			base_path: "",
			remote_node_id: "",
			max_file_size: "",
			chunk_size: "5",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "relay_stream" as const,
			remote_upload_strategy: "relay_stream" as const,
			s3_upload_strategy: "presigned" as const,
			s3_download_strategy: "relay_stream" as const,
			storage_native_processing_enabled: false,
			thumbnail_processor: null,
			thumbnail_extensions: [],
			storage_native_media_metadata_enabled: false,
			media_metadata_extensions: [],
		};

		expect(getEndpointValidationMessage(baseForm, t)).toBe(
			"azure_blob_endpoint_protocol_required_error",
		);
		expect(
			getEndpointValidationMessage(
				{ ...baseForm, endpoint: "https://acct.blob.core.windows.net" },
				t,
			),
		).toBeNull();
		expect(
			getEndpointValidationMessage(
				{ ...baseForm, endpoint: "ftp://acct.blob.core.windows.net" },
				t,
			),
		).toBe("azure_blob_endpoint_protocol_required_error");
		expect(
			getEndpointValidationMessage(
				{
					...baseForm,
					driver_type: "s3",
					endpoint: "ftp://s3.example.com",
				},
				t,
			),
		).toBe("s3_endpoint_protocol_required_error");
	});

	it("builds Tencent COS CORS draft action payloads with optional saved policy reuse", () => {
		const form = {
			name: "COS Media",
			driver_type: "tencent_cos" as const,
			endpoint: " https://cos.ap-guangzhou.myqcloud.com ",
			bucket: " media-1250000000 ",
			access_key: "",
			secret_key: "",
			base_path: "tenant-a",
			remote_node_id: "",
			max_file_size: "",
			chunk_size: "5",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "relay_stream" as const,
			remote_upload_strategy: "relay_stream" as const,
			s3_upload_strategy: "presigned" as const,
			s3_download_strategy: "presigned" as const,
			storage_native_processing_enabled: true,
			thumbnail_processor: "storage_native" as const,
			thumbnail_extensions: [" .PNG ", "jpg"],
			storage_native_media_metadata_enabled: true,
			media_metadata_extensions: [" mp4 "],
		};

		expect(buildTencentCosCorsPayload(form, 34)).toEqual({
			action: "configure_tencent_cos_cors",
			policy_id: 34,
			driver_type: "tencent_cos",
			endpoint: "https://cos.ap-guangzhou.myqcloud.com",
			bucket: "media-1250000000",
			access_key: undefined,
			secret_key: undefined,
			base_path: "tenant-a",
			remote_node_id: undefined,
			options: {
				s3_upload_strategy: "presigned",
				s3_download_strategy: "presigned",
				storage_native_processing_enabled: true,
				thumbnail_processor: "storage_native",
				thumbnail_extensions: ["png", "jpg"],
				storage_native_media_metadata_enabled: true,
				media_metadata_extensions: ["mp4"],
			},
		});
		expect(buildTencentCosCorsPayload(form, null).policy_id).toBeUndefined();
	});

	it("omits empty credentials from update payloads", () => {
		expect(
			buildUpdatePolicyPayload({
				name: "Media",
				driver_type: "s3",
				endpoint: "https://example.com",
				bucket: "bucket-a",
				access_key: "",
				secret_key: "",
				base_path: "videos",
				remote_node_id: "",
				max_file_size: "",
				chunk_size: "5",
				is_default: true,
				content_dedup: false,
				remote_download_strategy: "relay_stream",
				remote_upload_strategy: "relay_stream",
				s3_upload_strategy: "relay_stream",
				s3_download_strategy: "presigned",
				storage_native_processing_enabled: false,
				thumbnail_processor: null,
				thumbnail_extensions: [],
			}),
		).toEqual({
			name: "Media",
			endpoint: "https://example.com",
			bucket: "bucket-a",
			base_path: "videos",
			max_file_size: undefined,
			chunk_size: 5 * 1024 * 1024,
			is_default: true,
			options: {
				s3_upload_strategy: "relay_stream",
				s3_download_strategy: "presigned",
			},
		});
	});

	it("builds remote payloads with remote node binding only", () => {
		expect(
			buildCreatePolicyPayload({
				name: "Remote Edge",
				driver_type: "remote",
				endpoint: "",
				bucket: "",
				access_key: "",
				secret_key: "",
				base_path: "tenant-a/uploads",
				remote_node_id: "9",
				max_file_size: "",
				chunk_size: "4",
				is_default: false,
				content_dedup: false,
				remote_download_strategy: "presigned",
				remote_upload_strategy: "presigned",
				s3_upload_strategy: "relay_stream",
				s3_download_strategy: "relay_stream",
				storage_native_processing_enabled: false,
				thumbnail_processor: null,
				thumbnail_extensions: [],
			}),
		).toEqual({
			name: "Remote Edge",
			driver_type: "remote",
			endpoint: "",
			bucket: "",
			access_key: "",
			secret_key: "",
			base_path: "tenant-a/uploads",
			remote_node_id: 9,
			max_file_size: undefined,
			chunk_size: 4 * 1024 * 1024,
			is_default: false,
			options: {
				remote_download_strategy: "presigned",
				remote_upload_strategy: "presigned",
			},
		});

		expect(
			buildPolicyTestPayload({
				name: "Remote Edge",
				driver_type: "remote",
				endpoint: "",
				bucket: "",
				access_key: "",
				secret_key: "",
				base_path: "tenant-a/uploads",
				remote_node_id: "9",
				max_file_size: "",
				chunk_size: "4",
				is_default: false,
				content_dedup: false,
				remote_download_strategy: "presigned",
				remote_upload_strategy: "presigned",
				s3_upload_strategy: "relay_stream",
				s3_download_strategy: "relay_stream",
				storage_native_processing_enabled: false,
				thumbnail_processor: null,
				thumbnail_extensions: [],
			}),
		).toEqual({
			driver_type: "remote",
			endpoint: undefined,
			bucket: undefined,
			access_key: undefined,
			secret_key: undefined,
			base_path: "tenant-a/uploads",
			remote_node_id: 9,
			options: {
				remote_download_strategy: "presigned",
				remote_upload_strategy: "presigned",
			},
		});
	});

	it("builds Azure Blob payloads with object-storage options but without S3 path style", () => {
		const azureForm = {
			name: "Azure Archive",
			driver_type: "azure_blob" as const,
			endpoint: " https://acct.blob.core.windows.net/ ",
			bucket: " container-a ",
			access_key: " account-name ",
			secret_key: " account-key ",
			base_path: "archives",
			remote_node_id: "",
			max_file_size: "",
			chunk_size: "8",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "relay_stream" as const,
			remote_upload_strategy: "relay_stream" as const,
			s3_upload_strategy: "presigned" as const,
			s3_download_strategy: "relay_stream" as const,
			s3_path_style: false,
			storage_native_processing_enabled: false,
			thumbnail_processor: null,
			thumbnail_extensions: [],
			storage_native_media_metadata_enabled: false,
			media_metadata_extensions: [],
		};
		const payload = buildCreatePolicyPayload({
			...azureForm,
		});

		expect(payload).toEqual({
			name: "Azure Archive",
			driver_type: "azure_blob",
			endpoint: "https://acct.blob.core.windows.net/",
			bucket: "container-a",
			access_key: "account-name",
			secret_key: "account-key",
			base_path: "archives",
			remote_node_id: undefined,
			max_file_size: undefined,
			chunk_size: 8 * 1024 * 1024,
			is_default: false,
			options: {
				s3_upload_strategy: "presigned",
				s3_download_strategy: "relay_stream",
			},
		});
		expect(normalizePolicyForm(azureForm)).toEqual({
			...azureForm,
			endpoint: "https://acct.blob.core.windows.net/",
			bucket: "container-a",
			access_key: "account-name",
			secret_key: "account-key",
		});
		expect(getPolicyConnectionTestKey(azureForm)).toBe(
			JSON.stringify({
				driver_type: "azure_blob",
				endpoint: "https://acct.blob.core.windows.net/",
				bucket: "container-a",
				access_key: "account-name",
				secret_key: "account-key",
				base_path: "archives",
				remote_node_id: undefined,
				options: {
					s3_upload_strategy: "presigned",
					s3_download_strategy: "relay_stream",
				},
			}),
		);

		expect(
			hasConnectionFieldChanges(
				{
					...getPolicyForm({
						id: 15,
						name: "Azure Archive",
						driver_type: "azure_blob",
						endpoint: "https://acct.blob.core.windows.net",
						bucket: "container-a",
						access_key: "",
						secret_key: "",
						base_path: "archives",
						remote_node_id: null,
						max_file_size: null,
						allowed_types: [],
						options: {
							s3_upload_strategy: "presigned",
							s3_download_strategy: "relay_stream",
						},
						is_default: false,
						chunk_size: 8 * 1024 * 1024,
						created_at: "",
						updated_at: "",
					} as StoragePolicy),
					s3_upload_strategy: "relay_stream",
				},
				{
					id: 15,
					name: "Azure Archive",
					driver_type: "azure_blob",
					endpoint: "https://acct.blob.core.windows.net",
					bucket: "container-a",
					access_key: "",
					secret_key: "",
					base_path: "archives",
					remote_node_id: null,
					max_file_size: null,
					allowed_types: [],
					options: {
						s3_upload_strategy: "presigned",
						s3_download_strategy: "relay_stream",
					},
					is_default: false,
					chunk_size: 8 * 1024 * 1024,
					created_at: "",
					updated_at: "",
				} as StoragePolicy,
			),
		).toBe(false);
	});

	it("preserves policy-level thumbnail options in create and update payloads", () => {
		const form = {
			name: "Native Thumbnails",
			driver_type: "remote" as const,
			endpoint: "",
			bucket: "",
			access_key: "",
			secret_key: "",
			base_path: "tenant-a/uploads",
			remote_node_id: "9",
			max_file_size: "",
			chunk_size: "4",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "presigned" as const,
			remote_upload_strategy: "presigned" as const,
			s3_upload_strategy: "relay_stream" as const,
			s3_download_strategy: "relay_stream" as const,
			storage_native_processing_enabled: true,
			thumbnail_processor: "storage_native" as const,
			thumbnail_extensions: ["png", "jpg"],
		};

		expect(buildCreatePolicyPayload(form).options).toEqual({
			remote_download_strategy: "presigned",
			remote_upload_strategy: "presigned",
			storage_native_processing_enabled: true,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["png", "jpg"],
		});
		expect(buildUpdatePolicyPayload(form).options).toEqual({
			remote_download_strategy: "presigned",
			remote_upload_strategy: "presigned",
			storage_native_processing_enabled: true,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["png", "jpg"],
		});
	});

	it("keeps storage-native thumbnail suffixes independent per policy", () => {
		const baseForm = {
			name: "COS Native",
			driver_type: "tencent_cos" as const,
			endpoint: "https://cos.ap-guangzhou.myqcloud.com",
			bucket: "bucket-1250000000",
			access_key: "AKID",
			secret_key: "SECRET",
			base_path: "",
			remote_node_id: "",
			max_file_size: "",
			chunk_size: "5",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "relay_stream" as const,
			remote_upload_strategy: "relay_stream" as const,
			s3_upload_strategy: "relay_stream" as const,
			s3_download_strategy: "relay_stream" as const,
			storage_native_processing_enabled: true,
			thumbnail_processor: "storage_native" as const,
			thumbnail_extensions: [" .PNG ", "jpg", ".png", "../../etc/passwd"],
		};

		expect(buildCreatePolicyPayload(baseForm).options).toEqual({
			s3_upload_strategy: "relay_stream",
			s3_download_strategy: "relay_stream",
			storage_native_processing_enabled: true,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["png", "jpg"],
		});
		expect(
			buildCreatePolicyPayload({
				...baseForm,
				name: "COS WebP",
				thumbnail_extensions: ["webp", "gif"],
			}).options,
		).toEqual({
			s3_upload_strategy: "relay_stream",
			s3_download_strategy: "relay_stream",
			storage_native_processing_enabled: true,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["webp", "gif"],
		});
	});

	it("does not persist any storage-native options when storage-native processing is disabled", () => {
		const payload = buildCreatePolicyPayload({
			name: "Plain S3",
			driver_type: "s3",
			endpoint: "https://s3.example.com",
			bucket: "bucket",
			access_key: "AKID",
			secret_key: "SECRET",
			base_path: "",
			remote_node_id: "",
			max_file_size: "",
			chunk_size: "5",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "relay_stream",
			remote_upload_strategy: "relay_stream",
			s3_upload_strategy: "relay_stream",
			s3_download_strategy: "relay_stream",
			storage_native_processing_enabled: false,
			storage_native_media_metadata_enabled: true,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["png"],
			media_metadata_extensions: ["mp4"],
		});

		expect(payload.options).toEqual({
			s3_upload_strategy: "relay_stream",
			s3_download_strategy: "relay_stream",
		});
	});

	it("preserves storage-native media metadata switch with empty suffixes", () => {
		const payload = buildCreatePolicyPayload({
			name: "COS Metadata",
			driver_type: "tencent_cos",
			endpoint: "https://cos.ap-guangzhou.myqcloud.com",
			bucket: "bucket-1250000000",
			access_key: "AKID",
			secret_key: "SECRET",
			base_path: "",
			remote_node_id: "",
			max_file_size: "",
			chunk_size: "5",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "relay_stream",
			remote_upload_strategy: "relay_stream",
			s3_upload_strategy: "relay_stream",
			s3_download_strategy: "relay_stream",
			storage_native_processing_enabled: true,
			storage_native_media_metadata_enabled: true,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["jpg"],
			media_metadata_extensions: [],
		});

		expect(payload.options).toEqual({
			s3_upload_strategy: "relay_stream",
			s3_download_strategy: "relay_stream",
			storage_native_processing_enabled: true,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["jpg"],
			storage_native_media_metadata_enabled: true,
		});
	});

	it("normalizes storage-native media metadata suffixes per policy", () => {
		const payload = buildCreatePolicyPayload({
			name: "COS Metadata",
			driver_type: "tencent_cos",
			endpoint: "https://cos.ap-guangzhou.myqcloud.com",
			bucket: "bucket-1250000000",
			access_key: "AKID",
			secret_key: "SECRET",
			base_path: "",
			remote_node_id: "",
			max_file_size: "",
			chunk_size: "5",
			is_default: false,
			content_dedup: false,
			remote_download_strategy: "relay_stream",
			remote_upload_strategy: "relay_stream",
			s3_upload_strategy: "relay_stream",
			s3_download_strategy: "relay_stream",
			storage_native_processing_enabled: true,
			storage_native_media_metadata_enabled: true,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["jpg"],
			media_metadata_extensions: [
				" .MP4 ",
				"mp4",
				".Mov",
				"",
				"../../etc/passwd",
			],
		});

		expect(payload.options).toMatchObject({
			storage_native_media_metadata_enabled: true,
			media_metadata_extensions: ["mp4", "mov"],
		});
	});

	it("detects connection field changes per driver without treating policy options as connection changes", () => {
		const localPolicy = {
			id: 11,
			name: "Local",
			driver_type: "local",
			endpoint: "",
			bucket: "",
			access_key: "",
			secret_key: "",
			base_path: "/srv/data",
			remote_node_id: null,
			max_file_size: null,
			allowed_types: [],
			options: {},
			is_default: false,
			chunk_size: 5 * 1024 * 1024,
			created_at: "",
			updated_at: "",
		} as StoragePolicy;
		const localForm = getPolicyForm(localPolicy);

		expect(
			buildCreatePolicyPayload({
				...localForm,
				content_dedup: true,
			}).options,
		).toEqual({ content_dedup: true });
		expect(hasConnectionFieldChanges(localForm, null)).toBe(true);
		expect(hasConnectionFieldChanges(localForm, localPolicy)).toBe(false);
		expect(
			hasConnectionFieldChanges(
				{ ...localForm, base_path: "/srv/other" },
				localPolicy,
			),
		).toBe(true);

		const remotePolicy = {
			...localPolicy,
			id: 12,
			name: "Remote",
			driver_type: "remote",
			base_path: "tenant-a",
			remote_node_id: 9,
			options: {
				remote_download_strategy: "presigned",
				remote_upload_strategy: "presigned",
			},
		} as StoragePolicy;
		const remoteForm = getPolicyForm(remotePolicy);

		expect(hasConnectionFieldChanges(remoteForm, remotePolicy)).toBe(false);
		expect(
			hasConnectionFieldChanges(
				{ ...remoteForm, remote_node_id: "10" },
				remotePolicy,
			),
		).toBe(true);

		const s3Policy = {
			...localPolicy,
			id: 13,
			name: "S3",
			driver_type: "s3",
			endpoint: "https://s3.example.com",
			bucket: "media",
			base_path: "uploads",
			options: {
				s3_download_strategy: "presigned",
				s3_upload_strategy: "presigned",
			},
		} as StoragePolicy;
		const s3Form = getPolicyForm(s3Policy);

		expect(hasConnectionFieldChanges(s3Form, s3Policy)).toBe(false);
		expect(
			hasConnectionFieldChanges({ ...s3Form, secret_key: "SECRET" }, s3Policy),
		).toBe(true);
	});
});
