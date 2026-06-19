import { normalizeS3ConnectionFields } from "@/lib/s3Endpoint";
import type {
	DriverType,
	StorageConnectorDescriptor,
	StoragePolicy,
	StoragePolicyOptions,
} from "@/types/api";
import {
	microsoftGraphCredentials,
	updateMicrosoftGraphCredentials,
} from "./applicationCredentials";
import {
	isObjectStorageDriver,
	isOneDriveDriver,
	supportsMicrosoftGraphApplicationConfig,
	supportsObjectStorageConnection,
	supportsOneDrivePolicyOptions,
	supportsRemoteNodeBinding,
} from "./descriptorPredicates";
import type { PolicyFormData } from "./formTypes";
import { getPolicyForm } from "./formTypes";
import { buildPolicyOptions } from "./storagePolicyOptions";

export type S3CompatiblePromotionDriverType = Extract<
	DriverType,
	"tencent_cos"
>;

export interface S3CompatibleDriverPromotionTarget {
	driverLabel: string;
	driverType: S3CompatiblePromotionDriverType;
}

export function parseRemoteNodeId(value: string): number | undefined {
	if (!value) {
		return undefined;
	}

	const parsed = Number(value);
	return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : undefined;
}

export function isTencentCosEndpoint(endpoint: string) {
	const trimmedEndpoint = endpoint.trim();
	if (!trimmedEndpoint) {
		return false;
	}

	try {
		const host = new URL(trimmedEndpoint).hostname.toLowerCase();
		return host === "myqcloud.com" || host.endsWith(".myqcloud.com");
	} catch {
		return false;
	}
}

export function getS3CompatibleDriverPromotionTarget(
	policy: {
		driver_type: DriverType;
		endpoint: string;
	} | null,
	getDriverLabel: (driverType: S3CompatiblePromotionDriverType) => string,
): S3CompatibleDriverPromotionTarget | null {
	if (policy?.driver_type !== "s3") {
		return null;
	}

	// Keep provider detection centralized so future OSS/OBS promotions only need
	// one UI registry change plus the matching backend allowlist entry.
	if (isTencentCosEndpoint(policy.endpoint)) {
		return {
			driverLabel: getDriverLabel("tencent_cos"),
			driverType: "tencent_cos",
		};
	}

	return null;
}

export function normalizePolicyForm(
	form: PolicyFormData,
	descriptor?: StorageConnectorDescriptor | null,
): PolicyFormData {
	const shouldNormalizeObjectStorage = shouldUseObjectStorageConnection(
		form.driver_type,
		descriptor,
	);
	const shouldNormalizeMicrosoftGraph = shouldUseMicrosoftGraphConfig(
		form.driver_type,
		descriptor,
	);

	if (!shouldNormalizeObjectStorage && !shouldNormalizeMicrosoftGraph) {
		return form;
	}

	if (shouldNormalizeMicrosoftGraph) {
		const microsoftGraph = microsoftGraphCredentials(form);
		const normalizedCredentials = updateMicrosoftGraphCredentials(form, {
			cloud: form.onedrive_cloud,
			tenant: form.onedrive_tenant.trim(),
			client_id: microsoftGraph.client_id.trim(),
			client_secret: microsoftGraph.client_secret.trim(),
			scopes: microsoftGraph.scopes.trim(),
		});
		const normalized = {
			...form,
			onedrive_tenant: form.onedrive_tenant.trim(),
			onedrive_drive_id: form.onedrive_drive_id.trim(),
			onedrive_root_item_id: form.onedrive_root_item_id.trim(),
			onedrive_site_id: form.onedrive_site_id.trim(),
			onedrive_group_id: form.onedrive_group_id.trim(),
			application_credentials: normalizedCredentials,
		};
		return normalized.onedrive_tenant === form.onedrive_tenant &&
			normalized.onedrive_drive_id === form.onedrive_drive_id &&
			normalized.onedrive_root_item_id === form.onedrive_root_item_id &&
			normalized.onedrive_site_id === form.onedrive_site_id &&
			normalized.onedrive_group_id === form.onedrive_group_id &&
			normalized.application_credentials === form.application_credentials
			? form
			: normalized;
	}

	const normalized = normalizeS3ConnectionFields(form.endpoint, form.bucket);
	const normalizedAccessKey = form.access_key.trim();
	const normalizedSecretKey = form.secret_key.trim();
	if (
		normalized.endpoint === form.endpoint &&
		normalized.bucket === form.bucket &&
		normalizedAccessKey === form.access_key &&
		normalizedSecretKey === form.secret_key
	) {
		return form;
	}

	return {
		...form,
		endpoint: normalized.endpoint,
		bucket: normalized.bucket,
		access_key: normalizedAccessKey,
		secret_key: normalizedSecretKey,
	};
}

