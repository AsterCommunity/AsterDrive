import type {
	RemoteCreateStorageTargetRequest,
	RemoteStorageTargetConnectorDescriptor,
	RemoteStorageTargetInfo,
	RemoteUpdateStorageTargetRequest,
} from "@/types/api";

export type RemoteStorageTargetFieldValue = string | number | boolean;
export interface RemoteStorageTargetFormData {
	name: string;
	connector_id: string;
	values: Record<string, RemoteStorageTargetFieldValue>;
	is_default: boolean;
}

const valueForField = (
	field: RemoteStorageTargetConnectorDescriptor["fields"][number],
	values: Record<string, RemoteStorageTargetFieldValue>,
): RemoteStorageTargetFieldValue =>
	values[field.name] ??
	field.default_value ??
	(field.kind === "boolean" ? false : field.kind === "number" ? 0 : "");

export function createRemoteStorageTargetForm(
	descriptor: RemoteStorageTargetConnectorDescriptor,
	isDefault: boolean,
): RemoteStorageTargetFormData {
	return {
		name: "",
		connector_id: descriptor.connector_id,
		values: Object.fromEntries(
			descriptor.fields.map((field) => [field.name, valueForField(field, {})]),
		),
		is_default: isDefault,
	};
}

export function getRemoteStorageTargetForm(
	target: RemoteStorageTargetInfo,
	descriptor?: RemoteStorageTargetConnectorDescriptor,
): RemoteStorageTargetFormData {
	const saved = target.connector_config.values;
	return {
		name: target.name,
		connector_id: target.connector_id,
		values: descriptor
			? Object.fromEntries(
					descriptor.fields.map((field) => [
						field.name,
						field.scope === "connector_config"
							? valueForField(field, saved)
							: valueForField(field, {}),
					]),
				)
			: { ...saved },
		is_default: target.is_default,
	};
}

function normalizedValue(value: RemoteStorageTargetFieldValue) {
	return typeof value === "string" ? value.trim() : value;
}

function splitValues(
	form: RemoteStorageTargetFormData,
	descriptor: RemoteStorageTargetConnectorDescriptor,
	preserveEmptyCredential: boolean,
) {
	const configValues: Record<string, RemoteStorageTargetFieldValue> = {};
	const credentialValues: Record<string, RemoteStorageTargetFieldValue> = {};
	for (const field of descriptor.fields) {
		const value = normalizedValue(valueForField(field, form.values));
		if (field.scope === "connector_config") {
			configValues[field.name] = value;
		} else if (
			field.scope === "static_credential" &&
			!(preserveEmptyCredential && value === "")
		) {
			credentialValues[field.name] = value;
		}
	}
	return { configValues, credentialValues };
}

function connectorConfig(
	descriptor: RemoteStorageTargetConnectorDescriptor,
	values: Record<string, RemoteStorageTargetFieldValue>,
) {
	return {
		format_version: 1,
		connector_id: descriptor.connector_id,
		schema_version: descriptor.config_schema_version,
		values,
	};
}

export function buildCreateRemoteStorageTargetPayload(
	form: RemoteStorageTargetFormData,
	descriptor: RemoteStorageTargetConnectorDescriptor,
): RemoteCreateStorageTargetRequest {
	const { configValues, credentialValues } = splitValues(
		form,
		descriptor,
		false,
	);
	return {
		name: form.name.trim(),
		connector_config: connectorConfig(descriptor, configValues),
		credential:
			Object.keys(credentialValues).length > 0
				? { mode: "static", values: credentialValues }
				: undefined,
		is_default: form.is_default,
	};
}

export function buildUpdateRemoteStorageTargetPayload(
	form: RemoteStorageTargetFormData,
	descriptor: RemoteStorageTargetConnectorDescriptor,
	editingTarget: RemoteStorageTargetInfo,
): RemoteUpdateStorageTargetRequest {
	const sameConnector = editingTarget.connector_id === descriptor.connector_id;
	const { configValues, credentialValues } = splitValues(
		form,
		descriptor,
		sameConnector,
	);
	return {
		name: form.name.trim(),
		connector_config: connectorConfig(descriptor, configValues),
		credential:
			Object.keys(credentialValues).length > 0
				? { mode: "static", values: credentialValues }
				: undefined,
		is_default: form.is_default,
	};
}

export const emptyRemoteStorageTargetForm: RemoteStorageTargetFormData = {
	name: "",
	connector_id: "",
	values: {},
	is_default: false,
};
