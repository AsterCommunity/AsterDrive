import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
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
	const endpointHintKey =
		form.driver_type === "tencent_cos"
			? "cos_endpoint_hint"
			: "s3_endpoint_hint";
	const endpointPlaceholder =
		form.driver_type === "tencent_cos"
			? "https://<bucket-appid>.cos.<region>.myqcloud.com"
			: "https://s3.amazonaws.com";

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
			<div className="grid grid-cols-2 gap-4">
				<div className="space-y-2">
					<Label htmlFor="access_key">{t("access_key")}</Label>
					<Input
						id="access_key"
						name="storage-policy-access-key"
						value={form.access_key}
						onChange={(e) => onFieldChange("access_key", e.target.value)}
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
					<Label htmlFor="secret_key">{t("secret_key")}</Label>
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
