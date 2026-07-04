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

function parseOptionalFiniteNumber(value: string): number | undefined {
	const trimmed = value.trim();
	if (!trimmed) {
		return undefined;
	}

	const parsed = Number(trimmed);
	return Number.isFinite(parsed) ? parsed : undefined;
}

function parseOptionalChunkSizeBytes(value: string): number {
	const parsed = parseOptionalFiniteNumber(value);
	return parsed == null ? 0 : parsed * 1024 * 1024;
}

export function buildPolicyTestPayload(
	form: PolicyFormData,
	descriptor?: StorageConnectorDescriptor | null,
	policyId?: number | null,
) {
	const normalizedForm = normalizePolicyForm(form, descriptor);

	return {
		...(policyId != null ? { policy_id: policyId } : {}),
		driver_type: normalizedForm.driver_type,
		endpoint: normalizedForm.endpoint || undefined,
		bucket: normalizedForm.bucket || undefined,
		access_key: normalizedForm.access_key || undefined,
		secret_key: normalizedForm.secret_key || undefined,
		base_path: normalizedForm.base_path || undefined,
		remote_node_id: parseRemoteNodeId(normalizedForm.remote_node_id),
		remote_storage_target_key:
			normalizedForm.remote_storage_target_key || undefined,
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
		remote_storage_target_key:
			normalizedForm.remote_storage_target_key || undefined,
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
		remote_storage_target_key:
			normalizedForm.remote_storage_target_key || undefined,
		max_file_size: parseOptionalFiniteNumber(normalizedForm.max_file_size),
		chunk_size: parseOptionalChunkSizeBytes(normalizedForm.chunk_size),
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
		remote_storage_target_key:
			normalizedForm.remote_storage_target_key || undefined,
		max_file_size: parseOptionalFiniteNumber(normalizedForm.max_file_size),
		chunk_size: parseOptionalChunkSizeBytes(normalizedForm.chunk_size),
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
