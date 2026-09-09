import { translateStorageConnectorMessage } from "@/lib/adminStorageConnectorLocalizations";
import { ApiError } from "@/services/http";
import type {
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
} from "@/types/api";
import type { Translate } from "./StoragePolicyFieldTypes";

export type StorageConnectorFieldErrorScope =
	StorageConnectorFieldDescriptor["scope"];

export interface StorageConnectorFieldError {
	name: string;
	scope: StorageConnectorFieldErrorScope;
	message: string;
}

export type StorageConnectorFieldErrors = Record<string, string>;

export function storageConnectorFieldErrorKey(
	scope: StorageConnectorFieldErrorScope,
	name: string,
) {
	return `${scope}:${name}`;
}

/**
 * Converts a structured connector validation diagnostic into a localized form
 * error. Unknown diagnostics stay on the normal global error path.
 */
export function getStorageConnectorFieldError(
	error: unknown,
	descriptor: StorageConnectorDescriptor,
	t: Translate,
): StorageConnectorFieldError | null {
	if (!(error instanceof ApiError) || !error.diagnostic) {
		return null;
	}

	const diagnostic = error.diagnostic as typeof error.diagnostic & {
		field?: string | null;
		scope?: string | null;
	};
	if (!diagnostic.field || !diagnostic.scope) {
		return null;
	}

	const field = descriptor.fields.find(
		(candidate) =>
			candidate.name === diagnostic.field &&
			candidate.scope === diagnostic.scope &&
			candidate.scope !== "action_input",
	);
	if (!field) {
		return null;
	}

	const connectorT: Translate = (key, values) =>
		translateStorageConnectorMessage(t, descriptor.connector_id, key, values);
	const label = connectorT(field.label_key);
	const missing =
		diagnostic.message.includes("missing") ||
		diagnostic.message.includes(" is required");
	const message = missing
		? field.required_message_key
			? connectorT(field.required_message_key, { field: label })
			: t("policy_connector_field_required", { field: label })
		: t("policy_connector_field_invalid", { field: label });

	return {
		message,
		name: field.name,
		scope: field.scope,
	};
}
