import type { StorageConnectorDescriptor } from "@/types/api";
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
