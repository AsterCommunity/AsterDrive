import type {
	DriverType,
	MicrosoftGraphCloud,
	ObjectStorageDownloadStrategy,
	ObjectStorageUploadStrategy,
	OneDriveAccountMode,
	ProviderDownloadStrategy,
	ProviderResumableUploadStrategy,
	RemoteDownloadStrategy,
	RemoteUploadStrategy,
	StorageConnectorDescriptor,
	StoragePolicyOptions,
} from "@/types/api";

export interface StoragePolicyOptionsForm {
	driver_type: DriverType;
	content_dedup: boolean;
	remote_download_strategy: RemoteDownloadStrategy;
	remote_upload_strategy: RemoteUploadStrategy;
	object_storage_upload_strategy: ObjectStorageUploadStrategy;
	object_storage_download_strategy: ObjectStorageDownloadStrategy;
	provider_resumable_upload_strategy: ProviderResumableUploadStrategy;
	provider_download_strategy: ProviderDownloadStrategy;
	s3_path_style?: boolean;
	onedrive_cloud: MicrosoftGraphCloud;
	onedrive_account_mode: OneDriveAccountMode;
	onedrive_tenant: string;
	onedrive_drive_id: string;
	onedrive_root_item_id: string;
	onedrive_site_id: string;
	onedrive_group_id: string;
	policy_option_values?: Record<string, string>;
	storage_native_processing_enabled: boolean;
	storage_native_media_metadata_enabled?: boolean;
	thumbnail_processor: StoragePolicyOptions["thumbnail_processor"];
	thumbnail_extensions: string[];
	media_metadata_extensions?: string[];
}

type LegacyStoragePolicyOptions = StoragePolicyOptions & {
	s3_upload_strategy?: ObjectStorageUploadStrategy | null;
	s3_download_strategy?: ObjectStorageDownloadStrategy | null;
};

export function getEffectiveObjectStorageUploadStrategy(
	options: StoragePolicyOptions,
): ObjectStorageUploadStrategy {
	const legacyOptions = options as LegacyStoragePolicyOptions;
	return (
		options.object_storage_upload_strategy ??
		legacyOptions.s3_upload_strategy ??
		"relay_stream"
	);
}

export function getEffectiveObjectStorageDownloadStrategy(
	options: StoragePolicyOptions,
): ObjectStorageDownloadStrategy {
	const legacyOptions = options as LegacyStoragePolicyOptions;
	return (
		options.object_storage_download_strategy ??
		legacyOptions.s3_download_strategy ??
		"relay_stream"
	);
}

export function getEffectiveS3PathStyle(options: StoragePolicyOptions) {
	return options.s3_path_style ?? true;
}

export function getEffectiveRemoteDownloadStrategy(
	options: StoragePolicyOptions,
): RemoteDownloadStrategy {
	return options.remote_download_strategy ?? "relay_stream";
}

export function getEffectiveRemoteUploadStrategy(
	options: StoragePolicyOptions,
): RemoteUploadStrategy {
	return options.remote_upload_strategy ?? "relay_stream";
}

export function getEffectiveProviderResumableUploadStrategy(
	options: StoragePolicyOptions,
): ProviderResumableUploadStrategy {
	return options.provider_resumable_upload_strategy ?? "server_relay";
}

export function getEffectiveProviderDownloadStrategy(
	options: StoragePolicyOptions,
): ProviderDownloadStrategy {
	return options.provider_download_strategy ?? "server_relay";
}

export function buildPolicyOptions(
	form: StoragePolicyOptionsForm,
	descriptor?: StorageConnectorDescriptor | null,
): StoragePolicyOptions {
	if (descriptor) {
		return {
			...buildDescriptorPolicyOptions(form, descriptor),
			...buildStorageNativeOptions(form),
		};
	}

	return {
		...buildPolicyOptionsFallback(form),
		...buildStorageNativeOptions(form),
	};
}

function buildDescriptorPolicyOptions(
	form: StoragePolicyOptionsForm,
	descriptor: StorageConnectorDescriptor,
): StoragePolicyOptions {
	const hasOption = (name: string) =>
		descriptor.fields.some(
			(field) => field.scope === "policy_options" && field.name === name,
		);
	const options: StoragePolicyOptions = {};

	if (hasOption("content_dedup") && form.content_dedup) {
		options.content_dedup = true;
	}
	if (hasOption("remote_download_strategy")) {
		options.remote_download_strategy = form.remote_download_strategy;
	}
	if (hasOption("remote_upload_strategy")) {
		options.remote_upload_strategy = form.remote_upload_strategy;
	}
	if (hasOption("provider_resumable_upload_strategy")) {
		options.provider_resumable_upload_strategy =
			form.provider_resumable_upload_strategy;
	}
	if (hasOption("provider_download_strategy")) {
		options.provider_download_strategy = form.provider_download_strategy;
	}
	if (
		hasOption("object_storage_upload_strategy") ||
		hasOption("s3_upload_strategy")
	) {
		options.object_storage_upload_strategy =
			form.object_storage_upload_strategy;
	}
	if (
		hasOption("object_storage_download_strategy") ||
		hasOption("s3_download_strategy")
	) {
		options.object_storage_download_strategy =
			form.object_storage_download_strategy;
	}
	if (hasOption("s3_path_style") && form.s3_path_style === false) {
		options.s3_path_style = false;
	}
	if (hasOption("account_mode")) {
		Object.assign(options, buildOneDrivePolicyOptions(form));
	}

	const optionRecord = options as Record<string, unknown>;
	for (const field of descriptor.fields) {
		if (
			field.scope !== "policy_options" ||
			(field.kind !== "text" && field.kind !== "secret") ||
			Object.hasOwn(optionRecord, field.name)
		) {
			continue;
		}
		const rawValue = form.policy_option_values?.[field.name] ?? "";
		const value = field.trim_on_blur === true ? rawValue.trim() : rawValue;
		if (value) {
			optionRecord[field.name] = value;
		}
	}

	return options;
}

