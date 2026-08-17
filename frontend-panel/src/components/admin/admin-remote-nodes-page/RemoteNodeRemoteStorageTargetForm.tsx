import { useTranslation } from "react-i18next";
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
import type {
	RemoteStorageTargetConnectorDescriptor,
	RemoteStorageTargetInfo,
} from "@/types/api";
import type {
	RemoteNodeRemoteStorageTargetDraftMode,
	RemoteNodeRemoteStorageTargetFieldChangeHandler,
	RemoteStorageTargetFormData,
} from "./RemoteNodeRemoteStorageTargetTypes";

interface Props {
	defaultToggleLocked: boolean;
	connectorDescriptors: RemoteStorageTargetConnectorDescriptor[];
	draftMode: RemoteNodeRemoteStorageTargetDraftMode;
	editingProfile: RemoteStorageTargetInfo | null;
	fieldErrors: ReadonlyMap<string, string>;
	form: RemoteStorageTargetFormData;
	onCancel: () => void;
	onFieldChange: RemoteNodeRemoteStorageTargetFieldChangeHandler;
	onSubmit: () => void;
	submitDisabled: boolean;
	submitting: boolean;
}

export function RemoteNodeRemoteStorageTargetForm({
	defaultToggleLocked,
	connectorDescriptors,
	draftMode,
	editingProfile,
	fieldErrors,
	form,
	onCancel,
	onFieldChange,
	onSubmit,
	submitDisabled,
	submitting,
}: Props) {
	const { t } = useTranslation("admin");
	const active =
		connectorDescriptors.find(
			(descriptor) => descriptor.connector_id === form.connector_id,
		) ?? null;
	const options = connectorDescriptors.map((descriptor) => ({
		label: t(descriptor.label_key),
		value: descriptor.connector_id,
	}));
	const preservesSavedCredential =
		draftMode === "edit" &&
		editingProfile?.connector_id === form.connector_id &&
		editingProfile.credential_configured;
	return (
		<div className="mt-4 rounded-2xl border border-border/70 bg-muted/10 p-4">
			<div className="flex items-start justify-between gap-3">
				<div>
					<h4 className="text-sm font-semibold">
						{t(
							draftMode === "create"
								? "remote_node_ingress_profile_form_create_title"
								: "remote_node_ingress_profile_form_edit_title",
						)}
					</h4>
					<p className="mt-1 text-xs text-muted-foreground">
						{t("remote_node_ingress_profile_form_desc")}
					</p>
				</div>
				<Button
					type="button"
					variant="outline"
					size="sm"
					onClick={onCancel}
					disabled={submitting}
				>
					{t("core:cancel")}
				</Button>
			</div>
			<div className="mt-4 grid gap-4 md:grid-cols-2">
				<div className="space-y-2">
					<Label htmlFor="remote-target-name">{t("core:name")}</Label>
					<Input
						id="remote-target-name"
						value={form.name}
						onChange={(event) => onFieldChange("name", event.target.value)}
						aria-invalid={fieldErrors.has("name") || undefined}
					/>
					{fieldErrors.get("name") ? (
						<p className="text-xs text-destructive">
							{fieldErrors.get("name")}
						</p>
					) : null}
				</div>
				<div className="space-y-2">
					<Label htmlFor="remote-target-connector">
						{t("remote_node_storage_target_connector")}
					</Label>
					<Select
						items={options}
						value={form.connector_id}
						onValueChange={(value) => {
							if (value != null) onFieldChange("connector_id", value);
						}}
					>
						<SelectTrigger id="remote-target-connector">
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							{options.map((option) => (
								<SelectItem key={option.value} value={option.value}>
									{option.label}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
					{fieldErrors.get("connector_id") ? (
						<p className="text-xs text-destructive">
							{fieldErrors.get("connector_id")}
						</p>
					) : null}
				</div>
				{active?.fields.map((field) => {
					const value = form.values[field.name] ?? "";
					const error = fieldErrors.get(field.name);
					const label = t(field.label_key);
					if (field.kind === "boolean")
						return (
							<div key={field.name} className="space-y-2">
								<div className="flex items-center gap-2">
									<Switch
										id={`remote-target-${field.name}`}
										checked={Boolean(value)}
										onCheckedChange={(next) =>
											onFieldChange("value", { name: field.name, value: next })
										}
									/>
									<Label htmlFor={`remote-target-${field.name}`}>{label}</Label>
								</div>
								{field.help_key ? (
									<p className="text-xs text-muted-foreground">
										{t(field.help_key)}
									</p>
								) : null}
							</div>
						);
					if (field.kind === "select" && field.select) {
						const selectOptions = (field.select.options ?? [])
							.filter((option) => option.value != null)
							.map((option) => ({
								label: t(option.label_key),
								value: String(option.value),
							}));
						return (
							<div key={field.name} className="space-y-2">
								<Label>{label}</Label>
								<Select
									items={selectOptions}
									value={String(value)}
									onValueChange={(next) => {
										if (next != null)
											onFieldChange("value", {
												name: field.name,
												value:
													field.select?.value_kind === "integer"
														? Number(next)
														: next,
											});
									}}
								>
									<SelectTrigger>
										<SelectValue />
									</SelectTrigger>
									<SelectContent>
										{selectOptions.map((option) => (
											<SelectItem key={option.value} value={option.value}>
												{option.label}
											</SelectItem>
										))}
									</SelectContent>
								</Select>
								{error ? (
									<p className="text-xs text-destructive">{error}</p>
								) : null}
							</div>
						);
					}
					return (
						<div key={field.name} className="space-y-2">
							<Label htmlFor={`remote-target-${field.name}`}>{label}</Label>
							<Input
								id={`remote-target-${field.name}`}
								type={
									field.secret
										? "password"
										: field.kind === "number"
											? "number"
											: "text"
								}
								value={String(value)}
								placeholder={
									field.secret && preservesSavedCredential
										? "••••••••"
										: (field.placeholder ?? undefined)
								}
								onChange={(event) =>
									onFieldChange("value", {
										name: field.name,
										value:
											field.kind === "number"
												? Number(event.target.value)
												: event.target.value,
									})
								}
								aria-invalid={Boolean(error) || undefined}
							/>
							{field.help_key ? (
								<p className="text-xs text-muted-foreground">
									{t(field.help_key)}
								</p>
							) : field.secret ? (
								<p className="text-xs text-muted-foreground">
									{t(
										preservesSavedCredential
											? "remote_node_ingress_profile_credentials_optional_hint"
											: "remote_node_ingress_profile_credentials_hint",
									)}
								</p>
							) : null}
							{error ? (
								<p className="text-xs text-destructive">{error}</p>
							) : null}
						</div>
					);
				})}
				<div className="space-y-2 md:col-span-2">
					<div className="flex items-center gap-2">
						<Switch
							id="remote-target-default"
							checked={form.is_default}
							onCheckedChange={(value) => onFieldChange("is_default", value)}
							disabled={defaultToggleLocked}
						/>
						<Label htmlFor="remote-target-default">
							{t("remote_node_ingress_profile_default_toggle")}
						</Label>
					</div>
					<p className="text-xs text-muted-foreground">
						{t(
							defaultToggleLocked
								? "remote_node_ingress_profile_default_locked_hint"
								: "remote_node_ingress_profile_default_hint",
						)}
					</p>
				</div>
			</div>
			<div className="mt-4 flex justify-end gap-2">
				<Button
					type="button"
					variant="outline"
					onClick={onCancel}
					disabled={submitting}
				>
					{t("core:cancel")}
				</Button>
				<Button type="button" onClick={onSubmit} disabled={submitDisabled}>
					<Icon
						name={submitting ? "Spinner" : "FloppyDisk"}
						className={`mr-1 size-4 ${submitting ? "animate-spin" : ""}`}
					/>
					{draftMode === "create" ? t("core:create") : t("save_changes")}
				</Button>
			</div>
		</div>
	);
}
