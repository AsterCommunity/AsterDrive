import type { PolicyFormData } from "./formTypes";

export interface MicrosoftGraphCredentialForm {
	client_id: string;
	client_secret: string;
	scopes: string;
}

export function microsoftGraphCredentials(
	form: PolicyFormData,
): MicrosoftGraphCredentialForm {
	return {
		client_id: form.credential_values.client_id ?? "",
		client_secret: form.credential_values.client_secret ?? "",
		scopes: form.credential_values.scopes ?? "",
	};
}

export function updateMicrosoftGraphCredentials(
	form: PolicyFormData,
	patch: Partial<MicrosoftGraphCredentialForm>,
) {
	return {
		...form.credential_values,
		...patch,
	};
}
