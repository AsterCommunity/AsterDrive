import type {
	CreatePolicyRequest,
	ExecuteDraftStoragePolicyActionRequest,
	ResolveStorageConnectorTransitionsRequest,
	StorageConnectorActionId,
	StorageConnectorDescriptor,
	StorageConnectorFieldValue,
	TestPolicyParamsRequest,
	UpdatePolicyRequest,
} from "@/types/api";
import { normalizePolicyForm } from "./connectionNormalization";
import type { PolicyFormData } from "./formTypes";

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
	descriptor: StorageConnectorDescriptor,
	policyId?: number | null,
): TestPolicyParamsRequest {
	return {
		...(policyId != null ? { policy_id: policyId } : {}),
		connection: buildStorageConnectorConnection(form, descriptor, true),
	};
}

export function buildStorageConnectorTransitionResolverPayload(
	form: PolicyFormData,
	descriptor: StorageConnectorDescriptor,
): ResolveStorageConnectorTransitionsRequest {
	const normalizedForm = normalizePolicyForm(form, descriptor);
	return {
		connector_config: buildConnectorConfig(normalizedForm, descriptor),
		behavior: buildBehavior(normalizedForm),
	};
}

export function buildStorageConnectorActionPayload(
	form: PolicyFormData,
	policyId: number | null | undefined,
	descriptor: StorageConnectorDescriptor,
	actionId: StorageConnectorActionId,
	values: Record<string, StorageConnectorFieldValue>,
): ExecuteDraftStoragePolicyActionRequest {
	return {
		action_id: actionId,
		values,
		policy_id: policyId ?? undefined,
		connection: buildStorageConnectorConnection(form, descriptor, true),
	};
}

export function buildCreatePolicyPayload(
	form: PolicyFormData,
	descriptor: StorageConnectorDescriptor,
): CreatePolicyRequest {
	const normalizedForm = normalizePolicyForm(form, descriptor);
	return {
		name: normalizedForm.name,
		connection: buildStorageConnectorConnection(
			normalizedForm,
			descriptor,
			true,
		),
		max_file_size: parseOptionalFiniteNumber(normalizedForm.max_file_size),
		chunk_size: parseOptionalChunkSizeBytes(normalizedForm.chunk_size),
		is_default: normalizedForm.is_default,
	};
}

export function buildUpdatePolicyPayload(
	form: PolicyFormData,
	descriptor: StorageConnectorDescriptor,
): UpdatePolicyRequest {
	const normalizedForm = normalizePolicyForm(form, descriptor);
	const credential = buildCredential(normalizedForm, descriptor, false);
	return {
		name: normalizedForm.name,
		connector_config: buildConnectorConfig(normalizedForm, descriptor),
		behavior: buildBehavior(normalizedForm),
		...(credential ? { credential } : {}),
		max_file_size: parseOptionalFiniteNumber(normalizedForm.max_file_size),
		chunk_size: parseOptionalChunkSizeBytes(normalizedForm.chunk_size),
		is_default: normalizedForm.is_default,
	};
}

export function buildStorageConnectorConnection(
	form: PolicyFormData,
	descriptor: StorageConnectorDescriptor,
	requireCredential: boolean,
) {
	const normalizedForm = normalizePolicyForm(form, descriptor);
	return {
		connector_config: buildConnectorConfig(normalizedForm, descriptor),
		behavior: buildBehavior(normalizedForm),
		credential: buildCredential(
			normalizedForm,
			descriptor,
			requireCredential,
		) ?? {
			mode: "none" as const,
		},
	};
}

function buildConnectorConfig(
	form: PolicyFormData,
	descriptor: StorageConnectorDescriptor,
) {
	return {
		format_version: 1,
		connector_id: descriptor.connector_id,
		schema_version: descriptor.config_schema_version,
		values: connectorConfigValues(form, descriptor),
	} as never;
}

function connectorConfigValues(
	form: PolicyFormData,
	descriptor: StorageConnectorDescriptor,
) {
	const values: Record<string, unknown> = {};
	for (const field of descriptor.fields) {
		if (field.scope !== "connector_config") {
			continue;
		}
		const value = form.connector_config_values[field.name];
		if (
			value === undefined ||
			value === null ||
			(value === "" && !field.required)
		) {
			continue;
		}
		values[field.name] = value;
	}
	return values;
}

function buildCredential(
	form: PolicyFormData,
	descriptor: StorageConnectorDescriptor,
	required: boolean,
) {
	const values = credentialValues(form, descriptor);
	const hasValues = Object.keys(values).length > 0;

	if (descriptor.credential_mode === "static_secret") {
		return hasValues || required
			? { mode: "static" as const, values }
			: undefined;
	}
	if (descriptor.credential_mode === "oauth_delegated") {
		return hasValues || required
			? { mode: "authorization_application" as const, values }
			: undefined;
	}
	return required ? { mode: "none" as const } : undefined;
}

function credentialValues(
	form: PolicyFormData,
	descriptor: StorageConnectorDescriptor,
) {
	const expectedScope =
		descriptor.credential_mode === "static_secret"
			? "static_credential"
			: descriptor.credential_mode === "oauth_delegated"
				? "authorization_application"
				: null;
	if (expectedScope == null) {
		return {};
	}
	const values: Record<string, string> = {};
	for (const field of descriptor.fields) {
		if (field.scope !== expectedScope) {
			continue;
		}
		const value = form.credential_values[field.name] ?? "";
		if (value !== "") {
			values[field.name] = value;
		}
	}
	return values;
}

function buildBehavior(form: PolicyFormData) {
	return {
		thumbnail_processor: form.thumbnail_processor ?? undefined,
		thumbnail_extensions: form.thumbnail_extensions,
		media_metadata_extensions: form.media_metadata_extensions,
	};
}
