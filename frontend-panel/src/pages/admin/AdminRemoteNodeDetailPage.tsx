import {
	Navigate,
	useLocation,
	useNavigate,
	useParams,
} from "react-router-dom";
import { RemoteNodeEnrollmentPanel } from "@/components/admin/admin-remote-nodes-page/RemoteNodeEnrollmentPanel";
import { RemoteNodePage } from "@/components/admin/admin-remote-nodes-page/RemoteNodePage";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { usePageTitle } from "@/hooks/usePageTitle";
import type { RemoteEnrollmentCommandInfo } from "@/types/api";
import { useAdminRemoteNodeDetailController } from "./useAdminRemoteNodeDetailController";

export default function AdminRemoteNodeDetailPage() {
	const { nodeId, section } = useParams<{
		nodeId?: string;
		section?: string;
	}>();
	const parsedNodeId = Number(nodeId);
	if (!Number.isSafeInteger(parsedNodeId) || parsedNodeId <= 0) {
		return <Navigate to="/admin/remote-nodes" replace />;
	}
	if (section == null) {
		return (
			<Navigate to={`/admin/remote-nodes/${parsedNodeId}/overview`} replace />
		);
	}
	if (section !== "overview" && section !== "storage-targets") {
		return (
			<Navigate to={`/admin/remote-nodes/${parsedNodeId}/overview`} replace />
		);
	}
	return (
		<AdminRemoteNodeDetailContent
			remoteNodeId={parsedNodeId}
			pageTab={section}
		/>
	);
}

function AdminRemoteNodeDetailContent({
	remoteNodeId,
	pageTab,
}: {
	remoteNodeId: number;
	pageTab: "overview" | "storage-targets";
}) {
	const controller = useAdminRemoteNodeDetailController(remoteNodeId, pageTab);
	const navigate = useNavigate();
	const location = useLocation();
	usePageTitle(
		`${controller.t("edit_remote_node")} · ${
			pageTab === "overview"
				? controller.t("overview")
				: controller.t("remote_node_storage_targets_tab")
		}`,
	);
	const routeState = location.state as {
		enrollmentCommand?: RemoteEnrollmentCommandInfo;
		enrollmentError?: string;
	} | null;
	const enrollmentCommand =
		controller.enrollmentCommand ?? routeState?.enrollmentCommand ?? null;
	const enrollmentError =
		controller.enrollmentCommandError ?? routeState?.enrollmentError ?? null;
	return (
		<AdminLayout>
			<AdminPageShell>
				{controller.loading ? (
					<div className="flex items-center justify-center gap-2 py-16 text-sm text-muted-foreground">
						<Icon name="Spinner" className="size-4 animate-spin" />
						{controller.t("core:loading")}
					</div>
				) : controller.node && controller.form ? (
					<RemoteNodePage
						baseUrlValidationMessage={controller.baseUrlValidationMessage}
						deploymentPanel={
							controller.node.enrollment_status !== "completed" ? (
								<RemoteNodeEnrollmentPanel
									command={enrollmentCommand}
									errorMessage={enrollmentError}
									loading={controller.enrollmentCommandLoading}
									onCopy={controller.copyToClipboard}
									onGenerate={() => void controller.generateEnrollmentCommand()}
									onRetry={() => void controller.generateEnrollmentCommand()}
								/>
							) : null
						}
						editingNode={controller.node}
						form={controller.form}
						mode="edit"
						onBack={controller.navigateBack}
						onPageTabChange={(tab) => {
							if (tab !== "overview" && tab !== "storage-targets") return;
							navigate(`/admin/remote-nodes/${remoteNodeId}/${tab}`, {
								viewTransition: false,
							});
						}}
						onFieldChange={controller.setField}
						onRunConnectionTest={controller.runConnectionTest}
						onSubmit={() => void controller.submit()}
						pageBackLabel={controller.t("back_to_remote_nodes")}
						pageTab={pageTab}
						remoteStorageTargetConnectorDescriptors={
							controller.remoteStorageTargetConnectorDescriptors
						}
						remoteStorageTargetConnectorDescriptorsError={
							controller.remoteStorageTargetConnectorDescriptorsError
						}
						remoteStorageTargetConnectorDescriptorsLoading={
							controller.remoteStorageTargetConnectorDescriptorsLoading
						}
						remoteStorageTargets={controller.remoteStorageTargets}
						remoteStorageTargetsEnabled={
							controller.node.enrollment_status === "completed"
						}
						remoteStorageTargetsError={controller.remoteStorageTargetsError}
						remoteStorageTargetsLoading={controller.remoteStorageTargetsLoading}
						onCreateRemoteStorageTarget={controller.createRemoteStorageTarget}
						onDeleteRemoteStorageTarget={controller.deleteRemoteStorageTarget}
						onUpdateRemoteStorageTarget={controller.updateRemoteStorageTarget}
						submitting={controller.submitting}
					/>
				) : (
					<div className="flex flex-col items-center gap-4 py-16 text-center">
						<p className="text-sm text-muted-foreground">
							{controller.t("remote_node_not_found")}
						</p>
						<Button
							variant="outline"
							size="sm"
							onClick={controller.navigateBack}
						>
							<Icon name="ArrowLeft" className="mr-1 size-4" />
							{controller.t("back_to_remote_nodes")}
						</Button>
					</div>
				)}
			</AdminPageShell>
		</AdminLayout>
	);
}