function buildPolicyOptionsFallback(
	form: StoragePolicyOptionsForm,
): StoragePolicyOptions {
	const options: StoragePolicyOptions = {};
	if (form.content_dedup) {
		options.content_dedup = true;
	}
	if (form.remote_download_strategy !== "relay_stream") {
		options.remote_download_strategy = form.remote_download_strategy;
	}
	if (form.remote_upload_strategy !== "relay_stream") {
		options.remote_upload_strategy = form.remote_upload_strategy;
	}
	if (form.provider_resumable_upload_strategy !== "server_relay") {
		options.provider_resumable_upload_strategy =
			form.provider_resumable_upload_strategy;
	}
	if (form.provider_download_strategy !== "server_relay") {
		options.provider_download_strategy = form.provider_download_strategy;
	}
	if (form.object_storage_upload_strategy !== "relay_stream") {
		options.object_storage_upload_strategy =
			form.object_storage_upload_strategy;
	}
	if (form.object_storage_download_strategy !== "relay_stream") {
		options.object_storage_download_strategy =
			form.object_storage_download_strategy;
	}
	if (form.s3_path_style === false) {
		options.s3_path_style = false;
	}
	if (hasOneDrivePolicyOptionValues(form)) {
		Object.assign(options, buildOneDrivePolicyOptions(form));
	}
	return options;
}

function buildOneDrivePolicyOptions(
	form: StoragePolicyOptionsForm,
): StoragePolicyOptions {
	const options: StoragePolicyOptions = {
		onedrive_cloud: form.onedrive_cloud ?? "global",
		onedrive_account_mode: form.onedrive_account_mode ?? "work_or_school",
	};
	const tenant = form.onedrive_tenant?.trim() ?? "";
	const driveId = form.onedrive_drive_id?.trim() ?? "";
	const rootItemId = form.onedrive_root_item_id?.trim() ?? "";
	const siteId = form.onedrive_site_id?.trim() ?? "";
	const groupId = form.onedrive_group_id?.trim() ?? "";
	if (tenant) {
		options.onedrive_tenant = tenant;
	}
	if (driveId) {
		options.onedrive_drive_id = driveId;
	}
	options.onedrive_root_item_id = rootItemId || "root";
	if (form.onedrive_account_mode === "sharepoint_site" && siteId) {
		options.onedrive_site_id = siteId;
	}
	if (form.onedrive_account_mode === "group_drive" && groupId) {
		options.onedrive_group_id = groupId;
	}
	return options;
}

function hasOneDrivePolicyOptionValues(form: StoragePolicyOptionsForm) {
	const tenant = form.onedrive_tenant?.trim();
	return Boolean(
		(form.onedrive_account_mode != null &&
			form.onedrive_account_mode !== "work_or_school") ||
			(form.onedrive_cloud != null && form.onedrive_cloud !== "global") ||
			(tenant != null && tenant !== "" && tenant !== "common") ||
			hasText(form.onedrive_drive_id) ||
			hasText(form.onedrive_root_item_id) ||
			hasText(form.onedrive_site_id) ||
			hasText(form.onedrive_group_id),
	);
}

function hasText(value: string | null | undefined) {
	return Boolean(value?.trim());
}

function buildStorageNativeOptions(
	form: StoragePolicyOptionsForm,
): StoragePolicyOptions {
	if (!form.storage_native_processing_enabled) {
		return {};
	}

	const options: StoragePolicyOptions = {
		storage_native_processing_enabled: true,
	};
	if (form.thumbnail_processor) {
		options.thumbnail_processor = form.thumbnail_processor;
		options.thumbnail_extensions = normalizeThumbnailExtensions(
			form.thumbnail_extensions,
		);
	}
	if (form.storage_native_media_metadata_enabled) {
		options.storage_native_media_metadata_enabled = true;
		const mediaMetadataExtensions = normalizeThumbnailExtensions(
			form.media_metadata_extensions ?? [],
		);
		if (mediaMetadataExtensions.length > 0) {
			options.media_metadata_extensions = mediaMetadataExtensions;
		}
	}
	return options;
}

const SAFE_STORAGE_NATIVE_EXTENSION_PATTERN = /^[a-z0-9_-]{1,32}$/;

export function normalizeThumbnailExtensions(values: string[]) {
	const normalized: string[] = [];
	for (const value of values) {
		const extension = value.trim().replace(/^\.+/, "").toLowerCase();
		if (
			SAFE_STORAGE_NATIVE_EXTENSION_PATTERN.test(extension) &&
			!normalized.includes(extension)
		) {
			normalized.push(extension);
		}
	}
	return normalized;
}
