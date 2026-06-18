import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type { SharedFieldProps } from "./StoragePolicyFieldTypes";

export function S3ConnectionFields({
	bucketError,
	endpointValidationMessage,
	form,
	isCreateMode,
	onFieldChange,
	onSyncNormalizedS3Form,
	showCreateValidation = false,
	t,
}: SharedFieldProps & {
	bucketError: string | null;
	endpointValidationMessage: string | null;
	isCreateMode: boolean;
	onSyncNormalizedS3Form: () => void;
	showCreateValidation?: boolean;
}) {
	// Broad availability comes from the connector descriptor. This component only
	// keeps provider-specific labels and placeholders until the form schema itself
	// becomes descriptor-rendered.
	const endpointHintKey =
		form.driver_type === "tencent_cos"
			? "cos_endpoint_hint"
			: form.driver_type === "azure_blob"
				? "azure_blob_endpoint_hint"
				: "s3_endpoint_hint";
	const endpointPlaceholder =
		form.driver_type === "tencent_cos"
			? "https://<bucket-appid>.cos.<region>.myqcloud.com"
			: form.driver_type === "azure_blob"
				? "https://<account>.blob.core.windows.net"
				: "https://s3.amazonaws.com";
	const accessKeyLabel =
		form.driver_type === "azure_blob"
			? "azure_blob_account_name"
			: "access_key";
	const secretKeyLabel =
		form.driver_type === "azure_blob" ? "azure_blob_account_key" : "secret_key";

	return (
		<>
			<div className="space-y-2">
				<Label htmlFor="endpoint">{t("endpoint")}</Label>
				<Input
					id="endpoint"
					value={form.endpoint}
					onChange={(e) => onFieldChange("endpoint", e.target.value)}
					onBlur={onSyncNormalizedS3Form}
					aria-invalid={endpointValidationMessage ? true : undefined}
					className={ADMIN_CONTROL_HEIGHT_CLASS}
					placeholder={endpointPlaceholder}
				/>
				{endpointValidationMessage ? (
					<p className="text-xs text-destructive">
						{endpointValidationMessage}
					</p>
				) : null}
				<p className="text-xs text-muted-foreground">{t(endpointHintKey)}</p>
			</div>
			<div className="space-y-2">
				<Label htmlFor="bucket">{t("bucket")}</Label>
				<Input
					id="bucket"
					value={form.bucket}
					onChange={(e) => onFieldChange("bucket", e.target.value)}
					aria-invalid={showCreateValidation && bucketError ? true : undefined}
					className={ADMIN_CONTROL_HEIGHT_CLASS}
					required
				/>
				{showCreateValidation && bucketError ? (
					<p className="text-xs text-destructive">{bucketError}</p>
				) : null}
			</div>
			{form.driver_type === "s3" ? (
				<S3PathStyleField form={form} t={t} onFieldChange={onFieldChange} />
			) : null}
			<div className="grid grid-cols-2 gap-4">
				<div className="space-y-2">
					<Label htmlFor="access_key">{t(accessKeyLabel)}</Label>
					<Input
						id="access_key"
						name="storage-policy-access-key"
						value={form.access_key}
						onChange={(e) => onFieldChange("access_key", e.target.value)}
						onBlur={(e) => {
							if (form.driver_type === "azure_blob") {
								onFieldChange("access_key", e.target.value.trim());
							}
						}}
						autoComplete="off"
						className={ADMIN_CONTROL_HEIGHT_CLASS}
						placeholder={
							isCreateMode
								? undefined
								: t("policy_editor_credentials_keep_placeholder")
						}
					/>
				</div>
				<div className="space-y-2">
					<Label htmlFor="secret_key">{t(secretKeyLabel)}</Label>
					<Input
						id="secret_key"
						name="storage-policy-secret-key"
						type="password"
						value={form.secret_key}
						onChange={(e) => onFieldChange("secret_key", e.target.value)}
						autoComplete="new-password"
						className={ADMIN_CONTROL_HEIGHT_CLASS}
						placeholder={
							isCreateMode
								? undefined
								: t("policy_editor_credentials_keep_placeholder")
						}
					/>
				</div>
			</div>
		</>
	);
}

function S3PathStyleField({ form, onFieldChange, t }: SharedFieldProps) {
	return (
		<div className="space-y-2 pt-1">
			<div className="flex items-center gap-2">
				<Switch
					id="s3_path_style"
					checked={form.s3_path_style ?? true}
					onCheckedChange={(value) => onFieldChange("s3_path_style", value)}
				/>
				<Label htmlFor="s3_path_style">{t("s3_path_style")}</Label>
			</div>
			<p className="text-xs text-muted-foreground">{t("s3_path_style_desc")}</p>
		</div>
	);
}
