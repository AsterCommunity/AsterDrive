import { normalizePolicyForm } from "@/components/admin/storage-policy-dialog/connectionNormalization";
import {
	emptyForm,
	type PolicyFormData,
} from "@/components/admin/storage-policy-dialog/formTypes";
import { buildStorageConnectorConnection } from "@/components/admin/storage-policy-dialog/payloadBuilders";
import type {
	RemoteCreateStorageTargetRequest,
	RemoteStorageTargetInfo,
	RemoteUpdateStorageTargetRequest,
	StorageConnectorDescriptor,
} from "@/types/api";

export type RemoteStorageTargetFormData = PolicyFormData;

export function isRemoteStorageTargetConnectorId(
	value: unknown,
): value is string {
	return typeof value === "string" && value.trim().length > 0;
}

export function getRemoteStorageTargetForm(
	target: RemoteStorageTargetInfo,
): RemoteStorageTargetFormData {
	return {
		...emptyForm,
		name: target.name,
		connector_id: target.connector_id ?? "",
		connector_config_values: Object.fromEntries(
			Object.entries(target.connector_config?.values ?? {}).filter(
				(entry): entry is [string, string | number | boolean] => {
					const value = entry[1];
					return (
						typeof value === "string" ||
						typeof value === "number" ||
						typeof value === "boolean"
					);
				},
			),
		),
		credential_values: {},
		is_default: target.is_default,
	};
}

function buildStorageConnection(
	form: RemoteStorageTargetFormData,
	descriptor: StorageConnectorDescriptor,
	requireCredential: boolean,
) {
	const policyConnection = buildStorageConnectorConnection(
		form,
		descriptor,
		requireCredential,
	);
	return {
		connector_config: policyConnection.connector_config,
		credential: policyConnection.credential,
	};
}

export function buildCreateRemoteStorageTargetPayload(
	form: RemoteStorageTargetFormData,
	descriptor: StorageConnectorDescriptor,
): RemoteCreateStorageTargetRequest {
	const normalized = normalizePolicyForm(form, descriptor);
	return {
		name: normalized.name.trim(),
		connection: buildStorageConnection(normalized, descriptor, true),
		is_default: normalized.is_default,
	};
}

export function buildUpdateRemoteStorageTargetPayload(
	form: RemoteStorageTargetFormData,
	descriptor: StorageConnectorDescriptor,
	editingTarget: RemoteStorageTargetInfo,
): RemoteUpdateStorageTargetRequest {
	const normalized = normalizePolicyForm(form, descriptor);
	const connectorChanged =
		editingTarget.connector_id !== descriptor.connector_id;
	const hasCredentialValues = Object.values(normalized.credential_values).some(
		(value) => value.trim().length > 0,
	);
	return {
		name: normalized.name.trim(),
		connection: buildStorageConnection(
			normalized,
			descriptor,
			connectorChanged || hasCredentialValues,
		),
		is_default: normalized.is_default,
	};
}

export const emptyRemoteStorageTargetForm: RemoteStorageTargetFormData = {
	...emptyForm,
	connector_id: "",
	is_default: false,
};
