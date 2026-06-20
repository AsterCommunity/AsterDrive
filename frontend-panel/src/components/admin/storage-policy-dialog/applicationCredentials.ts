import type { MicrosoftGraphCloud } from "@/types/api";
import type { PolicyFormData } from "./formTypes";

export type MicrosoftGraphCredentialForm = {
	cloud?: MicrosoftGraphCloud;
	tenant?: string;
	client_id: string;
	client_secret: string;
	scopes: string;
};

export type StorageApplicationCredentialForm = {
	microsoft_graph?: MicrosoftGraphCredentialForm;
};

export function microsoftGraphCredentials(
	form: PolicyFormData,
): MicrosoftGraphCredentialForm {
	return (
		form.application_credentials?.microsoft_graph ?? {
			cloud: form.onedrive_cloud,
			tenant: form.onedrive_tenant,
			client_id: "",
			client_secret: "",
			scopes: "",
		}
	);
}

export function updateMicrosoftGraphCredentials(
	form: PolicyFormData,
	patch: Partial<MicrosoftGraphCredentialForm>,
): StorageApplicationCredentialForm {
	const current = microsoftGraphCredentials(form);
	return {
		...form.application_credentials,
		microsoft_graph: {
			...current,
			...patch,
		},
	};
}