function getComparableOneDrivePolicyOptions(
	policy: StoragePolicy,
): StoragePolicyOptions {
	return buildPolicyOptions(getPolicyForm(policy));
}

export function hasConnectionFieldChanges(
	form: PolicyFormData,
	editingPolicy: StoragePolicy | null,
	descriptor?: StorageConnectorDescriptor | null,
) {
	const normalizedForm = normalizePolicyForm(form, descriptor);

	if (!editingPolicy) {
		return true;
	}

	if (
		shouldUseObjectStorageConnection(normalizedForm.driver_type, descriptor)
	) {
		return (
			normalizedForm.endpoint !== editingPolicy.endpoint ||
			normalizedForm.bucket !== editingPolicy.bucket ||
			normalizedForm.base_path !== editingPolicy.base_path ||
			normalizedForm.access_key !== "" ||
			normalizedForm.secret_key !== ""
		);
	}

	if (shouldUseRemoteNodeBinding(normalizedForm.driver_type, descriptor)) {
		return (
			parseRemoteNodeId(normalizedForm.remote_node_id) !==
				editingPolicy.remote_node_id ||
			normalizedForm.base_path !== editingPolicy.base_path
		);
	}

	if (shouldUseMicrosoftGraphConfig(normalizedForm.driver_type, descriptor)) {
		return (
			normalizedForm.base_path !== editingPolicy.base_path ||
			JSON.stringify(buildPolicyOptions(normalizedForm, descriptor)) !==
				JSON.stringify(getComparableOneDrivePolicyOptions(editingPolicy))
		);
	}

	return normalizedForm.base_path !== editingPolicy.base_path;
}

export function getPolicyConnectionTestKey(
	form: PolicyFormData,
	descriptor?: StorageConnectorDescriptor | null,
) {
	const normalizedForm = normalizePolicyForm(form, descriptor);

	return JSON.stringify({
		driver_type: normalizedForm.driver_type,
		endpoint: normalizedForm.endpoint,
		bucket: normalizedForm.bucket,
		access_key: normalizedForm.access_key,
		secret_key: normalizedForm.secret_key,
		base_path: normalizedForm.base_path,
		remote_node_id: parseRemoteNodeId(normalizedForm.remote_node_id),
		options: buildPolicyOptions(normalizedForm, descriptor),
	});
}

export function getEndpointValidationMessage(
	form: PolicyFormData,
	t: (key: string) => string,
	descriptor?: StorageConnectorDescriptor | null,
) {
	const shouldValidateEndpoint = shouldUseObjectStorageConnection(
		form.driver_type,
		descriptor,
	);
	if (!shouldValidateEndpoint) {
		return null;
	}

	const trimmedEndpoint = form.endpoint.trim();
	if (!trimmedEndpoint) {
		return null;
	}
	const endpointProtocolMessage =
		descriptor?.fields.find(
			(field) => field.scope === "connection" && field.name === "endpoint",
		)?.invalid_protocol_message_key ?? "s3_endpoint_protocol_required_error";

	let endpointUrl: URL;
	try {
		endpointUrl = new URL(trimmedEndpoint);
	} catch {
		return t(endpointProtocolMessage);
	}

	if (endpointUrl.protocol !== "http:" && endpointUrl.protocol !== "https:") {
		return t(endpointProtocolMessage);
	}

	return null;
}

function shouldUseObjectStorageConnection(
	driverType: DriverType,
	descriptor?: StorageConnectorDescriptor | null,
) {
	return descriptor
		? supportsObjectStorageConnection(descriptor)
		: isObjectStorageDriver(driverType);
}

function shouldUseMicrosoftGraphConfig(
	driverType: DriverType,
	descriptor?: StorageConnectorDescriptor | null,
) {
	return descriptor
		? supportsOneDrivePolicyOptions(descriptor) ||
				supportsMicrosoftGraphApplicationConfig(descriptor)
		: isOneDriveDriver(driverType);
}

function shouldUseRemoteNodeBinding(
	driverType: DriverType,
	descriptor?: StorageConnectorDescriptor | null,
) {
	return descriptor
		? supportsRemoteNodeBinding(descriptor)
		: driverType === "remote";
}
