import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import { ExternalAuthCallbackDialog } from "@/components/admin/admin-external-auth-page/ExternalAuthCallbackDialog";
import { ExternalAuthProviderDialog } from "@/components/admin/admin-external-auth-page/ExternalAuthProviderDialog";
import { ExternalAuthProvidersTable } from "@/components/admin/admin-external-auth-page/ExternalAuthProvidersTable";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { EmptyState } from "@/components/common/EmptyState";
import { SkeletonTable } from "@/components/common/SkeletonTable";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { cn } from "@/lib/utils";
import { useAdminExternalAuthPageController } from "./useAdminExternalAuthPageController";

export default function AdminExternalAuthPage() {
	const controller = useAdminExternalAuthPageController();
	const {
		copyCallbackUrl,
		createStep,
		createStepDirection,
		createStepTouched,
		createSteps,
		currentPage,
		createdProviderCallback,
		deleteProviderName,
		deletingId,
		dialogOpen,
		dialogProps,
		editingProvider,
		form,
		goCreateBack,
		goCreateNext,
		goCreateStep,
		handleDialogOpenChange,
		handlePageSizeChange,
		loadProviders,
		loading,
		nextPageDisabled,
		openCreate,
		openEdit,
		pageSize,
		pageSizeOptions,
		prevPageDisabled,
		providerKinds,
		providers,
		requestConfirm,
		setCreatedProviderCallback,
		setField,
		setOffset,
		setProviderKind,
		submitProvider,
		submitting,
		t,
		testFormConnection,
		testProvider,
		testResult,
		testingId,
		total,
		totalPages,
	} = controller;

	return (
		<AdminLayout>
			<AdminPageShell>
				<AdminPageHeader
					title={t("external_auth")}
					description={t("external_auth_intro")}
					actions={
						<>
							<Button
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={openCreate}
							>
								<Icon name="Plus" className="mr-1 size-4" />
								{t("external_auth_provider_create")}
							</Button>
							<Button
								variant="outline"
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() => void loadProviders()}
								disabled={loading}
							>
								<Icon
									name={loading ? "Spinner" : "ArrowsClockwise"}
									className={cn("mr-1 size-3.5", loading && "animate-spin")}
								/>
								{t("core:refresh")}
							</Button>
						</>
					}
				/>

				{testResult ? (
					<div className="rounded-lg border border-emerald-200 bg-emerald-50 px-4 py-3 text-sm text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950/50 dark:text-emerald-200">
						{testResult}
					</div>
				) : null}

				{loading ? (
					<SkeletonTable columns={6} rows={6} />
				) : providers.length === 0 ? (
					<EmptyState
						icon={<Icon name="Globe" className="size-5" />}
						title={t("external_auth_providers_empty")}
						description={t("external_auth_providers_empty_desc")}
					/>
				) : (
					<ExternalAuthProvidersTable
						deletingId={deletingId}
						items={providers}
						onCopyCallbackUrl={(value) => void copyCallbackUrl(value)}
						onEdit={openEdit}
						onRequestDelete={requestConfirm}
						onTestProvider={(provider) => void testProvider(provider)}
						providerKinds={providerKinds}
						testingId={testingId}
					/>
				)}

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

				<ExternalAuthProviderDialog
					createStep={createStep}
					createStepDirection={createStepDirection}
					createStepTouched={createStepTouched}
					createSteps={createSteps}
					form={form}
					mode={editingProvider ? "edit" : "create"}
					onCreateBack={goCreateBack}
					onCreateNext={goCreateNext}
					onCreateStepChange={goCreateStep}
					open={dialogOpen}
					provider={editingProvider}
					providerKinds={providerKinds}
					submitting={submitting}
					onCopyCallbackUrl={(value) => void copyCallbackUrl(value)}
					onFieldChange={setField}
					onOpenChange={handleDialogOpenChange}
					onProviderKindChange={setProviderKind}
					onSubmit={() => void submitProvider()}
					onTestConnection={testFormConnection}
					testResult={testResult}
				/>

				<ExternalAuthCallbackDialog
					provider={createdProviderCallback}
					onCopy={(value) => void copyCallbackUrl(value)}
					onOpenChange={(open) => {
						if (!open) {
							setCreatedProviderCallback(null);
						}
					}}
				/>

				<ConfirmDialog
					{...dialogProps}
					title={t("external_auth_provider_delete_title", {
						name: deleteProviderName,
					})}
					description={t("external_auth_provider_delete_desc")}
					confirmLabel={t("core:delete")}
					variant="destructive"
				/>
			</AdminPageShell>
		</AdminLayout>
	);
}
