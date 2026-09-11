import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type {
	AdminExternalAuthProviderInfo,
	AdminExternalAuthProviderKindInfo,
} from "@/types/api";
import type {
	ExternalAuthProviderFieldChange,
	ExternalAuthProviderFormData,
} from "./shared";

interface ExternalAuthConnectionFieldsProps {
	formTouched: boolean;
	form: ExternalAuthProviderFormData;
	onFieldChange: ExternalAuthProviderFieldChange;
	provider: AdminExternalAuthProviderInfo | null;
	selectedKind: AdminExternalAuthProviderKindInfo | null;
	showIssuerUrl: boolean;
	showManualEndpoints: boolean;
}

export function ExternalAuthConnectionFields({
	formTouched,
	form,
	onFieldChange,
	provider,
	selectedKind,
	showIssuerUrl,
	showManualEndpoints,
}: ExternalAuthConnectionFieldsProps) {
	const { t } = useTranslation("admin");

	return (
		<>
			{showIssuerUrl ? (
				<div className="space-y-2 md:col-span-2">
					<Label htmlFor="external-auth-provider-issuer">
						{t("external_auth_provider_issuer_url")}
					</Label>
					<Input
						id="external-auth-provider-issuer"
						value={form.issuerUrl}
						placeholder="https://id.example.com/application/o/asterdrive/"
						aria-invalid={
							formTouched &&
							selectedKind?.issuer_url_required &&
							!form.issuerUrl.trim()
								? true
								: undefined
						}
						onChange={(event) => onFieldChange("issuerUrl", event.target.value)}
					/>
				</div>
			) : null}
			{showManualEndpoints ? (
				<>
					<div className="space-y-2 md:col-span-2">
						<Label htmlFor="external-auth-provider-authorization-url">
							{t("external_auth_provider_authorization_url")}
						</Label>
						<Input
							id="external-auth-provider-authorization-url"
							value={form.authorizationUrl}
							placeholder="https://id.example.com/oauth/authorize"
							aria-invalid={
								formTouched &&
								selectedKind?.authorization_url_required &&
								!form.authorizationUrl.trim()
									? true
									: undefined
							}
							onChange={(event) =>
								onFieldChange("authorizationUrl", event.target.value)
							}
						/>
					</div>
					<div className="space-y-2">
						<Label htmlFor="external-auth-provider-token-url">
							{t("external_auth_provider_token_url")}
						</Label>
						<Input
							id="external-auth-provider-token-url"
							value={form.tokenUrl}
							placeholder="https://id.example.com/oauth/token"
							aria-invalid={
								formTouched &&
								selectedKind?.token_url_required &&
								!form.tokenUrl.trim()
									? true
									: undefined
							}
							onChange={(event) =>
								onFieldChange("tokenUrl", event.target.value)
							}
						/>
					</div>
					<div className="space-y-2">
						<Label htmlFor="external-auth-provider-userinfo-url">
							{t("external_auth_provider_userinfo_url")}
						</Label>
						<Input
							id="external-auth-provider-userinfo-url"
							value={form.userinfoUrl}
							placeholder="https://id.example.com/oauth/userinfo"
							aria-invalid={
								formTouched &&
								selectedKind?.userinfo_url_required &&
								!form.userinfoUrl.trim()
									? true
									: undefined
							}
							onChange={(event) =>
								onFieldChange("userinfoUrl", event.target.value)
							}
						/>
					</div>
				</>
			) : null}
			<div className="space-y-2">
				<Label htmlFor="external-auth-provider-client-id">
					{t("external_auth_provider_client_id")}
				</Label>
				<Input
					id="external-auth-provider-client-id"
					value={form.clientId}
					aria-invalid={formTouched && !form.clientId.trim() ? true : undefined}
					onChange={(event) => onFieldChange("clientId", event.target.value)}
				/>
			</div>
			<div className="space-y-2">
				<Label htmlFor="external-auth-provider-client-secret">
					{t("external_auth_provider_client_secret")}
				</Label>
				<Input
					id="external-auth-provider-client-secret"
					type="password"
					value={form.clientSecret}
					placeholder={
						provider?.client_secret_configured
							? t("external_auth_provider_secret_keep_placeholder")
							: ""
					}
					onChange={(event) =>
						onFieldChange("clientSecret", event.target.value)
					}
				/>
				<p className="text-xs text-muted-foreground">
					{provider?.client_secret_configured
						? t("external_auth_provider_secret_keep_hint")
						: t("external_auth_provider_secret_hint")}
				</p>
			</div>
		</>
	);
}
