import { useTranslation } from "react-i18next";
import { RemoteNodeRemoteStorageTargetSection } from "@/components/admin/admin-remote-nodes-page/RemoteNodeRemoteStorageTargetSection";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type {
	RemoteCreateStorageTargetRequest,
	RemoteNodeInfo,
	RemoteStorageTargetDriverDescriptor,
	RemoteStorageTargetInfo,
	RemoteUpdateStorageTargetRequest,
} from "@/types/api";
import type { RemoteNodeFormData } from "../remoteNodeDialogShared";
import {
	RemoteNodeDiagnosticsCard,
	RemoteNodeSectionIntro,
	RemoteNodeSummaryCard,
} from "./RemoteNodeDialogCards";
import type {
	RemoteNodeFieldChangeHandler,
	RemoteNodeSummaryItem,
} from "./RemoteNodeDialogTypes";
import {
	type TransportModeOption,
	TransportModeSelector,
} from "./TransportModeSelector";

interface RemoteNodeEditFormProps {
	baseUrlValidationMessage: string | null;
	editingNode: RemoteNodeInfo | null;
	enabledToneClass: string;
	form: RemoteNodeFormData;
	remoteStorageTargetDriverDescriptors: RemoteStorageTargetDriverDescriptor[];
	remoteStorageTargetDriverDescriptorsError: string | null;
	remoteStorageTargetDriverDescriptorsLoading: boolean;
	remoteStorageTargets: RemoteStorageTargetInfo[];
	remoteStorageTargetsEnabled: boolean;
	remoteStorageTargetsError: string | null;
	remoteStorageTargetsLoading: boolean;
	modeToneClass: string;
	onCreateRemoteStorageTarget?: (
		payload: RemoteCreateStorageTargetRequest,
	) => Promise<void>;
	onDeleteRemoteStorageTarget?: (
		profile: RemoteStorageTargetInfo,
	) => Promise<void>;
	onFieldChange: RemoteNodeFieldChangeHandler;
	onUpdateRemoteStorageTarget?: (
		target_key: string,
		payload: RemoteUpdateStorageTargetRequest,
	) => Promise<void>;
	summaryItems: RemoteNodeSummaryItem[];
	transportOptions: TransportModeOption[];
}

export function RemoteNodeEditForm({
	baseUrlValidationMessage,
	editingNode,
	enabledToneClass,
	form,
	remoteStorageTargetDriverDescriptors,
	remoteStorageTargetDriverDescriptorsError,
	remoteStorageTargetDriverDescriptorsLoading,
	remoteStorageTargets,
	remoteStorageTargetsEnabled,
	remoteStorageTargetsError,
	remoteStorageTargetsLoading,
	modeToneClass,
	onCreateRemoteStorageTarget,
	onDeleteRemoteStorageTarget,
	onFieldChange,
	onUpdateRemoteStorageTarget,
	summaryItems,
	transportOptions,
}: RemoteNodeEditFormProps) {
	const { t } = useTranslation("admin");

	return (
		<div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_280px]">
			<div className="min-w-0 space-y-4">
				<section className="rounded-2xl border border-border/70 bg-background/70 p-5">
					<RemoteNodeSectionIntro
						title={t("remote_node_overview_title")}
						description={t("remote_node_overview_desc")}
					/>
					<div className="grid gap-4 md:grid-cols-2">
						<div className="space-y-2">
							<Label htmlFor="remote-node-name">{t("core:name")}</Label>
							<Input
								id="remote-node-name"
								value={form.name}
								onChange={(event) => onFieldChange("name", event.target.value)}
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								required
							/>
							<p className="text-xs text-muted-foreground">
								{t("remote_node_name_hint")}
							</p>
						</div>
						<div className="space-y-3 md:col-span-2">
							<Label id="remote-node-edit-transport-mode-label">
								{t("remote_node_transport_mode")}
							</Label>
							<TransportModeSelector
								ariaLabelledBy="remote-node-edit-transport-mode-label"
								options={transportOptions}
								value={form.transport_mode}
								onChange={(value) => onFieldChange("transport_mode", value)}
							/>
						</div>
						<div className="space-y-2 md:col-span-2">
							<Label htmlFor="remote-node-base-url">{t("base_url")}</Label>
							<Input
								id="remote-node-base-url"
								value={form.base_url}
								onChange={(event) =>
									onFieldChange("base_url", event.target.value)
								}
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								aria-invalid={baseUrlValidationMessage ? true : undefined}
								placeholder="https://remote.example.com"
							/>
							<p className="text-xs text-muted-foreground">
								{t("remote_node_base_url_hint")}
							</p>
							{baseUrlValidationMessage ? (
								<p className="text-xs text-destructive">
									{baseUrlValidationMessage}
								</p>
							) : null}
						</div>
					</div>
				</section>

				<section className="rounded-2xl border border-border/70 bg-background/70 p-5">
					<RemoteNodeSectionIntro
						title={t("remote_node_credentials_title")}
						description={t("remote_node_credentials_desc")}
					/>
					<div className="rounded-2xl border border-dashed border-border/70 bg-muted/10 p-4">
						<p className="text-sm leading-6 text-muted-foreground">
							{t("remote_node_wizard_auto_credentials_desc")}
						</p>
					</div>
				</section>

				{remoteStorageTargetsEnabled &&
				onCreateRemoteStorageTarget &&
				onUpdateRemoteStorageTarget &&
				onDeleteRemoteStorageTarget ? (
					<RemoteNodeRemoteStorageTargetSection
						targets={remoteStorageTargets}
						driverDescriptors={remoteStorageTargetDriverDescriptors}
						loading={
							remoteStorageTargetsLoading ||
							remoteStorageTargetDriverDescriptorsLoading
						}
						errorMessage={
							remoteStorageTargetsError ??
							remoteStorageTargetDriverDescriptorsError
						}
						onCreateTarget={onCreateRemoteStorageTarget}
						onUpdateTarget={onUpdateRemoteStorageTarget}
						onDeleteTarget={onDeleteRemoteStorageTarget}
					/>
				) : null}

				<section className="rounded-2xl border border-border/70 bg-background/70 p-5">
					<RemoteNodeSectionIntro
						title={t("remote_node_status_settings_title")}
						description={t("remote_node_status_settings_desc")}
					/>
					<div className="space-y-4">
						<div className="space-y-2">
							<div className="flex items-center gap-2">
								<Switch
									id="remote-node-enabled"
									checked={form.is_enabled}
									onCheckedChange={(value) =>
										onFieldChange("is_enabled", value)
									}
								/>
								<Label htmlFor="remote-node-enabled">
									{t("remote_node_enabled")}
								</Label>
							</div>
							<p className="text-xs text-muted-foreground">
								{t("remote_node_enabled_desc")}
							</p>
						</div>
					</div>
				</section>
			</div>

			<div className="min-w-0 space-y-4 lg:sticky lg:top-0 lg:self-start">
				<RemoteNodeSummaryCard
					description={t("policy_editor_summary_desc")}
					editingNode={editingNode}
					enabledToneClass={enabledToneClass}
					form={form}
					modeToneClass={modeToneClass}
					summaryItems={summaryItems}
				/>

				{editingNode ? (
					<RemoteNodeDiagnosticsCard editingNode={editingNode} />
				) : null}
			</div>
		</div>
	);
}
