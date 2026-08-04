import { InlineConfirm } from "@/components/common/ManagerDialogShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
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
import type {
	StorageConnectorActionDescriptor,
	StorageConnectorFieldDescriptor,
	StorageConnectorFieldValue,
} from "@/types/api";
import type { Translate } from "./StoragePolicyFieldTypes";

export type StorageConnectorActionValues = Record<
	string,
	Record<string, StorageConnectorFieldValue>
>;

interface StorageConnectorActionsPanelProps {
	actions: StorageConnectorActionDescriptor[];
	connectorId?: string | null;
	confirmActionId: string | null;
	submittingActionId: string | null;
	t: Translate;
	values: StorageConnectorActionValues;
	onCancel: () => void;
	onConfirm: (actionId: string) => void;
	onRequest: (actionId: string) => void;
	onValueChange: (
		actionId: string,
		fieldName: string,
		value: StorageConnectorFieldValue | undefined,
	) => void;
}

export function StorageConnectorActionsPanel({
	actions,
	connectorId,
	confirmActionId,
	submittingActionId,
	t,
	values,
	onCancel,
	onConfirm,
	onRequest,
	onValueChange,
}: StorageConnectorActionsPanelProps) {
	const connectorT: Translate = (key, values) =>
		translateStorageConnectorMessage(t, connectorId, key, values);
	if (actions.length === 0) {
		return null;
	}

	return (
		<section className="space-y-3 border-t border-border/70 pt-4">
			{actions.map((action) => {
				const confirmOpen = confirmActionId === action.action_id;
				const submitting = submittingActionId === action.action_id;
				const actionValues = values[action.action_id] ?? {};

				return (
					<div
						key={action.action_id}
						className="space-y-3 border-b pb-4 last:border-b-0 last:pb-0"
					>
						<div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
							<div className="min-w-0 space-y-1">
								<p className="text-sm font-medium">
									{connectorT(action.label_key)}
								</p>
								<p className="text-xs leading-5 text-muted-foreground">
									{connectorT(action.description_key)}
								</p>
							</div>
							<Button
								type="button"
								variant="outline"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								disabled={submitting || confirmOpen}
								onClick={() => onRequest(action.action_id)}
							>
								{submitting ? (
									<Icon name="Spinner" className="mr-1 size-3.5 animate-spin" />
								) : null}
								{connectorT(action.label_key)}
							</Button>
						</div>

						{action.fields && action.fields.length > 0 ? (
							<div className="grid gap-3 md:grid-cols-2">
								{action.fields.map((field) => (
									<ActionField
										key={field.name}
										actionId={action.action_id}
										field={field}
										t={connectorT}
										value={actionValues[field.name]}
										onChange={onValueChange}
									/>
								))}
							</div>
						) : null}

						{confirmOpen ? (
							<InlineConfirm>
								<div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
									<div>
										<p className="text-sm font-medium">
											{t("policy_connector_action_confirm_title", {
												action: connectorT(action.label_key),
											})}
										</p>
										<p className="mt-1 text-xs leading-5 text-muted-foreground">
											{t("policy_connector_action_confirm_desc")}
										</p>
									</div>
									<div className="flex shrink-0 items-center gap-2">
										<Button
											type="button"
											variant="outline"
											className={ADMIN_CONTROL_HEIGHT_CLASS}
											disabled={submitting}
											onClick={onCancel}
										>
											{t("core:cancel")}
										</Button>
										<Button
											type="button"
											className={ADMIN_CONTROL_HEIGHT_CLASS}
											disabled={submitting}
											onClick={() => onConfirm(action.action_id)}
										>
											{submitting ? (
												<Icon
													name="Spinner"
													className="mr-1 size-3.5 animate-spin"
												/>
											) : null}
											{t("policy_connector_action_confirm")}
										</Button>
									</div>
								</div>
							</InlineConfirm>
						) : null}
					</div>
				);
			})}
		</section>
	);
}

function ActionField({
	actionId,
	field,
	t,
	value,
	onChange,
}: {
	actionId: string;
	field: StorageConnectorFieldDescriptor;
	t: Translate;
	value: StorageConnectorFieldValue | undefined;
	onChange: StorageConnectorActionsPanelProps["onValueChange"];
}) {
	const inputId = `storage-action-${actionId}-${field.name}`;
	const resolvedValue = value ?? field.default_value;

	if (field.kind === "boolean") {
		return (
			<div className="flex min-h-9 items-center justify-between gap-3">
				<Label htmlFor={inputId}>{t(field.label_key)}</Label>
				<Switch
					id={inputId}
					checked={resolvedValue === true}
					onCheckedChange={(checked) => onChange(actionId, field.name, checked)}
				/>
			</div>
		);
	}

	if (field.kind === "select") {
		const options = (field.select?.options ?? []).map((option) => ({
			label: t(option.label_key),
			value: String(option.value),
		}));
		return (
			<div className="space-y-2">
				<Label htmlFor={inputId}>{t(field.label_key)}</Label>
				<Select
					items={options}
					value={
						typeof resolvedValue === "string" ||
						typeof resolvedValue === "number"
							? String(resolvedValue)
							: null
					}
					onValueChange={(nextValue) => {
						const normalized =
							nextValue == null
								? undefined
								: field.select?.value_kind === "integer"
									? Number(nextValue)
									: nextValue;
						onChange(actionId, field.name, normalized);
					}}
				>
					<SelectTrigger id={inputId}>
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
			</div>
		);
	}

	return (
		<div className="space-y-2">
			<Label htmlFor={inputId}>{t(field.label_key)}</Label>
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
					typeof resolvedValue === "string" || typeof resolvedValue === "number"
						? resolvedValue
						: ""
				}
				required={field.required}
				placeholder={field.placeholder ?? undefined}
				autoComplete={field.secret ? "new-password" : "off"}
				className={ADMIN_CONTROL_HEIGHT_CLASS}
				onChange={(event) => {
					if (field.kind === "number") {
						const numberValue = event.target.valueAsNumber;
						onChange(
							actionId,
							field.name,
							Number.isFinite(numberValue) ? numberValue : undefined,
						);
						return;
					}
					onChange(actionId, field.name, event.target.value);
				}}
				onBlur={(event) => {
					if (field.trim_on_blur && field.kind !== "number") {
						onChange(actionId, field.name, event.target.value.trim());
					}
				}}
			/>
			{field.help_key ? (
				<p className="text-xs text-muted-foreground">{t(field.help_key)}</p>
			) : null}
		</div>
	);
}
