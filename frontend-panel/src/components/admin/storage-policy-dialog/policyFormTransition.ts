import type { DriverType, StorageConnectorDescriptor } from "@/types/api";
import {
	supportsObjectStorageConnection,
	supportsObjectStorageTransferStrategy,
	supportsOneDrivePolicyOptions,
	supportsRemoteNodeBinding,
	supportsStorageNativeProcessing,
} from "./descriptorPredicates";
import {
	DEFAULT_STORAGE_NATIVE_THUMBNAIL_EXTENSIONS,
	type PolicyFormData,
} from "./formTypes";

export function applyPolicyFormFieldChange<K extends keyof PolicyFormData>(
	form: PolicyFormData,
	key: K,
	value: PolicyFormData[K],
): PolicyFormData {
	if (key === "storage_native_processing_enabled") {
		const enabled = value as boolean;
		return {
			...form,
			storage_native_processing_enabled: enabled,
			thumbnail_processor: enabled ? "storage_native" : null,
			thumbnail_extensions: enabled
				? form.thumbnail_extensions.length > 0
					? form.thumbnail_extensions
					: [...DEFAULT_STORAGE_NATIVE_THUMBNAIL_EXTENSIONS]
				: [],
			storage_native_media_metadata_enabled: enabled
				? form.storage_native_media_metadata_enabled
				: false,
			media_metadata_extensions: enabled
				? (form.media_metadata_extensions ?? [])
				: [],
		};
	}

	if (key === "remote_node_id") {
		return {
			...form,
			remote_node_id: value as string,
			remote_storage_target_key: "",
		};
	}

	return { ...form, [key]: value };
}

export function applyPolicyDriverTransition(
	form: PolicyFormData,
	driverType: DriverType,
	nextDriverDescriptor: StorageConnectorDescriptor | null | undefined,
): PolicyFormData {
	const { s3_path_style: previousS3PathStyle, ...formWithoutS3PathStyle } =
		form;
	const nextSupportsStorageNativeProcessing =
		supportsStorageNativeProcessing(nextDriverDescriptor);
	const nextPolicyOptionValues = policyOptionValuesForDescriptor(
		form.policy_option_values ?? {},
		nextDriverDescriptor,
	);

	if (supportsObjectStorageConnection(nextDriverDescriptor)) {
		return {
			...formWithoutS3PathStyle,
			driver_type: driverType,
			policy_option_values: nextPolicyOptionValues,
			remote_node_id: "",
			remote_storage_target_key: "",
			storage_native_processing_enabled: nextSupportsStorageNativeProcessing
				? form.storage_native_processing_enabled
				: false,
			thumbnail_processor: nextSupportsStorageNativeProcessing
				? form.thumbnail_processor
				: null,
			thumbnail_extensions: nextSupportsStorageNativeProcessing
				? form.thumbnail_extensions
				: [],
			storage_native_media_metadata_enabled: nextSupportsStorageNativeProcessing
				? form.storage_native_media_metadata_enabled
				: false,
			media_metadata_extensions: nextSupportsStorageNativeProcessing
				? (form.media_metadata_extensions ?? [])
				: [],
			...(supportsObjectStorageTransferStrategy(nextDriverDescriptor)
				? { s3_path_style: previousS3PathStyle ?? true }
				: {}),
		};
	}

	if (supportsRemoteNodeBinding(nextDriverDescriptor)) {
		return {
			...formWithoutS3PathStyle,
			driver_type: driverType,
			endpoint: "",
			bucket: "",
			access_key: "",
			secret_key: "",
			policy_option_values: nextPolicyOptionValues,
			content_dedup: false,
			storage_native_processing_enabled: false,
			thumbnail_processor: null,
			thumbnail_extensions: [],
			storage_native_media_metadata_enabled: false,
			media_metadata_extensions: [],
			remote_download_strategy: "relay_stream",
			remote_upload_strategy: "relay_stream",
			remote_storage_target_key: "",
		};
	}

	if (supportsOneDrivePolicyOptions(nextDriverDescriptor)) {
		return {
			...formWithoutS3PathStyle,
			driver_type: driverType,
			endpoint: "",
			bucket: "",
			access_key: "",
			secret_key: "",
			policy_option_values: nextPolicyOptionValues,
			remote_node_id: "",
			remote_storage_target_key: "",
			content_dedup: false,
			onedrive_cloud: form.onedrive_cloud || "global",
			onedrive_account_mode: form.onedrive_account_mode || "work_or_school",
			onedrive_tenant: form.onedrive_tenant || "common",
			onedrive_drive_id: form.onedrive_drive_id,
			onedrive_root_item_id: form.onedrive_root_item_id,
			onedrive_site_id: form.onedrive_site_id,
			onedrive_group_id: form.onedrive_group_id,
			application_credentials: {
				microsoft_graph: {
					cloud: form.onedrive_cloud || "global",
					tenant: form.onedrive_tenant || "common",
					client_id: "",
					client_secret: "",
					scopes: "",
				},
			},
			storage_native_processing_enabled: false,
			thumbnail_processor: null,
			thumbnail_extensions: [],
			storage_native_media_metadata_enabled: false,
			media_metadata_extensions: [],
			remote_download_strategy: "relay_stream",
			remote_upload_strategy: "relay_stream",
			object_storage_upload_strategy: "relay_stream",
			object_storage_download_strategy: "relay_stream",
			provider_resumable_upload_strategy: "server_relay",
			provider_download_strategy: "server_relay",
		};
	}

	return {
		...formWithoutS3PathStyle,
		driver_type: driverType,
		endpoint: "",
		bucket: "",
		access_key: "",
		secret_key: "",
		policy_option_values: nextPolicyOptionValues,
		remote_node_id: "",
		remote_storage_target_key: "",
		storage_native_processing_enabled: false,
		thumbnail_processor: null,
		thumbnail_extensions: [],
		storage_native_media_metadata_enabled: false,
		media_metadata_extensions: [],
		remote_download_strategy: "relay_stream",
		remote_upload_strategy: "relay_stream",
		object_storage_upload_strategy: "relay_stream",
		object_storage_download_strategy: "relay_stream",
		provider_resumable_upload_strategy: "server_relay",
		provider_download_strategy: "server_relay",
	};
}

function policyOptionValuesForDescriptor(
	values: Record<string, string>,
	descriptor: StorageConnectorDescriptor | null | undefined,
) {
	const nextValues: Record<string, string> = {};
	for (const field of descriptor?.fields ?? []) {
		if (
			field.scope === "policy_options" &&
			(field.kind === "text" || field.kind === "secret") &&
			Object.hasOwn(values, field.name)
		) {
			nextValues[field.name] = values[field.name];
		}
	}
	return nextValues;
}
