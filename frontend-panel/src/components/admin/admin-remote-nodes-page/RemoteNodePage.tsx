import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { AdminDetailPageShell } from "@/components/layout/AdminDetailPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type {
	RemoteCreateStorageTargetRequest,
	RemoteNodeInfo,
	RemoteStorageTargetInfo,
	RemoteUpdateStorageTargetRequest,
	StorageConnectorDescriptor,
} from "@/types/api";
import type {
	RemoteNodeFormData,
	RemoteNodeTransportMode,
} from "../remoteNodePageShared";
import { RemoteNodeEditForm } from "./RemoteNodeEditForm";
import type { RemoteNodeFieldChangeHandler } from "./RemoteNodePageTypes";
import {
	getRemoteNodeEnrollmentStatusLabel,
	getRemoteNodeTransportLabel,
	getRemoteNodeTransportTone,
	hasCompletedRemoteNodeEnrollment,
	TestConnectionButton,
} from "./shared";

interface RemoteNodePageProps {
	baseUrlValidationMessage: string | null;
	deploymentPanel?: ReactNode;
	editingNode: RemoteNodeInfo | null;
	form: RemoteNodeFormData;
	mode: "create" | "edit";
	onBack: () => void;
	onPageTabChange?: (value: string) => void;
	onCreateRemoteStorageTarget?: (
		payload: RemoteCreateStorageTargetRequest,
	) => Promise<void>;
	onDeleteRemoteStorageTarget?: (
		profile: RemoteStorageTargetInfo,
	) => Promise<void>;
	onFieldChange: RemoteNodeFieldChangeHandler;
	onRunConnectionTest: () => Promise<boolean>;
	onSubmit: () => void;
	onUpdateRemoteStorageTarget?: (
		target_key: string,
		payload: RemoteUpdateStorageTargetRequest,
	) => Promise<void>;
	pageBackLabel: string;
	pageTab?: "overview" | "storage-targets";
	remoteStorageTargetConnectorDescriptors?: StorageConnectorDescriptor[];
	remoteStorageTargetConnectorDescriptorsError?: string | null;
	remoteStorageTargetConnectorDescriptorsLoading?: boolean;
	remoteStorageTargets?: RemoteStorageTargetInfo[];
	remoteStorageTargetsEnabled?: boolean;
	remoteStorageTargetsError?: string | null;
	remoteStorageTargetsLoading?: boolean;
	submitting: boolean;
}

