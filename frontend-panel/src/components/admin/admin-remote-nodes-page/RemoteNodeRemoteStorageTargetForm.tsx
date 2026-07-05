import { useTranslation } from "react-i18next";
import {
	isRemoteStorageTargetDriverType,
	type RemoteStorageTargetFormData,
} from "@/components/admin/remoteStorageTargetDialogShared";
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
	RemoteStorageTargetDriverDescriptor,
	RemoteStorageTargetDriverFieldDescriptor,
	RemoteStorageTargetInfo,
} from "@/types/api";
import type {
	RemoteNodeRemoteStorageTargetDraftMode,
	RemoteNodeRemoteStorageTargetFieldChangeHandler,
} from "./RemoteNodeRemoteStorageTargetTypes";

interface RemoteNodeRemoteStorageTargetFormProps {
	accessKeyError: string | null;
	bucketError: string | null;
	defaultToggleLocked: boolean;
	driverDescriptors: RemoteStorageTargetDriverDescriptor[];
	driverTypeError: string | null;
	draftMode: RemoteNodeRemoteStorageTargetDraftMode;
	editingProfile: RemoteStorageTargetInfo | null;
	endpointError: string | null;
	form: RemoteStorageTargetFormData;
	localPathError: string | null;
	nameError: string | null;
	onCancel: () => void;
	onFieldChange: RemoteNodeRemoteStorageTargetFieldChangeHandler;
	onSubmit: () => void;
	secretKeyError: string | null;
	submitDisabled: boolean;
	submitting: boolean;
}

