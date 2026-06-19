import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import {
	microsoftGraphCredentials,
	updateMicrosoftGraphCredentials,
} from "./applicationCredentials";
import type { SharedFieldProps } from "./StoragePolicyFieldTypes";

export function OneDriveApplicationFields({
	clientIdError,
	clientSecretError,
	form,
	onFieldChange,
	showValidation = false,
	t,
	useSavedCredentialPlaceholder = false,
}: SharedFieldProps & {
	clientIdError?: string | null;
	clientSecretError?: string | null;
	showValidation?: boolean;
	useSavedCredentialPlaceholder?: boolean;
}) {
	const clientIdErrorId = "onedrive_client_id_error";
	const clientSecretErrorId = "onedrive_client_secret_error";
	const microsoftGraph = microsoftGraphCredentials(form);

	return (
		<div className="grid gap-4 md:grid-cols-2">
			<div className="space-y-2">
				<Label htmlFor="onedrive_client_id">{t("onedrive_client_id")}</Label>
				<Input
					id="onedrive_client_id"
					value={microsoftGraph.client_id}
					onChange={(event) =>
						onFieldChange(
							"application_credentials",
							updateMicrosoftGraphCredentials(form, {
								client_id: event.target.value,
							}),
						)
					}
					aria-invalid={showValidation && clientIdError ? true : undefined}
					aria-describedby={
						showValidation && clientIdError ? clientIdErrorId : undefined
					}
					className={ADMIN_CONTROL_HEIGHT_CLASS}
					autoComplete="off"
					placeholder={
						useSavedCredentialPlaceholder
							? t("onedrive_client_id_keep_placeholder")
							: t("onedrive_client_id_placeholder")
					}
					required={showValidation}
				/>
				{showValidation && clientIdError ? (
					<p id={clientIdErrorId} className="text-xs text-destructive">
						{clientIdError}
					</p>
				) : null}
			</div>
			<div className="space-y-2">
				<Label htmlFor="onedrive_client_secret">
					{t("onedrive_client_secret")}
				</Label>
				<Input
					id="onedrive_client_secret"
					type="password"
					value={microsoftGraph.client_secret}
					onChange={(event) =>
						onFieldChange(
							"application_credentials",
							updateMicrosoftGraphCredentials(form, {
								client_secret: event.target.value,
							}),
						)
					}
					aria-invalid={showValidation && clientSecretError ? true : undefined}
					aria-describedby={
						showValidation && clientSecretError
							? clientSecretErrorId
							: undefined
					}
					className={ADMIN_CONTROL_HEIGHT_CLASS}
					autoComplete="new-password"
					placeholder={
						useSavedCredentialPlaceholder
							? t("onedrive_client_secret_keep_placeholder")
							: t("onedrive_client_secret_placeholder")
					}
					required={showValidation}
				/>
				{showValidation && clientSecretError ? (
					<p id={clientSecretErrorId} className="text-xs text-destructive">
						{clientSecretError}
					</p>
				) : null}
			</div>
		</div>
	);
}
