import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type {
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
} from "@/types/api";
import type { SharedFieldProps } from "./StoragePolicyFieldTypes";

export function ObjectStorageConnectionFields({
	bucketError,
	endpointValidationMessage,
	form,
	isCreateMode,
	onFieldChange,
	onSyncNormalizedObjectStorageForm,
	showCreateValidation = false,
	storageDriverDescriptor,
	t,
}: SharedFieldProps & {
	bucketError: string | null;
	endpointValidationMessage: string | null;
	isCreateMode: boolean;
	onSyncNormalizedObjectStorageForm: () => void;
	showCreateValidation?: boolean;
	storageDriverDescriptor?: StorageConnectorDescriptor | null;
}) {
	const endpointField = fieldDescriptor(storageDriverDescriptor, "endpoint");
	const bucketField = fieldDescriptor(storageDriverDescriptor, "bucket");
	const accessKeyField = fieldDescriptor(storageDriverDescriptor, "access_key");
	const secretKeyField = fieldDescriptor(storageDriverDescriptor, "secret_key");
	const pathStyleField = fieldDescriptor(
		storageDriverDescriptor,
		"s3_path_style",
	);
	const showPathStyleField = isFieldVisibleForDriver(
		pathStyleField,
		form.driver_type,
	);
	const policyOptionTextFields = policyOptionTextFieldDescriptors(
		storageDriverDescriptor,
		form.driver_type,
	);
	const hasBucketField = bucketField != null;

	return (
		<>
			<div className="space-y-2">
				<Label htmlFor="endpoint">
					{t(fieldLabelKey(endpointField, "endpoint"))}
				</Label>
				<Input
					id="endpoint"
					value={form.endpoint}
					onChange={(e) => onFieldChange("endpoint", e.target.value)}
					onBlur={onSyncNormalizedObjectStorageForm}
					aria-invalid={endpointValidationMessage ? true : undefined}
					className={ADMIN_CONTROL_HEIGHT_CLASS}
					placeholder={endpointField?.placeholder ?? "https://s3.amazonaws.com"}
				/>
				{endpointValidationMessage ? (
					<p className="text-xs text-destructive">
						{endpointValidationMessage}
					</p>
				) : null}
				{endpointField?.help_key ? (
					<p className="text-xs text-muted-foreground">
						{t(endpointField.help_key)}
					</p>
				) : null}
			</div>
			{hasBucketField ? (
				<div className="space-y-2">
					<Label htmlFor="bucket">
						{t(fieldLabelKey(bucketField, "bucket"))}
					</Label>
					<Input
						id="bucket"
						value={form.bucket}
						onChange={(e) => onFieldChange("bucket", e.target.value)}
						aria-invalid={
							showCreateValidation && bucketError ? true : undefined
						}
						className={ADMIN_CONTROL_HEIGHT_CLASS}
						required={bucketField.required}
					/>
					{showCreateValidation && bucketError ? (
						<p className="text-xs text-destructive">{bucketError}</p>
					) : null}
				</div>
			) : null}
			{showPathStyleField ? (
				<S3PathStyleField
					field={pathStyleField}
					form={form}
					t={t}
					onFieldChange={onFieldChange}
				/>
			) : null}
			<div className="grid grid-cols-2 gap-4">
				<div className="space-y-2">
					<Label htmlFor="access_key">
						{t(fieldLabelKey(accessKeyField, "access_key"))}
					</Label>
					<Input
						id="access_key"
						name="storage-policy-access-key"
						value={form.access_key}
						onChange={(e) => onFieldChange("access_key", e.target.value)}
						onBlur={(e) => {
							if (accessKeyField?.trim_on_blur === true) {
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
					<Label htmlFor="secret_key">
						{t(fieldLabelKey(secretKeyField, "secret_key"))}
					</Label>
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
			{policyOptionTextFields.map((field) => (
				<PolicyOptionTextField
					key={field.name}
					field={field}
					form={form}
					t={t}
					onFieldChange={onFieldChange}
				/>
			))}
		</>
	);
}

function PolicyOptionTextField({
	field,
	form,
	onFieldChange,
	t,
}: SharedFieldProps & {
	field: StorageConnectorFieldDescriptor;
}) {
	const optionValues = form.policy_option_values ?? {};
	const setPolicyOptionValue = (value: string) => {
		onFieldChange("policy_option_values", {
			...optionValues,
			[field.name]: value,
		});
	};

	return (
		<div className="space-y-2">
			<Label htmlFor={field.name}>{t(fieldLabelKey(field, field.name))}</Label>
			<Input
				id={field.name}
				type={field.kind === "secret" ? "password" : "text"}
				value={optionValues[field.name] ?? ""}
				onChange={(event) => setPolicyOptionValue(event.target.value)}
				onBlur={(event) => {
					if (field.trim_on_blur === true) {
						setPolicyOptionValue(event.target.value.trim());
					}
				}}
				autoComplete="off"
				className={ADMIN_CONTROL_HEIGHT_CLASS}
				placeholder={field.placeholder ?? undefined}
			/>
			{field.help_key ? (
				<p className="text-xs text-muted-foreground">{t(field.help_key)}</p>
			) : null}
		</div>
	);
}

function S3PathStyleField({
	field,
	form,
	onFieldChange,
	t,
}: SharedFieldProps & {
	field: StorageConnectorFieldDescriptor | null;
}) {
	return (
		<div className="space-y-2 pt-1">
			<div className="flex items-center gap-2">
				<Switch
					id="s3_path_style"
					checked={form.s3_path_style ?? true}
					onCheckedChange={(value) => onFieldChange("s3_path_style", value)}
				/>
				<Label htmlFor="s3_path_style">
					{t(fieldLabelKey(field, "s3_path_style"))}
				</Label>
			</div>
			{field?.help_key ? (
				<p className="text-xs text-muted-foreground">{t(field.help_key)}</p>
			) : null}
		</div>
	);
}

function fieldDescriptor(
	descriptor: StorageConnectorDescriptor | null | undefined,
	name: string,
) {
	return descriptor?.fields.find((field) => field.name === name) ?? null;
}

function policyOptionTextFieldDescriptors(
	descriptor: StorageConnectorDescriptor | null | undefined,
	driverType: string,
) {
	return (
		descriptor?.fields.filter(
			(field): field is StorageConnectorFieldDescriptor =>
				field.scope === "policy_options" &&
				(field.kind === "text" || field.kind === "secret") &&
				isFieldVisibleForDriver(field, driverType),
		) ?? []
	);
}

function fieldLabelKey(
	field: StorageConnectorFieldDescriptor | null,
	fallback: string,
) {
	return field?.label_key ?? fallback;
}

function isFieldVisibleForDriver(
	field: StorageConnectorFieldDescriptor | null,
	driverType: string,
) {
	if (!field) {
		return false;
	}
	const visibleDriverTypes = field.visible_when_driver_types ?? [];
	return (
		visibleDriverTypes.length === 0 ||
		visibleDriverTypes.includes(
			driverType as (typeof visibleDriverTypes)[number],
		)
	);
}
