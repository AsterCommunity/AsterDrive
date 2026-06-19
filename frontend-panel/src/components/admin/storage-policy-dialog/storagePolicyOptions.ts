import type {
	DriverType,
	MicrosoftGraphCloud,
	OneDriveAccountMode,
	RemoteDownloadStrategy,
	RemoteUploadStrategy,
	S3DownloadStrategy,
	S3UploadStrategy,
	StorageConnectorDescriptor,
	StoragePolicyOptions,
} from "@/types/api";

export interface StoragePolicyOptionsForm {
	driver_type: DriverType;
	content_dedup: boolean;
	remote_download_strategy: RemoteDownloadStrategy;
	remote_upload_strategy: RemoteUploadStrategy;
	s3_upload_strategy: S3UploadStrategy;
	s3_download_strategy: S3DownloadStrategy;
	s3_path_style?: boolean;
	onedrive_cloud: MicrosoftGraphCloud;
	onedrive_account_mode: OneDriveAccountMode;
	onedrive_tenant: string;
	onedrive_drive_id: string;
	onedrive_root_item_id: string;
	onedrive_site_id: string;
	onedrive_group_id: string;
	storage_native_processing_enabled: boolean;
	storage_native_media_metadata_enabled?: boolean;
	thumbnail_processor: StoragePolicyOptions["thumbnail_processor"];
	thumbnail_extensions: string[];
	media_metadata_extensions?: string[];
}

type PolicyOptionsSerializer = (
	form: StoragePolicyOptionsForm,
) => StoragePolicyOptions;

export function getEffectiveS3UploadStrategy(
	options: StoragePolicyOptions,
): S3UploadStrategy {
	return options.s3_upload_strategy ?? "relay_stream";
}

export function getEffectiveS3DownloadStrategy(
	options: StoragePolicyOptions,
): S3DownloadStrategy {
	return options.s3_download_strategy ?? "relay_stream";
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
		...(POLICY_OPTION_SERIALIZERS[form.driver_type]?.(form) ?? {}),
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
	if (hasOption("s3_upload_strategy")) {
		options.s3_upload_strategy = form.s3_upload_strategy;
	}
	if (hasOption("s3_download_strategy")) {
		options.s3_download_strategy = form.s3_download_strategy;
	}
	if (hasOption("s3_path_style") && form.s3_path_style === false) {
		options.s3_path_style = false;
	}
	if (hasOption("account_mode")) {
		Object.assign(options, buildOneDrivePolicyOptions(form));
	}

	return options;
}

const POLICY_OPTION_SERIALIZERS: Partial<
	Record<DriverType, PolicyOptionsSerializer>
> = {
	local: buildLocalPolicyOptions,
	remote: buildRemotePolicyOptions,
	s3: buildObjectStoragePolicyOptions,
	tencent_cos: buildObjectStoragePolicyOptions,
	azure_blob: buildObjectStoragePolicyOptions,
	one_drive: buildOneDrivePolicyOptions,
};

function buildLocalPolicyOptions(
	form: StoragePolicyOptionsForm,
): StoragePolicyOptions {
	return form.content_dedup ? { content_dedup: true } : {};
}

function buildRemotePolicyOptions(
	form: StoragePolicyOptionsForm,
): StoragePolicyOptions {
	return {
		remote_download_strategy: form.remote_download_strategy,
		remote_upload_strategy: form.remote_upload_strategy,
	};
}

function buildObjectStoragePolicyOptions(
	form: StoragePolicyOptionsForm,
): StoragePolicyOptions {
	const options: StoragePolicyOptions = {
		s3_upload_strategy: form.s3_upload_strategy,
		s3_download_strategy: form.s3_download_strategy,
	};
	if (form.driver_type === "s3" && form.s3_path_style === false) {
		options.s3_path_style = false;
	}
	return options;
}

function buildOneDrivePolicyOptions(
	form: StoragePolicyOptionsForm,
): StoragePolicyOptions {
	const options: StoragePolicyOptions = {
		onedrive_cloud: form.onedrive_cloud,
		onedrive_account_mode: form.onedrive_account_mode,
	};
	const tenant = form.onedrive_tenant.trim();
	const driveId = form.onedrive_drive_id.trim();
	const rootItemId = form.onedrive_root_item_id.trim();
	const siteId = form.onedrive_site_id.trim();
	const groupId = form.onedrive_group_id.trim();
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
