import type { StoragePolicy } from "@/types/api";

export const DEFAULT_STORAGE_NATIVE_THUMBNAIL_EXTENSIONS = [
	"jpg",
	"jpeg",
	"png",
	"webp",
	"gif",
];

export type ConnectorFormValue = string | number | boolean | null;

export interface PolicyFormData {
	name: string;
	connector_id: string;
	connector_config_values: Record<string, ConnectorFormValue>;
	connector_config_explicit_fields?: string[];
	credential_values: Record<string, string>;
	max_file_size: string;
	chunk_size: string;
	is_default: boolean;
	storage_native_thumbnail_enabled: boolean;
	storage_native_thumbnail_extensions: string[];
	storage_native_media_metadata_enabled: boolean;
	storage_native_media_metadata_extensions: string[];
}

interface ConnectorConfigEnvelopeView {
	connector_id: string;
	values: Record<string, unknown>;
}

export function getPolicyForm(policy: StoragePolicy): PolicyFormData {
	const connectorConfig = parseConnectorConfigEnvelope(policy.connector_config);

	return {
		name: policy.name,
		connector_id: connectorConfig.connector_id || policy.connector_id,
		connector_config_values: normalizeConnectorConfigValues(
			connectorConfig.values,
		),
		credential_values: {},
		max_file_size:
			policy.max_file_size != null ? String(policy.max_file_size) : "",
		chunk_size:
			policy.chunk_size != null
				? String(Math.round(policy.chunk_size / 1024 / 1024))
				: "5",
		is_default: policy.is_default,
		storage_native_thumbnail_enabled:
			policy.behavior.storage_native_thumbnail_enabled === true,
		storage_native_thumbnail_extensions:
			policy.behavior.storage_native_thumbnail_extensions ?? [],
		storage_native_media_metadata_enabled:
			policy.behavior.storage_native_media_metadata_enabled === true,
		storage_native_media_metadata_extensions:
			policy.behavior.storage_native_media_metadata_extensions ?? [],
	};
}

export const emptyForm: PolicyFormData = {
	name: "",
	connector_id: "",
	connector_config_values: {},
	connector_config_explicit_fields: [],
	credential_values: {},
	max_file_size: "",
	chunk_size: "5",
	is_default: false,
	storage_native_thumbnail_enabled: false,
	storage_native_thumbnail_extensions: [],
	storage_native_media_metadata_enabled: false,
	storage_native_media_metadata_extensions: [],
};

export function connectorFormValue(
	form: PolicyFormData,
	fieldName: string,
): ConnectorFormValue | undefined {
	return form.connector_config_values[fieldName];
}

export function connectorStringValue(
	form: PolicyFormData,
	fieldName: string,
	fallback = "",
): string {
	const value = connectorFormValue(form, fieldName);
	return typeof value === "string" ? value : fallback;
}

export function connectorBooleanValue(
	form: PolicyFormData,
	fieldName: string,
	fallback = false,
): boolean {
	const value = connectorFormValue(form, fieldName);
	return typeof value === "boolean" ? value : fallback;
}

export function connectorNumberValue(
	form: PolicyFormData,
	fieldName: string,
): number | null {
	const value = connectorFormValue(form, fieldName);
	return typeof value === "number" && Number.isFinite(value) ? value : null;
}

export function withConnectorFormValue(
	form: PolicyFormData,
	fieldName: string,
	value: ConnectorFormValue,
): PolicyFormData {
	return {
		...form,
		connector_config_values: {
			...form.connector_config_values,
			[fieldName]: value,
		},
	};
}

export function updatedConnectorConfigValues(
	form: PolicyFormData,
	fieldName: string,
	value: ConnectorFormValue,
) {
	return {
		...form.connector_config_values,
		[fieldName]: value,
	};
}

export function updatedCredentialValues(
	form: PolicyFormData,
	fieldName: string,
	value: string,
) {
	return {
		...form.credential_values,
		[fieldName]: value,
	};
}

function parseConnectorConfigEnvelope(
	value: unknown,
): ConnectorConfigEnvelopeView {
	if (!isRecord(value)) {
		return { connector_id: "", values: {} };
	}
	return {
		connector_id:
			typeof value.connector_id === "string" ? value.connector_id : "",
		values: isRecord(value.values) ? value.values : {},
	};
}

function normalizeConnectorConfigValues(
	values: Record<string, unknown>,
): Record<string, ConnectorFormValue> {
	const normalized: Record<string, ConnectorFormValue> = {};
	for (const [key, value] of Object.entries(values)) {
		if (
			typeof value === "string" ||
			typeof value === "number" ||
			typeof value === "boolean" ||
			value === null
		) {
			normalized[key] = value;
		}
	}
	return normalized;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}
