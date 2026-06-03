import { describe, expect, it } from "vitest";
import {
	buildCreatePolicyPayload,
	buildPolicyTestPayload,
	buildUpdatePolicyPayload,
	formatGoogleDriveLocationLabel,
	getPolicyForm,
	hasConnectionFieldChanges,
} from "@/components/admin/storagePolicyDialogShared";
import type { StoragePolicy } from "@/types/api";

describe("storagePolicyDialogShared", () => {
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
			google_drive_upload_strategy: "resumable",
			google_drive_root_folder_id: "",
			google_drive_shared_drive_id: "",
			google_drive_use_app_data_folder: false,
			thumbnail_processor: "storage_native",
			thumbnail_extensions: ["png", "jpg"],
			media_metadata_extensions: [],
		});
	});

	it("builds create payloads with normalized S3 fields", () => {
		expect(
			buildCreatePolicyPayload({
				name: "Media",
				driver_type: "s3",
				endpoint: "https://demo.r2.cloudflarestorage.com/photos",
				bucket: "",
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
				google_drive_upload_strategy: "resumable",
				google_drive_root_folder_id: "",
				google_drive_shared_drive_id: "",
				google_drive_use_app_data_folder: false,
				thumbnail_processor: null,
				thumbnail_extensions: [],
			}),
		).toEqual({
			name: "Media",
			driver_type: "s3",
			endpoint: "https://demo.r2.cloudflarestorage.com",
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
				google_drive_upload_strategy: "resumable",
				google_drive_root_folder_id: "",
				google_drive_shared_drive_id: "",
				google_drive_use_app_data_folder: false,
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
				google_drive_upload_strategy: "resumable",
				google_drive_root_folder_id: "",
				google_drive_shared_drive_id: "",
				google_drive_use_app_data_folder: false,
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
				google_drive_upload_strategy: "resumable",
				google_drive_root_folder_id: "",
				google_drive_shared_drive_id: "",
				google_drive_use_app_data_folder: false,
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
		});
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
			google_drive_upload_strategy: "resumable" as const,
			google_drive_root_folder_id: "",
			google_drive_shared_drive_id: "",
			google_drive_use_app_data_folder: false,
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

	it("builds Google Drive payloads with policy-level OAuth options", () => {
		expect(
			buildCreatePolicyPayload({
				name: "Drive",
				driver_type: "google_drive",
				endpoint: "",
				bucket: "",
				access_key: "client-id",
				secret_key: "client-secret",
				base_path: "drive-prefix",
				remote_node_id: "",
				max_file_size: "",
				chunk_size: "8",
				is_default: false,
				content_dedup: false,
				remote_download_strategy: "relay_stream",
				remote_upload_strategy: "relay_stream",
				s3_upload_strategy: "relay_stream",
				s3_download_strategy: "relay_stream",
				storage_native_processing_enabled: false,
				google_drive_upload_strategy: "resumable",
				google_drive_root_folder_id: " folder-1 ",
				google_drive_shared_drive_id: " shared-drive-1 ",
				google_drive_use_app_data_folder: false,
				thumbnail_processor: null,
				thumbnail_extensions: [],
			}),
		).toEqual({
			name: "Drive",
			driver_type: "google_drive",
			endpoint: "",
			bucket: "",
			access_key: "client-id",
			secret_key: "client-secret",
			base_path: "drive-prefix",
			max_file_size: undefined,
			chunk_size: 8 * 1024 * 1024,
			is_default: false,
			options: {
				google_drive_upload_strategy: "resumable",
				google_drive_root_folder_id: "folder-1",
				google_drive_shared_drive_id: "shared-drive-1",
			},
		});
	});

	it("formats Google Drive shared drive locations with an optional root folder", () => {
		expect(
			formatGoogleDriveLocationLabel({
				rootFolderId: "folder-1",
				rootLabel: "root",
				sharedDriveId: "shared-drive-1",
				useAppDataFolder: false,
			}),
		).toBe("shared-drive-1 / folder-1");
		expect(
			formatGoogleDriveLocationLabel({
				rootLabel: "root",
				sharedDriveId: "shared-drive-1",
				useAppDataFolder: false,
			}),
		).toBe("shared-drive-1 / root");
		expect(
			formatGoogleDriveLocationLabel({
				rootFolderId: "folder-1",
				rootLabel: "root",
				sharedDriveId: "shared-drive-1",
				useAppDataFolder: true,
			}),
		).toBe("appDataFolder");
	});
});
