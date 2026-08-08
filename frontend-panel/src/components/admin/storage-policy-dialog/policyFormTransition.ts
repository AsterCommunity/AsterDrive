import type {
	StorageConnectorDescriptor,
	StorageConnectorTransitionPreview,
} from "@/types/api";
import { normalizeConnectorFieldValue } from "./connectionNormalization";
import type { ConnectorFormValue, PolicyFormData } from "./formTypes";

export function applyPolicyFormFieldChange<K extends keyof PolicyFormData>(
	form: PolicyFormData,
	key: K,
	value: PolicyFormData[K],
): PolicyFormData {
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
		thumbnail_processor: null,
		thumbnail_extensions: [],
		media_metadata_extensions: [],
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
		thumbnail_processor: transition.target_behavior.thumbnail_processor ?? null,
		thumbnail_extensions: transition.target_behavior.thumbnail_extensions ?? [],
		media_metadata_extensions:
			transition.target_behavior.media_metadata_extensions ?? [],
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