export function RemoteNodePage({
	baseUrlValidationMessage,
	deploymentPanel,
	editingNode,
	form,
	mode,
	onBack,
	onPageTabChange,
	onCreateRemoteStorageTarget,
	onDeleteRemoteStorageTarget,
	onFieldChange,
	onRunConnectionTest,
	onSubmit,
	onUpdateRemoteStorageTarget,
	pageBackLabel,
	pageTab,
	remoteStorageTargetConnectorDescriptors = [],
	remoteStorageTargetConnectorDescriptorsError = null,
	remoteStorageTargetConnectorDescriptorsLoading = false,
	remoteStorageTargets = [],
	remoteStorageTargetsEnabled = false,
	remoteStorageTargetsError = null,
	remoteStorageTargetsLoading = false,
	submitting,
}: RemoteNodePageProps) {
	const { t } = useTranslation("admin");
	const isCreate = mode === "create";
	const formId = "remote-node-form";
	const normalizedTransportMode: RemoteNodeTransportMode =
		form.transport_mode === "direct" ||
		form.transport_mode === "reverse_tunnel" ||
		form.transport_mode === "auto"
			? form.transport_mode
			: "direct";
	const summaryItems = [
		{
			label: t("remote_node_transport_mode"),
			value: getRemoteNodeTransportLabel(t, normalizedTransportMode),
		},
		{
			label: t("base_url"),
			value: form.base_url || t("remote_node_base_url_empty"),
		},
		...(editingNode
			? [
					{
						label: t("remote_node_enrollment_status"),
						value: getRemoteNodeEnrollmentStatusLabel(
							t,
							editingNode.enrollment_status,
						),
					},
				]
			: [
					{
						label: t("remote_node_wizard_followup_label"),
						value: t("remote_node_wizard_followup_value"),
					},
				]),
		{
			label: t("remote_node_status"),
			value: form.is_enabled
				? t("remote_node_status_enabled")
				: t("remote_node_status_disabled"),
		},
	];
	const transportOptions = [
		{
			value: "direct" as const,
			label: t("remote_node_transport_direct"),
			description: t("remote_node_transport_direct_desc"),
		},
		{
			value: "reverse_tunnel" as const,
			label: t("remote_node_transport_reverse_tunnel"),
			description: t("remote_node_transport_reverse_tunnel_desc"),
		},
		{
			value: "auto" as const,
			label: t("remote_node_transport_auto"),
			description: t("remote_node_transport_auto_desc"),
		},
	];
	const canRunConnectionTest =
		editingNode !== null &&
		hasCompletedRemoteNodeEnrollment(editingNode) &&
		form.base_url === editingNode.base_url &&
		normalizedTransportMode === (editingNode.transport_mode ?? "direct") &&
		(normalizedTransportMode !== "direct" || Boolean(form.base_url.trim())) &&
		!baseUrlValidationMessage;
	return (
		<AdminDetailPageShell
			actions={
				isCreate ? (
					<Button
						type="submit"
						form={formId}
						className={ADMIN_CONTROL_HEIGHT_CLASS}
						disabled={
							submitting ||
							!form.name.trim() ||
							Boolean(baseUrlValidationMessage)
						}
					>
						<Icon name="Cloud" className="mr-1 size-4" />
						{t("remote_node_create_and_deploy")}
					</Button>
				) : pageTab === "storage-targets" ? null : (
					<>
						<TestConnectionButton
							onTest={onRunConnectionTest}
							disabled={submitting || !canRunConnectionTest}
						/>
						<Button
							type="submit"
							form={formId}
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							disabled={
								submitting ||
								!form.name.trim() ||
								Boolean(baseUrlValidationMessage)
							}
						>
							<Icon name="FloppyDisk" className="mr-1 size-4" />
							{t("save_changes")}
						</Button>
					</>
				)
			}
			backLabel={pageBackLabel}
			description={
				isCreate ? t("remote_nodes_intro") : t("remote_node_detail_desc")
			}
			onBack={onBack}
			title={
				isCreate
					? t("create_remote_node")
					: (editingNode?.name ?? t("edit_remote_node"))
			}
		>
			<form
				autoComplete="off"
				id={formId}
				onSubmit={(event) => {
					event.preventDefault();
					onSubmit();
				}}
			>
				<RemoteNodeEditForm
					baseUrlValidationMessage={baseUrlValidationMessage}
					deploymentPanel={deploymentPanel}
					editingNode={editingNode}
					enabledToneClass={
						form.is_enabled
							? "border-emerald-500/60 bg-emerald-500/10 text-emerald-600 dark:text-emerald-300"
							: "border-slate-500/40 bg-slate-500/10 text-slate-600 dark:text-slate-300"
					}
					form={form}
					modeToneClass={getRemoteNodeTransportTone(normalizedTransportMode)}
					pageTab={pageTab}
					onPageTabChange={onPageTabChange}
					remoteStorageTargetConnectorDescriptors={
						remoteStorageTargetConnectorDescriptors
					}
					remoteStorageTargetConnectorDescriptorsError={
						remoteStorageTargetConnectorDescriptorsError
					}
					remoteStorageTargetConnectorDescriptorsLoading={
						remoteStorageTargetConnectorDescriptorsLoading
					}
					remoteStorageTargets={remoteStorageTargets}
					remoteStorageTargetsEnabled={remoteStorageTargetsEnabled}
					remoteStorageTargetsError={remoteStorageTargetsError}
					remoteStorageTargetsLoading={remoteStorageTargetsLoading}
					onCreateRemoteStorageTarget={onCreateRemoteStorageTarget}
					onDeleteRemoteStorageTarget={onDeleteRemoteStorageTarget}
					onFieldChange={onFieldChange}
					onUpdateRemoteStorageTarget={onUpdateRemoteStorageTarget}
					summaryItems={summaryItems}
					transportOptions={transportOptions}
				/>
			</form>
		</AdminDetailPageShell>
	);
}
