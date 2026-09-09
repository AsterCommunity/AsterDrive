import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import { RemoteNodesTable } from "@/components/admin/admin-remote-nodes-page/RemoteNodesTable";
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
		createButtonTitle,
		currentPage,
		deleteDialogProps,
		deleteNodeName,
		deletingRemoteNodeId,
		handlePageSizeChange,
		handleRefresh,
		handleSortChange,
		loading,
		nextPageDisabled,
		openCreate,
		openEdit,
		pageSize,
		pageSizeOptions,
		prevPageDisabled,
		remoteNodes,
		requestConfirm,
		setOffset,
		sortBy,
		sortOrder,
		t,
		total,
		totalPages,
	} = controller;

	return (
		<AdminLayout>
			<AdminPageShell>
				<AdminPageHeader
					className="px-0 md:px-0"
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
					onEdit={openEdit}
					onRequestDelete={requestConfirm}
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={handleSortChange}
					pagination={
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
					}
				/>

				<ConfirmDialog
					{...deleteDialogProps}
					title={`${t("delete_remote_node")} "${deleteNodeName}"?`}
					description={t("delete_remote_node_desc")}
					confirmLabel={t("core:delete")}
					variant="destructive"
				/>
			</AdminPageShell>
		</AdminLayout>
	);
}
