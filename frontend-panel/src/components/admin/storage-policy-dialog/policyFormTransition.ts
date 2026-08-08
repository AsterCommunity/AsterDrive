import type {
	StorageConnectorDescriptor,
	StorageConnectorTransitionPreview,
} from "@/types/api";
import { normalizeConnectorFieldValue } from "./connectionNormalization";
import {
	type ConnectorFormValue,
	DEFAULT_STORAGE_NATIVE_THUMBNAIL_EXTENSIONS,
	type PolicyFormData,
} from "./formTypes";

export function applyPolicyFormFieldChange<K extends keyof PolicyFormData>(
	form: PolicyFormData,
	key: K,
	value: PolicyFormData[K],
): PolicyFormData {
	if (key === "storage_native_thumbnail_enabled") {
		const enabled = value as boolean;
		return {
			...form,
			storage_native_thumbnail_enabled: enabled,
			storage_native_thumbnail_extensions:
				enabled && form.storage_native_thumbnail_extensions.length === 0
					? [...DEFAULT_STORAGE_NATIVE_THUMBNAIL_EXTENSIONS]
					: form.storage_native_thumbnail_extensions,
		};
	}
	return { ...form, [key]: value };
}

/**
 * Start a clean connector-owned draft.
 *
 * Connector changes never carry opaque config or credentials across plugin
 * boundaries. Defaults come exclusively from the target descriptor, so a new
 * connector can participate without adding a frontend branch.
 */
export function applyPolicyConnectorTransition(
	form: PolicyFormData,
	connectorId: string,
	descriptor: StorageConnectorDescriptor | null | undefined,
): PolicyFormData {
	return {
		...form,
		connector_id: connectorId,
		connector_config_values: descriptorDefaultValues(descriptor),
		credential_values: {},
		storage_native_thumbnail_enabled: false,
		storage_native_media_metadata_enabled: false,
	};
}

/** Apply a backend-resolved transition without sending browser-held secrets. */
export function applyRecommendedPolicyConnectorTransition(
	form: PolicyFormData,
	transition: StorageConnectorTransitionPreview,
	targetDescriptor: StorageConnectorDescriptor,
): PolicyFormData {
	const targetValues = isRecord(transition.target_connector_config.values)
		? transition.target_connector_config.values
		: {};
	const connectorConfigValues = descriptorDefaultValues(targetDescriptor);
	for (const field of targetDescriptor.fields) {
		if (field.scope !== "connector_config") {
			continue;
		}
		const value = normalizeConnectorFieldValue(field, targetValues[field.name]);
		if (value === undefined) {
			delete connectorConfigValues[field.name];
		} else {
			connectorConfigValues[field.name] = value;
		}
	}

	const credentialValues: Record<string, string> = {};
	for (const mapping of transition.field_mappings ?? []) {
		if (
			mapping.source_scope === "connector_config" ||
			mapping.source_scope === "action_input" ||
			mapping.target_scope === "connector_config" ||
			mapping.target_scope === "action_input"
		) {
			continue;
		}
		const value = form.credential_values[mapping.source_name];
		if (value !== undefined && value !== "") {
			credentialValues[mapping.target_name] = value;
		}
	}

	return {
		...form,
		connector_id: transition.target_connector_id,
		connector_config_values: connectorConfigValues,
		credential_values: credentialValues,
		storage_native_thumbnail_enabled:
			transition.target_behavior.storage_native_thumbnail_enabled === true,
		storage_native_thumbnail_extensions:
			transition.target_behavior.storage_native_thumbnail_extensions ?? [],
		storage_native_media_metadata_enabled:
			transition.target_behavior.storage_native_media_metadata_enabled === true,
		storage_native_media_metadata_extensions:
			transition.target_behavior.storage_native_media_metadata_extensions ?? [],
	};
}

function descriptorDefaultValues(
	descriptor: StorageConnectorDescriptor | null | undefined,
): Record<string, ConnectorFormValue> {
	const values: Record<string, ConnectorFormValue> = {};
	for (const field of descriptor?.fields ?? []) {
		if (field.scope !== "connector_config") {
			continue;
		}
		const value = normalizeConnectorFieldValue(field, undefined);
		if (value !== undefined) {
			values[field.name] = value;
		}
	}
	return values;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}