export function RemoteNodeRemoteStorageTargetForm({
	accessKeyError,
	bucketError,
	defaultToggleLocked,
	driverDescriptors,
	driverTypeError,
	draftMode,
	editingProfile,
	endpointError,
	form,
	localPathError,
	nameError,
	onCancel,
	onFieldChange,
	onSubmit,
	secretKeyError,
	submitDisabled,
	submitting,
}: RemoteNodeRemoteStorageTargetFormProps) {
	const { t } = useTranslation("admin");
	const driverTypeOptions = driverDescriptors.map((descriptor) => ({
		label: t(descriptor.label_key),
		value: descriptor.driver_type,
	}));
	const activeDriverDescriptor =
		driverDescriptors.find(
			(descriptor) => descriptor.driver_type === form.driver_type,
		) ?? null;
	const fieldByName = new Map(
		activeDriverDescriptor?.fields.map((field) => [field.name, field]) ?? [],
	);
	const field = (name: string) => fieldByName.get(name);
	const fieldHelp = (
		descriptor: RemoteStorageTargetDriverFieldDescriptor | undefined,
	) => (descriptor?.help_key ? t(descriptor.help_key) : null);
	const fieldPlaceholder = (
		descriptor: RemoteStorageTargetDriverFieldDescriptor | undefined,
	) => descriptor?.placeholder ?? undefined;
	const basePathField = field("base_path");
	const endpointField = field("endpoint");
	const bucketField = field("bucket");
	const accessKeyField = field("access_key");
	const secretKeyField = field("secret_key");
	const isDefaultField = field("is_default");
	const canPreserveSecretOnEdit =
		draftMode === "edit" &&
		editingProfile?.driver_type === form.driver_type &&
		secretKeyField?.secret === true;

	return (
		<div className="mt-4 rounded-2xl border border-border/70 bg-muted/10 p-4">
			<div className="flex flex-wrap items-start justify-between gap-3">
				<div>
					<h4 className="text-sm font-semibold text-foreground">
						{draftMode === "create"
							? t("remote_node_ingress_profile_form_create_title")
							: t("remote_node_ingress_profile_form_edit_title")}
					</h4>
					<p className="mt-1 text-xs leading-5 text-muted-foreground">
						{t("remote_node_ingress_profile_form_desc")}
					</p>
				</div>
				<Button
					type="button"
					variant="outline"
					size="sm"
					className={ADMIN_CONTROL_HEIGHT_CLASS}
					onClick={onCancel}
					disabled={submitting}
				>
					{t("core:cancel")}
				</Button>
			</div>

			<div className="mt-4 grid gap-4 md:grid-cols-2">
				<div className="space-y-2">
					<Label htmlFor="managed-ingress-name">{t("core:name")}</Label>
					<Input
						id="managed-ingress-name"
						value={form.name}
						onChange={(event) => onFieldChange("name", event.target.value)}
						className={ADMIN_CONTROL_HEIGHT_CLASS}
						aria-invalid={nameError ? true : undefined}
					/>
					{nameError ? (
						<p className="text-xs text-destructive">{nameError}</p>
					) : null}
				</div>

				<div className="space-y-2">
					<Label htmlFor="managed-ingress-driver">{t("driver_type")}</Label>
					<Select
						items={driverTypeOptions}
						value={form.driver_type}
						onValueChange={(value) => {
							if (isRemoteStorageTargetDriverType(value)) {
								onFieldChange("driver_type", value);
							}
						}}
					>
						<SelectTrigger
							id="managed-ingress-driver"
							className={ADMIN_CONTROL_HEIGHT_CLASS}
						>
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							{driverTypeOptions.map((option) => (
								<SelectItem key={option.value} value={option.value}>
									{option.label}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
					{driverTypeError ? (
						<p className="text-xs text-destructive">{driverTypeError}</p>
					) : null}
				</div>

				{basePathField ? (
					<div className="space-y-2">
						<Label htmlFor="managed-ingress-base-path">{t("base_path")}</Label>
						<Input
							id="managed-ingress-base-path"
							value={form.base_path}
							onChange={(event) =>
								onFieldChange("base_path", event.target.value)
							}
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							aria-invalid={localPathError ? true : undefined}
							placeholder={fieldPlaceholder(basePathField)}
						/>
						{fieldHelp(basePathField) ? (
							<p className="text-xs text-muted-foreground">
								{fieldHelp(basePathField)}
							</p>
						) : null}
						{localPathError ? (
							<p className="text-xs text-destructive">{localPathError}</p>
						) : null}
					</div>
				) : null}

				{endpointField || bucketField || accessKeyField || secretKeyField ? (
					<>
						{endpointField ? (
							<div className="space-y-2">
								<Label htmlFor="managed-ingress-endpoint">
									{t("endpoint")}
								</Label>
								<Input
									id="managed-ingress-endpoint"
									value={form.endpoint}
									onChange={(event) =>
										onFieldChange("endpoint", event.target.value)
									}
									className={ADMIN_CONTROL_HEIGHT_CLASS}
									aria-invalid={endpointError ? true : undefined}
									placeholder={fieldPlaceholder(endpointField)}
								/>
								{endpointError ? (
									<p className="text-xs text-destructive">{endpointError}</p>
								) : null}
							</div>
						) : null}

						{bucketField ? (
							<div className="space-y-2">
								<Label htmlFor="managed-ingress-bucket">{t("bucket")}</Label>
								<Input
									id="managed-ingress-bucket"
									value={form.bucket}
									onChange={(event) =>
										onFieldChange("bucket", event.target.value)
									}
									className={ADMIN_CONTROL_HEIGHT_CLASS}
									aria-invalid={bucketError ? true : undefined}
								/>
								{bucketError ? (
									<p className="text-xs text-destructive">{bucketError}</p>
								) : null}
							</div>
						) : null}

						{accessKeyField ? (
							<div className="space-y-2">
								<Label htmlFor="managed-ingress-access-key">
									{t("access_key")}
								</Label>
								<Input
									id="managed-ingress-access-key"
									value={form.access_key}
									onChange={(event) =>
										onFieldChange("access_key", event.target.value)
									}
									className={ADMIN_CONTROL_HEIGHT_CLASS}
									aria-invalid={accessKeyError ? true : undefined}
								/>
								{accessKeyError ? (
									<p className="text-xs text-destructive">{accessKeyError}</p>
								) : null}
							</div>
						) : null}

						{secretKeyField ? (
							<div className="space-y-2">
								<Label htmlFor="managed-ingress-secret-key">
									{t("secret_key")}
								</Label>
								<Input
									id="managed-ingress-secret-key"
									type="password"
									value={form.secret_key}
									onChange={(event) =>
										onFieldChange("secret_key", event.target.value)
									}
									className={ADMIN_CONTROL_HEIGHT_CLASS}
									aria-invalid={secretKeyError ? true : undefined}
									placeholder={canPreserveSecretOnEdit ? "••••••••" : undefined}
								/>
								<p className="text-xs text-muted-foreground">
									{canPreserveSecretOnEdit
										? t("remote_node_ingress_profile_credentials_optional_hint")
										: t("remote_node_ingress_profile_credentials_hint")}
								</p>
								{secretKeyError ? (
									<p className="text-xs text-destructive">{secretKeyError}</p>
								) : null}
							</div>
						) : null}
					</>
				) : activeDriverDescriptor ? (
					<div className="rounded-2xl border border-dashed border-border/70 bg-background/70 p-4 md:col-span-2">
						<p className="text-sm leading-6 text-muted-foreground">
							{activeDriverDescriptor.description_key
								? t(activeDriverDescriptor.description_key)
								: t(activeDriverDescriptor.label_key)}
						</p>
					</div>
				) : null}

				{isDefaultField ? (
					<div className="space-y-2 md:col-span-2">
						<div className="flex items-center gap-2">
							<Switch
								id="managed-ingress-default"
								checked={form.is_default}
								onCheckedChange={(value) => onFieldChange("is_default", value)}
								disabled={defaultToggleLocked}
							/>
							<Label htmlFor="managed-ingress-default">
								{t("remote_node_ingress_profile_default_toggle")}
							</Label>
						</div>
						<p className="text-xs text-muted-foreground">
							{defaultToggleLocked
								? t("remote_node_ingress_profile_default_locked_hint")
								: t("remote_node_ingress_profile_default_hint")}
						</p>
					</div>
				) : null}
			</div>

			<div className="mt-4 flex justify-end gap-2">
				<Button
					type="button"
					variant="outline"
					className={ADMIN_CONTROL_HEIGHT_CLASS}
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
