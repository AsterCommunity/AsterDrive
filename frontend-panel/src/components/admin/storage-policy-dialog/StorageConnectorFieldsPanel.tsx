import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { translateStorageConnectorMessage } from "@/lib/adminStorageConnectorLocalizations";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { cn } from "@/lib/utils";
import type {
	RemoteNodeInfo,
	RemoteStorageTargetInfo,
	StorageConnectorDescriptor,
	StorageConnectorFieldDescriptor,
	StorageConnectorFieldValue,
} from "@/types/api";
import { normalizeConnectorFieldValue } from "./connectionNormalization";
import { connectorFormValue, type PolicyFormData } from "./formTypes";
import type { Translate } from "./StoragePolicyFieldTypes";

interface StorageConnectorFieldsPanelProps {
	descriptor: StorageConnectorDescriptor | null;
	fields?: StorageConnectorFieldDescriptor[];
	form: PolicyFormData;
	mode: "create" | "edit";
	remoteNodes: RemoteNodeInfo[];
	remoteStorageTargets: RemoteStorageTargetInfo[];
	showRequiredErrors: boolean;
	t: Translate;
	onFieldChange: <K extends keyof PolicyFormData>(
		key: K,
		value: PolicyFormData[K],
	) => void;
}

/** Generic renderer for connector-owned config and credential schemas. */
export function StorageConnectorFieldsPanel({
	descriptor,
	fields: declaredFields,
	form,
	mode,
	remoteNodes,
	remoteStorageTargets,
	showRequiredErrors,
	t,
	onFieldChange,
}: StorageConnectorFieldsPanelProps) {
	const fields =
		declaredFields ??
		descriptor?.fields.filter((field) => field.scope !== "action_input") ??
		[];
	if (fields.length === 0) {
		return null;
	}

	return (
		<div className={cn("grid gap-4", fields.length > 1 && "md:grid-cols-2")}>
			{fields.map((field) => (
				<ConnectorField
					key={`${field.scope}:${field.name}`}
					descriptor={descriptor}
					field={field}
					form={form}
					mode={mode}
					remoteNodes={remoteNodes}
					remoteStorageTargets={remoteStorageTargets}
					showRequiredErrors={showRequiredErrors}
					t={t}
					onFieldChange={onFieldChange}
				/>
			))}
		</div>
	);
}

function ConnectorField({
	descriptor,
	field,
	form,
	mode,
	remoteNodes,
	remoteStorageTargets,
	showRequiredErrors,
	t,
	onFieldChange,
}: StorageConnectorFieldsPanelProps & {
	field: StorageConnectorFieldDescriptor;
}) {
	const connectorT: Translate = (key, values) =>
		translateStorageConnectorMessage(t, descriptor?.connector_id, key, values);
	const hasDuplicateName =
		descriptor?.fields.some(
			(candidate) => candidate !== field && candidate.name === field.name,
		) ?? false;
	const inputId = hasDuplicateName
		? `storage-connector-${field.scope}-${field.name}`
		: field.name;
	const value = fieldValue(form, field);
	const missing =
		showRequiredErrors &&
		field.required &&
		(() => {
			const resolved = value ?? field.default_value;
			return resolved === undefined || resolved === null || resolved === "";
		})();
	const errorMessage = missing
		? field.required_message_key
			? connectorT(field.required_message_key, {
					field: connectorT(field.label_key),
				})
			: t("policy_connector_field_required", {
					field: connectorT(field.label_key),
				})
		: null;

	if (field.kind === "boolean") {
		return (
			<div className="space-y-2 md:col-span-2">
				<div className="flex min-h-9 items-center justify-between gap-3 rounded-lg border border-border/70 px-3 py-2">
					<Label htmlFor={inputId}>{connectorT(field.label_key)}</Label>
					<Switch
						id={inputId}
						checked={(value ?? field.default_value) === true}
						onCheckedChange={(checked) =>
							setFieldValue(form, descriptor, field, checked, onFieldChange)
						}
					/>
				</div>
				<FieldHelp field={field} t={connectorT} />
			</div>
		);
	}

	const options = fieldOptions(
		field,
		remoteNodes,
		remoteStorageTargets,
		connectorT,
	);
	if (field.kind === "select") {
		const dependencyMissing = field.select?.depends_on
			? fieldValueByName(form, descriptor, field.select.depends_on) == null ||
				fieldValueByName(form, descriptor, field.select.depends_on) === ""
			: false;
		const selectedValue = selectValue(value ?? field.default_value);
		const selectedDescription = options.find(
			(option) => option.value === selectedValue,
		)?.description;
		return (
			<div className="space-y-2">
				<Label htmlFor={inputId}>{connectorT(field.label_key)}</Label>
				<Select
					items={options}
					value={selectedValue}
					disabled={dependencyMissing}
					onValueChange={(nextValue) => {
						const normalized = normalizeSelectValue(field, nextValue);
						setFieldValue(form, descriptor, field, normalized, onFieldChange);
					}}
				>
					<SelectTrigger id={inputId} aria-invalid={missing || undefined}>
						<SelectValue placeholder={field.placeholder ?? undefined} />
					</SelectTrigger>
					<SelectContent>
						{options.map((option) => (
							<SelectItem key={option.value} value={option.value}>
								{option.label}
							</SelectItem>
						))}
					</SelectContent>
				</Select>
				{selectedDescription ? (
					<p className="text-xs leading-5 text-muted-foreground">
						{selectedDescription}
					</p>
				) : null}
				<FieldMessages
					errorMessage={errorMessage}
					field={field}
					t={connectorT}
				/>
			</div>
		);
	}

	const displayedValue = value ?? field.default_value ?? "";
	return (
		<div className="space-y-2">
			<Label htmlFor={inputId}>{connectorT(field.label_key)}</Label>
			<Input
				id={inputId}
				type={
					field.kind === "number"
						? "number"
						: field.secret || field.kind === "secret"
							? "password"
							: "text"
				}
				value={
					typeof displayedValue === "string" ||
					typeof displayedValue === "number"
						? displayedValue
						: ""
				}
				min={field.validation?.min_integer ?? undefined}
				max={field.validation?.max_integer ?? undefined}
				maxLength={field.validation?.max_length ?? undefined}
				required={field.required}
				aria-invalid={missing || undefined}
				placeholder={
					mode === "edit" && field.scope !== "connector_config"
						? t("policy_editor_credentials_keep_placeholder")
						: (field.placeholder ?? undefined)
				}
				autoComplete={field.secret ? "new-password" : "off"}
				className={ADMIN_CONTROL_HEIGHT_CLASS}
				onChange={(event) => {
					const nextValue =
						field.kind === "number"
							? Number.isFinite(event.target.valueAsNumber)
								? event.target.valueAsNumber
								: undefined
							: event.target.value;
					setFieldValue(form, descriptor, field, nextValue, onFieldChange);
				}}
				onBlur={(event) => {
					if (field.kind !== "number") {
						const normalized = normalizeConnectorFieldValue(
							field,
							event.target.value,
						);
						if (normalized === undefined) {
							return;
						}
						setFieldValue(form, descriptor, field, normalized, onFieldChange);
					}
				}}
			/>
			<FieldMessages errorMessage={errorMessage} field={field} t={connectorT} />
		</div>
	);
}

