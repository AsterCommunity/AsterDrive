import type {
	CreatePolicyRequest,
	ExecuteDraftStoragePolicyActionRequest,
	StorageConnectorDescriptor,
	UpdatePolicyRequest,
} from "@/types/api";
import {
	normalizePolicyForm,
	parseRemoteNodeId,
} from "./connectionNormalization";
import type { PolicyFormData } from "./formTypes";
import {
	buildStorageApplicationConfig,
	parseMicrosoftGraphScopes,
} from "./storagePolicyApplicationConfig";
import { buildPolicyOptions } from "./storagePolicyOptions";

export function buildPolicyTestPayload(
	form: PolicyFormData,
	descriptor?: StorageConnectorDescriptor | null,
) {
	const normalizedForm = normalizePolicyForm(form, descriptor);

	return {
		driver_type: normalizedForm.driver_type,
		endpoint: normalizedForm.endpoint || undefined,
		bucket: normalizedForm.bucket || undefined,
		access_key: normalizedForm.access_key || undefined,
		secret_key: normalizedForm.secret_key || undefined,
		base_path: normalizedForm.base_path || undefined,
		remote_node_id: parseRemoteNodeId(normalizedForm.remote_node_id),
		options: buildPolicyOptions(normalizedForm, descriptor),
	};
}

export function buildTencentCosCorsPayload(
	form: PolicyFormData,
	policyId?: number | null,
	descriptor?: StorageConnectorDescriptor | null,
): ExecuteDraftStoragePolicyActionRequest {
	const normalizedForm = normalizePolicyForm(form, descriptor);

	return {
		action: "configure_tencent_cos_cors",
		policy_id: policyId ?? undefined,
		driver_type: normalizedForm.driver_type,
		endpoint: normalizedForm.endpoint || undefined,
		bucket: normalizedForm.bucket || undefined,
		access_key: normalizedForm.access_key || undefined,
		secret_key: normalizedForm.secret_key || undefined,
		base_path: normalizedForm.base_path || undefined,
		remote_node_id: parseRemoteNodeId(normalizedForm.remote_node_id),
		options: buildPolicyOptions(normalizedForm, descriptor),
	};
}

export function buildCreatePolicyPayload(
	form: PolicyFormData,
	descriptor?: StorageConnectorDescriptor | null,
): CreatePolicyRequest {
	const normalizedForm = normalizePolicyForm(form, descriptor);
	const applicationConfig = buildStorageApplicationConfig(
		normalizedForm,
		descriptor,
	);
	const usesApplicationConfig = applicationConfig !== undefined;

	const payload: CreatePolicyRequest = {
		name: normalizedForm.name,
		driver_type: normalizedForm.driver_type,
		endpoint: normalizedForm.endpoint,
		bucket: normalizedForm.bucket,
		access_key: usesApplicationConfig ? "" : normalizedForm.access_key,
		secret_key: usesApplicationConfig ? "" : normalizedForm.secret_key,
		base_path: normalizedForm.base_path,
		remote_node_id: parseRemoteNodeId(normalizedForm.remote_node_id),
		max_file_size: normalizedForm.max_file_size
			? Number(normalizedForm.max_file_size)
			: undefined,
		chunk_size: normalizedForm.chunk_size
			? Number(normalizedForm.chunk_size) * 1024 * 1024
			: 0,
		is_default: normalizedForm.is_default,
		options: buildPolicyOptions(normalizedForm, descriptor),
	};
	if (applicationConfig) {
		payload.application_config = applicationConfig;
	}
	return payload;
}

export function buildUpdatePolicyPayload(
	form: PolicyFormData,
	descriptor?: StorageConnectorDescriptor | null,
): UpdatePolicyRequest {
	const normalizedForm = normalizePolicyForm(form, descriptor);
	const applicationConfig = buildStorageApplicationConfig(
		normalizedForm,
		descriptor,
	);
	const payload: UpdatePolicyRequest = {
		name: normalizedForm.name,
		endpoint: normalizedForm.endpoint,
		bucket: normalizedForm.bucket,
		base_path: normalizedForm.base_path,
		remote_node_id: parseRemoteNodeId(normalizedForm.remote_node_id),
		max_file_size: normalizedForm.max_file_size
			? Number(normalizedForm.max_file_size)
			: undefined,
		chunk_size: normalizedForm.chunk_size
			? Number(normalizedForm.chunk_size) * 1024 * 1024
			: 0,
		is_default: normalizedForm.is_default,
		options: buildPolicyOptions(normalizedForm, descriptor),
	};

	if (applicationConfig) {
		payload.application_config = applicationConfig;
	} else {
		if (normalizedForm.access_key) {
			payload.access_key = normalizedForm.access_key;
		}
		if (normalizedForm.secret_key) {
			payload.secret_key = normalizedForm.secret_key;
		}
	}

	return payload;
}

export { parseMicrosoftGraphScopes };
