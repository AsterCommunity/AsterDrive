import type { StorageConnectorDescriptor } from "@/types/api";
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
