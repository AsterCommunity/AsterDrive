import type {
	S3DownloadStrategy,
	S3UploadStrategy,
} from "@/components/admin/storagePolicyDialogShared";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type { SelectOption, SharedFieldProps } from "./StoragePolicyFieldTypes";
import { StrategySelectField } from "./StoragePolicyStrategyFields";

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
					placeholder="https://s3.amazonaws.com"
				/>
				{endpointValidationMessage ? (
					<p className="text-xs text-destructive">
						{endpointValidationMessage}
					</p>
				) : null}
				<p className="text-xs text-muted-foreground">{t("s3_endpoint_hint")}</p>
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

export function S3UploadStrategyField({
	form,
	onFieldChange,
	t,
}: SharedFieldProps) {
	const options = [
		{
			label: t("upload_strategy_relay_stream"),
			value: "relay_stream",
		},
		{
			label: t("upload_strategy_presigned"),
			value: "presigned",
		},
	] satisfies ReadonlyArray<SelectOption<S3UploadStrategy>>;

	return (
		<StrategySelectField
			id="s3_upload_strategy"
			label={t("s3_upload_strategy")}
			options={options}
			value={form.s3_upload_strategy}
			onChange={(value) => onFieldChange("s3_upload_strategy", value)}
			description={t(
				form.s3_upload_strategy === "relay_stream"
					? "upload_strategy_relay_stream_desc"
					: "upload_strategy_presigned_desc",
			)}
		/>
	);
}

export function S3DownloadStrategyField({
	form,
	onFieldChange,
	t,
}: SharedFieldProps) {
	const options = [
		{
			label: t("download_strategy_relay_stream"),
			value: "relay_stream",
		},
		{
			label: t("download_strategy_presigned"),
			value: "presigned",
		},
	] satisfies ReadonlyArray<SelectOption<S3DownloadStrategy>>;

	return (
		<StrategySelectField
			id="s3_download_strategy"
			label={t("s3_download_strategy")}
			options={options}
			value={form.s3_download_strategy}
			onChange={(value) => onFieldChange("s3_download_strategy", value)}
			description={t(
				form.s3_download_strategy === "relay_stream"
					? "download_strategy_relay_stream_desc"
					: "download_strategy_presigned_desc",
			)}
		/>
	);
}
