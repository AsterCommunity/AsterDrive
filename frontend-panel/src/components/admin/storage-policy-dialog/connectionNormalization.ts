import { normalizeObjectStorageConnectionFields } from "@/lib/objectStorageConnectionFields";
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
	supportsMicrosoftGraphApplicationConfig,
	supportsObjectStorageConnection,
	supportsOneDrivePolicyOptions,
	supportsRemoteNodeBinding,
} from "./descriptorPredicates";
import type { PolicyFormData } from "./formTypes";
import { getPolicyForm } from "./formTypes";
import { buildPolicyOptions } from "./storagePolicyOptions";

export interface S3CompatibleDriverPromotionTarget {
	driverLabel: string;
	driverType: DriverType;
}

export function parseRemoteNodeId(value: string): number | undefined {
	if (!value) {
		return undefined;
	}

	const parsed = Number(value);
	return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : undefined;
}

function endpointHostMatchesRule(
	host: string,
	rule: NonNullable<
		StorageConnectorDescriptor["driver_recommendations"]
	>[number]["endpoint_host_rules"][number],
) {
	const equals = rule.equals?.trim().toLowerCase();
	if (equals && host === equals) {
		return true;
	}

	const endsWith = rule.ends_with?.trim().toLowerCase();
	return Boolean(endsWith && host.endsWith(endsWith));
}

function parseEndpointHost(endpoint: string) {
	const trimmedEndpoint = endpoint.trim();
	if (!trimmedEndpoint) {
		return null;
	}

	try {
		return new URL(trimmedEndpoint).hostname.toLowerCase();
	} catch {
		return null;
	}
}

export function getS3CompatibleDriverPromotionTarget(
	policy: {
		driver_type: DriverType;
		endpoint: string;
	} | null,
	sourceDescriptor: StorageConnectorDescriptor | null | undefined,
	getDriverLabel: (driverType: DriverType) => string,
): S3CompatibleDriverPromotionTarget | null {
	if (policy == null || sourceDescriptor?.driver_type !== policy.driver_type) {
		return null;
	}

	const host = parseEndpointHost(policy.endpoint);
	if (host == null) {
		return null;
	}

	for (const recommendation of sourceDescriptor.driver_recommendations ?? []) {
		if (
			recommendation.endpoint_host_rules.some((rule) =>
				endpointHostMatchesRule(host, rule),
			)
		) {
			const driverType = recommendation.target_driver_type;
			return {
				driverLabel: getDriverLabel(driverType),
				driverType,
			};
		}
	}

	return null;
}

export function normalizePolicyForm(
	form: PolicyFormData,
	descriptor?: StorageConnectorDescriptor | null,
): PolicyFormData {
	const shouldNormalizeObjectStorage = shouldUseObjectStorageConnection(
		form,
		descriptor,
	);
	const shouldNormalizeMicrosoftGraph = shouldUseMicrosoftGraphConfig(
		form,
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

	const normalized = normalizeObjectStorageConnectionFields(
		form.endpoint,
		form.bucket,
	);
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
		shouldUseObjectStorageConnection(normalizedForm, descriptor, editingPolicy)
	) {
		return (
			normalizedForm.endpoint !== editingPolicy.endpoint ||
			normalizedForm.bucket !== editingPolicy.bucket ||
			normalizedForm.base_path !== editingPolicy.base_path ||
			normalizedForm.access_key !== "" ||
			normalizedForm.secret_key !== ""
		);
	}

	if (shouldUseRemoteNodeBinding(descriptor)) {
		return (
			parseRemoteNodeId(normalizedForm.remote_node_id) !==
				editingPolicy.remote_node_id ||
			normalizedForm.remote_storage_target_key !==
				(editingPolicy.remote_storage_target_key ?? "") ||
			normalizedForm.base_path !== editingPolicy.base_path
		);
	}

	if (
		shouldUseMicrosoftGraphConfig(normalizedForm, descriptor, editingPolicy)
	) {
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
		remote_storage_target_key: normalizedForm.remote_storage_target_key,
		options: buildPolicyOptions(normalizedForm, descriptor),
	});
}

export function getEndpointValidationMessage(
	form: PolicyFormData,
	t: (key: string) => string,
	descriptor?: StorageConnectorDescriptor | null,
) {
	if (
		descriptor
			? !supportsObjectStorageConnection(descriptor)
			: !hasObjectStorageConnectionFields(form)
	) {
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
	form: PolicyFormData,
	descriptor?: StorageConnectorDescriptor | null,
	editingPolicy?: StoragePolicy | null,
) {
	return descriptor
		? supportsObjectStorageConnection(descriptor)
		: hasObjectStorageConnectionFields(form, editingPolicy);
}

function shouldUseMicrosoftGraphConfig(
	form: PolicyFormData,
	descriptor?: StorageConnectorDescriptor | null,
	editingPolicy?: StoragePolicy | null,
) {
	return descriptor
		? supportsOneDrivePolicyOptions(descriptor) ||
				supportsMicrosoftGraphApplicationConfig(descriptor)
		: hasExplicitMicrosoftGraphFields(form) ||
				hasMicrosoftGraphPolicyOptions(editingPolicy);
}

function shouldUseRemoteNodeBinding(
	descriptor: StorageConnectorDescriptor | null | undefined,
) {
	return descriptor ? supportsRemoteNodeBinding(descriptor) : false;
}

function hasObjectStorageConnectionFields(
	form: PolicyFormData,
	editingPolicy?: StoragePolicy | null,
) {
	return Boolean(
		hasEndpointUrlScheme(form.endpoint) ||
			hasText(form.bucket) ||
			hasText(form.access_key) ||
			hasText(form.secret_key) ||
			hasEndpointUrlScheme(editingPolicy?.endpoint) ||
			hasText(editingPolicy?.bucket),
	);
}

function hasEndpointUrlScheme(endpoint?: string | null) {
	return /^[a-z][a-z0-9+.-]*:/i.test(endpoint?.trim() ?? "");
}

function hasExplicitMicrosoftGraphFields(form: PolicyFormData) {
	const microsoftGraph = microsoftGraphCredentials(form);
	const tenant = form.onedrive_tenant?.trim();
	return Boolean(
		(form.onedrive_account_mode != null &&
			form.onedrive_account_mode !== "work_or_school") ||
			(form.onedrive_cloud != null && form.onedrive_cloud !== "global") ||
			(tenant != null && tenant !== "" && tenant !== "common") ||
			hasText(form.onedrive_drive_id) ||
			hasText(form.onedrive_root_item_id) ||
			hasText(form.onedrive_site_id) ||
			hasText(form.onedrive_group_id) ||
			hasText(microsoftGraph.client_id) ||
			hasText(microsoftGraph.client_secret) ||
			hasText(microsoftGraph.scopes),
	);
}

function hasMicrosoftGraphPolicyOptions(policy?: StoragePolicy | null) {
	const options = policy?.options;
	return Boolean(
		options?.onedrive_account_mode ||
			options?.onedrive_cloud ||
			options?.onedrive_tenant ||
			options?.onedrive_drive_id ||
			options?.onedrive_root_item_id ||
			options?.onedrive_site_id ||
			options?.onedrive_group_id,
	);
}

function hasText(value: string | null | undefined) {
	return Boolean(value?.trim());
}
