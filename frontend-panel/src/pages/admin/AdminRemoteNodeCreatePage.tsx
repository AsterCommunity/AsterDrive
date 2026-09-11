import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { Navigate, useBlocker } from "react-router-dom";
import { RemoteNodePage } from "@/components/admin/admin-remote-nodes-page/RemoteNodePage";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { useAdminRemoteNodeCreateController } from "./useAdminRemoteNodeCreateController";

export default function AdminRemoteNodeCreatePage() {
	const controller = useAdminRemoteNodeCreateController();
	if (!controller.createAllowed) {
		return <Navigate to="/admin/remote-nodes" replace />;
	}
	return (
		<AdminLayout>
			<AdminPageShell>
				<RemoteNodePage
					baseUrlValidationMessage={controller.baseUrlValidationMessage}
					editingNode={null}
					form={controller.form}
					mode="create"
					onBack={controller.navigateBack}
					onFieldChange={controller.setField}
					onRunConnectionTest={() => Promise.resolve(false)}
					onSubmit={() => void controller.submit()}
					pageBackLabel={controller.t("back_to_remote_nodes")}
					submitting={controller.submitting}
				/>
				<RemoteNodeCreateNavigationGuard dirty={controller.createDirty} />
			</AdminPageShell>
		</AdminLayout>
	);
}

function RemoteNodeCreateNavigationGuard({ dirty }: { dirty: boolean }) {
	const { t } = useTranslation("admin");
	const blocker = useBlocker(dirty);
	const resetTimerRef = useRef<number | null>(null);

	useEffect(() => {
		return () => {
			if (resetTimerRef.current !== null) {
				window.clearTimeout(resetTimerRef.current);
			}
		};
	}, []);

	useEffect(() => {
		if (!dirty) return;
		const handleBeforeUnload = (event: BeforeUnloadEvent) => {
			event.preventDefault();
		};
		window.addEventListener("beforeunload", handleBeforeUnload);
		return () => window.removeEventListener("beforeunload", handleBeforeUnload);
	}, [dirty]);

	return (
		<ConfirmDialog
			open={blocker.state === "blocked"}
			onOpenChange={(open) => {
				if (open || blocker.state !== "blocked") return;
				if (resetTimerRef.current !== null) {
					window.clearTimeout(resetTimerRef.current);
				}
				resetTimerRef.current = window.setTimeout(() => {
					resetTimerRef.current = null;
					if (blocker.state === "blocked") blocker.reset();
				}, 0);
			}}
			title={t("remote_node_discard_title")}
			description={t("remote_node_discard_desc")}
			confirmLabel={t("remote_node_discard_confirm")}
			variant="destructive"
			onConfirm={() => {
				if (blocker.state === "blocked") blocker.proceed();
			}}
		/>
	);
}
