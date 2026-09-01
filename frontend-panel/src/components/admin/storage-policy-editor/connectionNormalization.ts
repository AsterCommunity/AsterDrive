import type {
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
	StoragePolicy,
} from "@/types/api";
import {
	isConnectorFieldVisible,
	normalizeConnectorConfigValues as normalizeConnectorConfigValuesByRules,
} from "./connectorFieldRules";
import type { ConnectorFormValue, PolicyFormData } from "./formTypes";

interface ConnectorSelection {
	connector_id: string;
	connector_config_values: Record<string, ConnectorFormValue>;
}

export function normalizePolicyForm(
	form: PolicyFormData,
	descriptor?: StorageConnectorDescriptor | null,
): PolicyFormData {
	if (!descriptor) {
		return form;
	}

	const connectorConfigValues = normalizeFieldValues(
		form.connector_config_values,
		descriptor,
		"connector_config",
	);
	const credentialValues = normalizeCredentialValues(
		form.credential_values,
		descriptor,
	);

	if (
		recordsEqual(connectorConfigValues, form.connector_config_values) &&
		recordsEqual(credentialValues, form.credential_values)
	) {
		return form;
	}

	return {
		...form,
		connector_config_values: connectorConfigValues,
		credential_values: credentialValues,
	};
}

export function hasConnectionFieldChanges(
	form: PolicyFormData,
	editingPolicy: StoragePolicy | null,
	descriptor?: StorageConnectorDescriptor | null,
) {
	if (!editingPolicy) {
		return true;
	}

	const normalizedForm = normalizePolicyForm(form, descriptor);
	const saved = policyConnectorSelection(editingPolicy);
	return (
		normalizedForm.connector_id !== saved.connector_id ||
		!recordsEqual(
			normalizedForm.connector_config_values,
			saved.connector_config_values,
		) ||
		Object.values(normalizedForm.credential_values).some(
			(value) => value !== "",
		)
	);
}

export function getPolicyConnectionTestKey(
	form: PolicyFormData,
	descriptor?: StorageConnectorDescriptor | null,
) {
	const normalizedForm = normalizePolicyForm(form, descriptor);
	return JSON.stringify({
		connector_id: normalizedForm.connector_id,
		connector_config_values: normalizedForm.connector_config_values,
		credential_values: normalizedForm.credential_values,
	});
}

export function getEndpointValidationMessage(
	form: PolicyFormData,
	translate: (key: string) => string,
	descriptor?: StorageConnectorDescriptor | null,
) {
	const endpointField = descriptor?.fields.find(
		(field) =>
			field.scope === "connector_config" &&
			((field.allowed_endpoint_protocols?.length ?? 0) > 0 ||
				field.allow_endpoint_without_protocol === true ||
				field.invalid_protocol_message_key != null),
	);
	if (!endpointField) {
		return null;
	}

	const rawEndpoint = form.connector_config_values[endpointField.name];
	const endpoint = typeof rawEndpoint === "string" ? rawEndpoint.trim() : "";
	if (!endpoint) {
		return null;
	}
	const errorKey =
		endpointField.invalid_protocol_message_key ??
		"policy_connector_endpoint_protocol_invalid";
	if (!hasEndpointUrlScheme(endpoint)) {
		return endpointField.allow_endpoint_without_protocol
			? null
			: translate(errorKey);
	}

	let parsed: URL;
	try {
		parsed = new URL(endpoint);
	} catch {
		return translate(errorKey);
	}
	const allowedProtocols = endpointField.allowed_endpoint_protocols ?? [];
	return allowedProtocols.length === 0 ||
		allowedProtocols.includes(parsed.protocol)
		? null
		: translate(errorKey);
}

export function policyConnectorSelection(
	policy: StoragePolicy,
): ConnectorSelection {
	const envelope: Record<string, unknown> = isRecord(policy.connector_config)
		? policy.connector_config
		: {};
	const values = isRecord(envelope.values) ? envelope.values : {};
	const connectorConfigValues: Record<string, ConnectorFormValue> = {};
	for (const [key, value] of Object.entries(values)) {
		if (
			typeof value === "string" ||
			typeof value === "number" ||
			typeof value === "boolean" ||
			value === null
		) {
			connectorConfigValues[key] = value;
		}
	}
	return {
		connector_id: policy.connector_id,
		connector_config_values: connectorConfigValues,
	};
}

function normalizeFieldValues(
	values: Record<string, ConnectorFormValue>,
	descriptor: StorageConnectorDescriptor,
	scope: "connector_config",
) {
	const conditionedValues = normalizeConnectorConfigValuesByRules(
		values,
		descriptor,
	);
	const normalized: Record<string, ConnectorFormValue> = {};
	for (const field of descriptor.fields) {
		if (field.scope !== scope) {
			continue;
		}
		if (
			field.inactive_value_behavior === "clear" &&
			!isConnectorFieldVisible(field, conditionedValues)
		) {
			continue;
		}
		const value = normalizeConnectorFieldValue(
			field,
			conditionedValues[field.name],
		);
		if (value === undefined) {
			continue;
		}
		normalized[field.name] = value;
	}
	return normalized;
}

/** Mirror backend descriptor default and text-normalization semantics. */
export function normalizeConnectorFieldValue(
	field: StorageConnectorFieldDescriptor,
	value: ConnectorFormValue | undefined,
): ConnectorFormValue | undefined {
	let normalized =
		value === undefined ? (field.default_value ?? undefined) : value;
	if (field.trim_on_blur === true && typeof normalized === "string") {
		normalized = normalized.trim();
	}
	if (
		normalized === "" &&
		!field.required &&
		field.default_mode === "missing_or_empty_text" &&
		field.default_value != null
	) {
		normalized = field.default_value;
		if (field.trim_on_blur === true && typeof normalized === "string") {
			normalized = normalized.trim();
		}
	}
	return normalized;
}

function normalizeCredentialValues(
	values: Record<string, string>,
	descriptor: StorageConnectorDescriptor,
) {
	const normalized: Record<string, string> = {};
	for (const field of descriptor.fields) {
		if (
			field.scope !== "static_credential" &&
			field.scope !== "authorization_application"
		) {
			continue;
		}
		const value = values[field.name];
		if (value === undefined) {
			continue;
		}
		normalized[field.name] = field.trim_on_blur === true ? value.trim() : value;
	}
	return normalized;
}

function hasEndpointUrlScheme(endpoint: string) {
	return /^[a-z][a-z0-9+.-]*:\/\//i.test(endpoint);
}

function recordsEqual(
	left: Record<string, ConnectorFormValue> | Record<string, string>,
	right: Record<string, ConnectorFormValue> | Record<string, string>,
) {
	const leftKeys = Object.keys(left);
	const rightKeys = Object.keys(right);
	return (
		leftKeys.length === rightKeys.length &&
		leftKeys.every((key) => left[key] === right[key])
	);
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}
