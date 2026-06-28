import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import { RemoteNodeDialog } from "@/components/admin/admin-remote-nodes-page/RemoteNodeDialog";
import { RemoteNodeEnrollmentDialog } from "@/components/admin/admin-remote-nodes-page/RemoteNodeEnrollmentDialog";
import { RemoteNodesTable } from "@/components/admin/admin-remote-nodes-page/RemoteNodesTable";
import { hasCompletedRemoteNodeEnrollment } from "@/components/admin/admin-remote-nodes-page/shared";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { useAdminRemoteNodesPageController } from "./useAdminRemoteNodesPageController";

export default function AdminRemoteNodesPage() {
	const controller = useAdminRemoteNodesPageController();
	const {
		copyToClipboard,
		createButtonTitle,
		createManagedIngressProfile,
		createStep,
		createStepTouched,
		currentPage,
		deleteDialogProps,
		deleteManagedIngressProfile,
		deleteNodeName,
		deletingRemoteNodeId,
		dialogOpen,
		editingId,
		editingNode,
		enrollmentCommand,
		enrollmentCommandCanTest,
		enrollmentDialogOpen,
		form,
		generatingEnrollmentId,
		handleCreateBack,
		handleCreateNext,
		handleCreateStepChange,
		handleDialogOpenChange,
		handleEnrollmentDialogOpenChange,
		handleGenerateEnrollmentCommand,
		handlePageSizeChange,
		handleRefresh,
		handleSortChange,
		handleSubmit,
		handleVerifyEnrollmentConnection,
		loading,
		managedIngressProfiles,
		managedIngressDriverDescriptors,
		managedIngressDriverDescriptorsError,
		managedIngressDriverDescriptorsLoading,
		managedIngressProfilesError,
		managedIngressProfilesLoading,
		nextPageDisabled,
		openCreate,
		openEdit,
		pageSize,
		pageSizeOptions,
		prevPageDisabled,
		remoteNodes,
		requestConfirm,
		runConnectionTest,
		setField,
		setOffset,
		sortBy,
		sortOrder,
		submitting,
		t,
		total,
		totalPages,
		updateManagedIngressProfile,
	} = controller;

	return (
		<AdminLayout>
			<AdminPageShell>
				<AdminPageHeader
					title={t("remote_nodes")}
					description={t("remote_nodes_intro")}
					actions={
						<>
							<Button
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={openCreate}
								title={createButtonTitle}
							>
								<Icon name="Plus" className="mr-1 size-4" />
								{t("new_remote_node")}
							</Button>
							<Button
								variant="outline"
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() => void handleRefresh()}
								disabled={loading}
							>
								<Icon
									name={loading ? "Spinner" : "ArrowsClockwise"}
									className={`mr-1 size-3.5 ${loading ? "animate-spin" : ""}`}
								/>
								{t("core:refresh")}
							</Button>
						</>
					}
				/>

				<RemoteNodesTable
					loading={loading}
					items={remoteNodes}
					deletingRemoteNodeId={deletingRemoteNodeId}
					generatingEnrollmentId={generatingEnrollmentId}
					onEdit={openEdit}
					onGenerateEnrollmentCommand={(node) =>
						void handleGenerateEnrollmentCommand(node)
					}
					onRequestDelete={requestConfirm}
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={handleSortChange}
				/>

				<AdminOffsetPagination
					total={total}
					currentPage={currentPage}
					totalPages={totalPages}
					pageSize={String(pageSize)}
					pageSizeOptions={pageSizeOptions}
					onPageSizeChange={handlePageSizeChange}
					prevDisabled={prevPageDisabled}
					nextDisabled={nextPageDisabled}
					onPrevious={() =>
						setOffset((current) => Math.max(0, current - pageSize))
					}
					onNext={() => setOffset((current) => current + pageSize)}
				/>

				<ConfirmDialog
					{...deleteDialogProps}
					title={`${t("delete_remote_node")} "${deleteNodeName}"?`}
					description={t("delete_remote_node_desc")}
					confirmLabel={t("core:delete")}
					variant="destructive"
				/>
				<RemoteNodeDialog
					open={dialogOpen}
					mode={editingId === null ? "create" : "edit"}
					form={form}
					editingNode={editingNode}
					submitting={submitting}
					createStep={createStep}
					createStepTouched={createStepTouched}
					managedIngressProfilesEnabled={
						editingId !== null &&
						editingNode !== null &&
						hasCompletedRemoteNodeEnrollment(editingNode)
					}
					managedIngressProfiles={managedIngressProfiles}
					managedIngressDriverDescriptors={managedIngressDriverDescriptors}
					managedIngressDriverDescriptorsError={
						managedIngressDriverDescriptorsError
					}
					managedIngressDriverDescriptorsLoading={
						managedIngressDriverDescriptorsLoading
					}
					managedIngressProfilesLoading={managedIngressProfilesLoading}
					managedIngressProfilesError={managedIngressProfilesError}
					onFieldChange={setField}
					onOpenChange={handleDialogOpenChange}
					onRunConnectionTest={() => runConnectionTest()}
					onSubmit={handleSubmit}
					onCreateBack={handleCreateBack}
					onCreateNext={handleCreateNext}
					onCreateStepChange={handleCreateStepChange}
					onCreateManagedIngressProfile={createManagedIngressProfile}
					onUpdateManagedIngressProfile={updateManagedIngressProfile}
					onDeleteManagedIngressProfile={deleteManagedIngressProfile}
				/>
				<RemoteNodeEnrollmentDialog
					open={enrollmentDialogOpen}
					command={enrollmentCommand}
					canTestConnection={enrollmentCommandCanTest}
					onCopy={copyToClipboard}
					onVerifyConnection={handleVerifyEnrollmentConnection}
					onOpenChange={handleEnrollmentDialogOpenChange}
				/>
			</AdminPageShell>
		</AdminLayout>
	);
}