function fieldValueByName(
	form: PolicyFormData,
	descriptor: StorageConnectorDescriptor | null,
	fieldName: string,
) {
	const field = descriptor?.fields.find(
		(candidate) => candidate.name === fieldName,
	);
	return field ? fieldValue(form, field) : undefined;
}

function fieldValue(
	form: PolicyFormData,
	field: StorageConnectorFieldDescriptor,
) {
	return field.scope === "connector_config"
		? connectorFormValue(form, field.name)
		: form.credential_values[field.name];
}

function setFieldValue(
	form: PolicyFormData,
	descriptor: StorageConnectorDescriptor | null,
	field: StorageConnectorFieldDescriptor,
	value: StorageConnectorFieldValue | null | undefined,
	onFieldChange: StorageConnectorFieldsPanelProps["onFieldChange"],
) {
	const dependentNames = collectDependentFieldNames(descriptor, field);
	if (field.scope === "connector_config") {
		const values = { ...form.connector_config_values };
		if (value === undefined) {
			delete values[field.name];
		} else {
			values[field.name] = value;
		}
		for (const name of dependentNames) {
			delete values[name];
		}
		onFieldChange("connector_config_values", values);
		return;
	}
	const values = {
		...form.credential_values,
		[field.name]: typeof value === "string" ? value : String(value ?? ""),
	};
	for (const name of dependentNames) {
		delete values[name];
	}
	onFieldChange("credential_values", values);
}

function fieldOptions(
	field: StorageConnectorFieldDescriptor,
	remoteNodes: RemoteNodeInfo[],
	remoteStorageTargets: RemoteStorageTargetInfo[],
	t: Translate,
) {
	switch (field.select?.data_source) {
		case "remote_nodes":
			return remoteNodes.map((node) => ({
				description: undefined,
				label: node.name,
				value: String(node.id),
			}));
		case "remote_storage_targets":
			return remoteStorageTargets.map((target) => ({
				description: undefined,
				label: target.name || target.target_key,
				value: target.target_key,
			}));
	}
	return (field.select?.options ?? []).map((option) => ({
		description: option.description_key ? t(option.description_key) : undefined,
		label: t(option.label_key),
		value: String(option.value),
	}));
}

function normalizeSelectValue(
	field: StorageConnectorFieldDescriptor,
	value: string | null,
) {
	if (value == null) {
		return null;
	}
	return field.select?.value_kind === "integer" ? Number(value) : value;
}

function collectDependentFieldNames(
	descriptor: StorageConnectorDescriptor | null,
	field: StorageConnectorFieldDescriptor,
) {
	const pending = [field.name];
	const names = new Set<string>();
	while (pending.length > 0) {
		const dependency = pending.shift();
		for (const candidate of descriptor?.fields ?? []) {
			if (
				candidate.scope === field.scope &&
				candidate.select?.depends_on === dependency &&
				!names.has(candidate.name)
			) {
				names.add(candidate.name);
				pending.push(candidate.name);
			}
		}
	}
	return names;
}

function selectValue(value: unknown) {
	return typeof value === "string" || typeof value === "number"
		? String(value)
		: null;
}

function FieldMessages({
	errorMessage,
	field,
	t,
}: {
	errorMessage: string | null;
	field: StorageConnectorFieldDescriptor;
	t: Translate;
}) {
	return (
		<>
			{errorMessage ? (
				<p className="text-xs text-destructive">{errorMessage}</p>
			) : null}
			<FieldHelp field={field} t={t} />
		</>
	);
}

function FieldHelp({
	field,
	t,
}: {
	field: StorageConnectorFieldDescriptor;
	t: Translate;
}) {
	return field.help_key ? (
		<p className="text-xs leading-5 text-muted-foreground">
			{t(field.help_key)}
		</p>
	) : null;
}
