import type {
	CreatePolicyRequest,
	DriverType,
	MicrosoftGraphCloud,
	StorageConnectorDescriptor,
} from "@/types/api";
import type { StorageApplicationCredentialForm } from "./applicationCredentials";

export interface StorageApplicationConfigForm {
	driver_type: DriverType;
	onedrive_cloud: MicrosoftGraphCloud;
	onedrive_tenant: string;
	application_credentials: StorageApplicationCredentialForm;
}

export function buildStorageApplicationConfig(
	form: StorageApplicationConfigForm,
	descriptor?: StorageConnectorDescriptor | null,
): CreatePolicyRequest["application_config"] | undefined {
	if (!supportsMicrosoftGraphApplicationConfig(descriptor)) {
		return undefined;
	}
	return {
		microsoft_graph: buildMicrosoftGraphApplicationConfig(form),
	};
}

function supportsMicrosoftGraphApplicationConfig(
	descriptor?: StorageConnectorDescriptor | null,
) {
	return (
		descriptor?.fields.some(
			(field) =>
				field.scope === "application_credential" && field.name === "client_id",
		) ?? false
	);
}

function buildMicrosoftGraphApplicationConfig(
	form: StorageApplicationConfigForm,
) {
	const microsoftGraph = form.application_credentials.microsoft_graph;
	const scopes = parseMicrosoftGraphScopes(microsoftGraph?.scopes ?? "");
	return {
		cloud: microsoftGraph?.cloud ?? form.onedrive_cloud,
		tenant: microsoftGraph?.tenant || form.onedrive_tenant || undefined,
		client_id: microsoftGraph?.client_id || undefined,
		client_secret: microsoftGraph?.client_secret || undefined,
		scopes: scopes.length > 0 ? scopes : undefined,
	};
}

export function parseMicrosoftGraphScopes(value: string) {
	return Array.from(
		new Set(
			value
				.split(/\s+/)
				.map((scope) => scope.trim())
				.filter(Boolean),
		),
	);
}
