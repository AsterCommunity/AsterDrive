import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type {
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
} from "@/types/api";
import { connectorBooleanValue, connectorStringValue } from "./formTypes";
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
	const staticCredentialFields =
		storageDriverDescriptor?.fields.filter(
			(field) => field.scope === "static_credential",
		) ?? [];
	const pathStyleField = fieldDescriptor(
		storageDriverDescriptor,
		"s3_path_style",
	);
	const showPathStyleField = isFieldVisibleForDriver(pathStyleField);
	const policyOptionTextFields = policyOptionTextFieldDescriptors(
		storageDriverDescriptor,
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
					value={connectorStringValue(form, "endpoint")}
					onChange={(e) =>
						onFieldChange("connector_config_values", {
							...form.connector_config_values,
							endpoint: e.target.value,
						})
					}
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
						value={connectorStringValue(form, "bucket")}
						onChange={(e) =>
							onFieldChange("connector_config_values", {
								...form.connector_config_values,
								bucket: e.target.value,
							})
						}
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
			{staticCredentialFields.length > 0 ? (
				<div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
					{staticCredentialFields.map((field) => (
						<StaticCredentialField
							key={field.name}
							field={field}
							form={form}
							isCreateMode={isCreateMode}
							onFieldChange={onFieldChange}
							t={t}
						/>
					))}
				</div>
			) : null}
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

function StaticCredentialField({
	field,
	form,
	isCreateMode,
	onFieldChange,
	t,
}: SharedFieldProps & {
	field: StorageConnectorFieldDescriptor;
	isCreateMode: boolean;
}) {
	const values = form.credential_values;
	const setValue = (value: string) => {
		onFieldChange("credential_values", {
			...values,
			[field.name]: value,
		});
	};

	return (
		<div className="space-y-2">
			<Label htmlFor={field.name}>{t(field.label_key)}</Label>
			<Input
				id={field.name}
				name={`storage-policy-${field.name}`}
				type={field.secret || field.kind === "secret" ? "password" : "text"}
				value={values[field.name] ?? ""}
				onChange={(event) => setValue(event.target.value)}
				onBlur={(event) => {
					if (field.trim_on_blur === true) {
						setValue(event.target.value.trim());
					}
				}}
				autoComplete={field.secret ? "new-password" : "off"}
				className={ADMIN_CONTROL_HEIGHT_CLASS}
				placeholder={
					field.placeholder ??
					(isCreateMode
						? undefined
						: t("policy_editor_credentials_keep_placeholder"))
				}
				required={isCreateMode && field.required}
			/>
			{field.help_key ? (
				<p className="text-xs text-muted-foreground">{t(field.help_key)}</p>
			) : null}
		</div>
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
	const optionValues = form.connector_config_values;
	const setPolicyOptionValue = (value: string) => {
		onFieldChange("connector_config_values", {
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
				value={String(optionValues[field.name] ?? "")}
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
					checked={connectorBooleanValue(form, "s3_path_style", true)}
					onCheckedChange={(value) =>
						onFieldChange("connector_config_values", {
							...form.connector_config_values,
							s3_path_style: value,
						})
					}
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
) {
	return (
		descriptor?.fields.filter(
			(field): field is StorageConnectorFieldDescriptor =>
				field.scope === "connector_config" &&
				(field.kind === "text" || field.kind === "secret") &&
				!["endpoint", "bucket", "base_path"].includes(field.name),
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
) {
	return field != null;
}
