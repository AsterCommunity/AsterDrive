import type {
	DriverType,
	MicrosoftGraphCloud,
	ObjectStorageDownloadStrategy,
	ObjectStorageUploadStrategy,
	OneDriveAccountMode,
	ProviderResumableUploadStrategy,
	RemoteDownloadStrategy,
	RemoteUploadStrategy,
	StoragePolicy,
	StoragePolicyOptions,
} from "@/types/api";
import type { StorageApplicationCredentialForm } from "./applicationCredentials";
import {
	getEffectiveObjectStorageDownloadStrategy,
	getEffectiveObjectStorageUploadStrategy,
	getEffectiveProviderResumableUploadStrategy,
	getEffectiveRemoteDownloadStrategy,
	getEffectiveRemoteUploadStrategy,
	getEffectiveS3PathStyle,
} from "./storagePolicyOptions";

export const DEFAULT_STORAGE_NATIVE_THUMBNAIL_EXTENSIONS = [
	"jpg",
	"jpeg",
	"png",
	"webp",
	"gif",
];

export interface PolicyFormData {
	name: string;
	driver_type: DriverType;
	endpoint: string;
	bucket: string;
	access_key: string;
	secret_key: string;
	base_path: string;
	remote_node_id: string;
	remote_storage_target_key: string;
	max_file_size: string;
	chunk_size: string;
	is_default: boolean;
	content_dedup: boolean;
	remote_download_strategy: RemoteDownloadStrategy;
	remote_upload_strategy: RemoteUploadStrategy;
	object_storage_upload_strategy: ObjectStorageUploadStrategy;
	object_storage_download_strategy: ObjectStorageDownloadStrategy;
	provider_resumable_upload_strategy: ProviderResumableUploadStrategy;
	s3_path_style?: boolean;
	onedrive_cloud: MicrosoftGraphCloud;
	onedrive_account_mode: OneDriveAccountMode;
	onedrive_tenant: string;
	onedrive_drive_id: string;
	onedrive_root_item_id: string;
	onedrive_site_id: string;
	onedrive_group_id: string;
	policy_option_values?: Record<string, string>;
	application_credentials: StorageApplicationCredentialForm;
	storage_native_processing_enabled: boolean;
	storage_native_media_metadata_enabled?: boolean;
	thumbnail_processor: StoragePolicyOptions["thumbnail_processor"];
	thumbnail_extensions: string[];
	media_metadata_extensions?: string[];
}

export function getPolicyForm(policy: StoragePolicy): PolicyFormData {
	const options = policy.options;

	return {
		name: policy.name,
		driver_type: policy.driver_type,
		endpoint: policy.endpoint,
		bucket: policy.bucket,
		access_key: "",
		secret_key: "",
		base_path: policy.base_path,
		remote_node_id:
			policy.remote_node_id != null ? String(policy.remote_node_id) : "",
		remote_storage_target_key: policy.remote_storage_target_key ?? "",
		max_file_size:
			policy.max_file_size != null ? String(policy.max_file_size) : "",
		chunk_size:
			policy.chunk_size != null
				? String(Math.round(policy.chunk_size / 1024 / 1024))
				: "5",
		is_default: policy.is_default,
		content_dedup: options.content_dedup === true,
		remote_download_strategy: getEffectiveRemoteDownloadStrategy(options),
		remote_upload_strategy: getEffectiveRemoteUploadStrategy(options),
		object_storage_upload_strategy:
			getEffectiveObjectStorageUploadStrategy(options),
		object_storage_download_strategy:
			getEffectiveObjectStorageDownloadStrategy(options),
		provider_resumable_upload_strategy:
			getEffectiveProviderResumableUploadStrategy(options),
		s3_path_style: getEffectiveS3PathStyle(options),
		onedrive_cloud: options.onedrive_cloud ?? "global",
		onedrive_account_mode: options.onedrive_account_mode ?? "work_or_school",
		onedrive_tenant: options.onedrive_tenant ?? "common",
		onedrive_drive_id: options.onedrive_drive_id ?? "",
		onedrive_root_item_id: options.onedrive_root_item_id ?? "",
		onedrive_site_id: options.onedrive_site_id ?? "",
		onedrive_group_id: options.onedrive_group_id ?? "",
		policy_option_values: scalarPolicyOptionValues(options),
		application_credentials: {
			microsoft_graph: {
				cloud: options.onedrive_cloud ?? "global",
				tenant: options.onedrive_tenant ?? "common",
				client_id: "",
				client_secret: "",
				scopes: "",
			},
		},
		storage_native_processing_enabled:
			options.storage_native_processing_enabled === true,
		thumbnail_processor:
			options.storage_native_processing_enabled === true
				? (options.thumbnail_processor ?? null)
				: null,
		thumbnail_extensions:
			options.storage_native_processing_enabled === true
				? (options.thumbnail_extensions ?? [])
				: [],
		storage_native_media_metadata_enabled:
			options.storage_native_processing_enabled === true &&
			options.storage_native_media_metadata_enabled === true,
		media_metadata_extensions:
			options.storage_native_processing_enabled === true
				? (options.media_metadata_extensions ?? [])
				: [],
	};
}

export const emptyForm: PolicyFormData = {
	name: "",
	driver_type: "local",
	endpoint: "",
	bucket: "",
	access_key: "",
	secret_key: "",
	base_path: "",
	remote_node_id: "",
	remote_storage_target_key: "",
	max_file_size: "",
	chunk_size: "5",
	is_default: false,
	content_dedup: false,
	remote_download_strategy: "relay_stream",
	remote_upload_strategy: "relay_stream",
	object_storage_upload_strategy: "relay_stream",
	object_storage_download_strategy: "relay_stream",
	provider_resumable_upload_strategy: "server_relay",
	s3_path_style: true,
	onedrive_cloud: "global",
	onedrive_account_mode: "work_or_school",
	onedrive_tenant: "common",
	onedrive_drive_id: "",
	onedrive_root_item_id: "",
	onedrive_site_id: "",
	onedrive_group_id: "",
	policy_option_values: {},
	application_credentials: {
		microsoft_graph: {
			cloud: "global",
			tenant: "common",
			client_id: "",
			client_secret: "",
			scopes: "",
		},
	},
	storage_native_processing_enabled: false,
	storage_native_media_metadata_enabled: false,
	thumbnail_processor: null,
	thumbnail_extensions: [],
	media_metadata_extensions: [],
};

function scalarPolicyOptionValues(
	options: StoragePolicyOptions,
): Record<string, string> {
	const values: Record<string, string> = {};
	for (const [key, value] of Object.entries(options)) {
		if (
			typeof value === "string" ||
			typeof value === "number" ||
			typeof value === "boolean"
		) {
			values[key] = String(value);
		}
	}
	return values;
}
