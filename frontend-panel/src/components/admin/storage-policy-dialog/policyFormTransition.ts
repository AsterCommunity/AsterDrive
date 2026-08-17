import type { StorageConnectorDescriptor } from "@/types/api";
import { normalizeConnectorConfigValues } from "./connectorFieldRules";
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
					? defaultStorageNativeThumbnailExtensions()
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
	// The extension vector is dormant configuration, not enabled state. Seed a
	// supported connector draft before the switch is turned on so the rules are
	// visible and editable without changing runtime behavior.
	const thumbnailExtensions =
		descriptor?.capabilities.storage_native_thumbnail === true &&
		form.storage_native_thumbnail_extensions.length === 0
			? defaultStorageNativeThumbnailExtensions()
			: form.storage_native_thumbnail_extensions;
	return {
		...form,
		connector_id: connectorId,
		connector_config_values: descriptorDefaultValues(descriptor),
		connector_config_explicit_fields: [],
		credential_values: {},
		storage_native_thumbnail_enabled: false,
		storage_native_thumbnail_extensions: thumbnailExtensions,
		storage_native_media_metadata_enabled: false,
	};
}

function defaultStorageNativeThumbnailExtensions(): string[] {
	return [...DEFAULT_STORAGE_NATIVE_THUMBNAIL_EXTENSIONS];
}

function descriptorDefaultValues(
	descriptor: StorageConnectorDescriptor | null | undefined,
): Record<string, ConnectorFormValue> {
	if (!descriptor) {
		return {};
	}
	return normalizeConnectorConfigValues({}, descriptor);
}
