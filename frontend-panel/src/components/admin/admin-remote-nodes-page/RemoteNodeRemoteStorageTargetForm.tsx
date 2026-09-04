import { useTranslation } from "react-i18next";
import type { RemoteStorageTargetFormData } from "@/components/admin/remoteStorageTargetDialogShared";
import { StorageConnectorFieldsPanel } from "@/components/admin/storage-policy-dialog/StorageConnectorFieldsPanel";
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
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type {
	RemoteStorageTargetInfo,
	StorageConnectorDescriptor,
} from "@/types/api";
import type {
	RemoteNodeRemoteStorageTargetDraftMode,
	RemoteNodeRemoteStorageTargetFieldChangeHandler,
} from "./RemoteNodeRemoteStorageTargetTypes";

const IS_DEFAULT_FIELD = "is_default";

interface RemoteNodeRemoteStorageTargetFormProps {
	defaultToggleLocked: boolean;
	connectorDescriptors: StorageConnectorDescriptor[];
	connectorIdError: string | null;
	draftMode: RemoteNodeRemoteStorageTargetDraftMode;
	form: RemoteStorageTargetFormData;
	nameError: string | null;
	onCancel: () => void;
	onFieldChange: RemoteNodeRemoteStorageTargetFieldChangeHandler;
	onSubmit: () => void;
	submitDisabled: boolean;
	submitting: boolean;
	targets: RemoteStorageTargetInfo[];
}

export function RemoteNodeRemoteStorageTargetForm({
	defaultToggleLocked,
	connectorDescriptors,
	connectorIdError,
	draftMode,
	form,
	nameError,
	onCancel,
	onFieldChange,
	onSubmit,
	submitDisabled,
	submitting,
	targets,
}: RemoteNodeRemoteStorageTargetFormProps) {
	const { t } = useTranslation("admin");
	const options = connectorDescriptors.map((descriptor) => ({
		label: t(descriptor.ui.label_key),
		value: descriptor.connector_id,
	}));
	const descriptor =
		connectorDescriptors.find(
			(candidate) => candidate.connector_id === form.connector_id,
		) ?? null;

	return (
		<div className="mt-4 rounded-2xl border border-border/70 bg-muted/10 p-4">
			<div className="flex flex-wrap items-start justify-between gap-3">
				<div>
					<h4 className="text-sm font-semibold text-foreground">
						{draftMode === "create"
							? t("remote_node_ingress_profile_form_create_title")
							: t("remote_node_ingress_profile_form_edit_title")}
					</h4>
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
						aria-invalid={nameError ? true : undefined}
					/>
					{nameError ? (
						<p className="text-xs text-destructive">{nameError}</p>
					) : null}
				</div>
				<div className="space-y-2">
					<Label htmlFor="remote-target-connector">{t("connector_type")}</Label>
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
					{connectorIdError ? (
						<p className="text-xs text-destructive">{connectorIdError}</p>
					) : null}
				</div>
			</div>

			<div className="mt-4">
				<StorageConnectorFieldsPanel
					descriptor={descriptor}
					fields={descriptor?.fields.filter(
						(field) => field.name !== IS_DEFAULT_FIELD,
					)}
					form={form}
					mode={draftMode}
					remoteNodes={[]}
					remoteStorageTargets={targets}
					showRequiredErrors
					t={t}
					onFieldChange={onFieldChange}
				/>
			</div>

			<div className="mt-4 flex items-center gap-2">
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

			<div className="mt-4 flex justify-end gap-2">
				<Button
					type="button"
					variant="outline"
					onClick={onCancel}
					disabled={submitting}
				>
					{t("core:cancel")}
				</Button>
				<Button
					type="button"
					className={ADMIN_CONTROL_HEIGHT_CLASS}
					onClick={onSubmit}
					disabled={submitDisabled}
				>
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
