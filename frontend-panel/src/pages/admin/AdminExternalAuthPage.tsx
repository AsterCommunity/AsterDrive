import { useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useBlocker, useNavigate } from "react-router-dom";
import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import { ExternalAuthCallbackDialog } from "@/components/admin/admin-external-auth-page/ExternalAuthCallbackDialog";
import { ExternalAuthProviderPage } from "@/components/admin/admin-external-auth-page/ExternalAuthProviderPage";
import {
	ExternalAuthProvidersTableHeader,
	ExternalAuthProvidersTableRow,
} from "@/components/admin/admin-external-auth-page/ExternalAuthProvidersTable";
import { AdminTableList } from "@/components/common/AdminTableList";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { cn } from "@/lib/utils";
import type { AdminExternalAuthPageVariant } from "./useAdminExternalAuthPageController";
import { useAdminExternalAuthPageController } from "./useAdminExternalAuthPageController";

export default function AdminExternalAuthPage({
	providerId,
	variant = "list",
}: {
	providerId?: number;
	variant?: AdminExternalAuthPageVariant;
}) {
	const navigate = useNavigate();
	const controller = useAdminExternalAuthPageController({
		providerId,
		variant,
	});
	const {
		copyCallbackUrl,
		createDirty,
		createStep,
		createStepDirection,
		createStepTouched,
		createSteps,
		currentPage,
		createdProviderCallback,
		deleteProviderName,
		deletingId,
		dialogProps,
		editingProvider,
		form,
		goCreateBack,
		goCreateNext,
		goCreateStep,
		handleCreatedProviderCallbackOpenChange,
		handlePageSizeChange,
		loadProviders,
		loading,
		navigateBackToProviders,
		nextPageDisabled,
		pageSize,
		pageSizeOptions,
		prevPageDisabled,
		providerKinds,
		providers,
		requestConfirm,
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
	const pageMode = variant !== "list";
	const providersEmptyIcon = useMemo(
		() => <Icon name="Globe" className="size-5" />,
		[],
	);
	const providersHeaderRow = useMemo(
		() => <ExternalAuthProvidersTableHeader />,
		[],
	);
	const providersPagination = useMemo(
		() => (
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
		),
		[
			currentPage,
			handlePageSizeChange,
			nextPageDisabled,
			pageSize,
			pageSizeOptions,
			prevPageDisabled,
			setOffset,
			total,
			totalPages,
		],
	);

	return (
		<AdminLayout>
			<AdminPageShell>
				{!pageMode ? (
					<>
						<AdminPageHeader
							className="px-0 md:px-0"
							title={t("external_auth")}
							description={t("external_auth_intro")}
							actions={
								<>
									<Button
										size="sm"
										className={ADMIN_CONTROL_HEIGHT_CLASS}
										onClick={() => {
											navigate("/admin/external-auth/new", {
												viewTransition: false,
											});
										}}
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
							<div className="rounded-lg bg-emerald-50 px-4 py-3 text-sm text-emerald-800 dark:bg-emerald-950/50 dark:text-emerald-200">
								{testResult}
							</div>
						) : null}

						<AdminTableList
							frameless
							loading={loading}
							items={providers}
							columns={6}
							rows={6}
							emptyIcon={providersEmptyIcon}
							emptyTitle={t("external_auth_providers_empty")}
							emptyDescription={t("external_auth_providers_empty_desc")}
							headerRow={providersHeaderRow}
							pagination={providersPagination}
							renderRow={(provider) => (
								<ExternalAuthProvidersTableRow
									key={provider.id}
									deletingId={deletingId}
									onCopyCallbackUrl={(value) => void copyCallbackUrl(value)}
									onEdit={(provider) => {
										navigate(`/admin/external-auth/${provider.id}`, {
											viewTransition: false,
										});
									}}
									onRequestDelete={requestConfirm}
									onTestProvider={(item) => void testProvider(item)}
									provider={provider}
									providerKinds={providerKinds}
									testingId={testingId}
								/>
							)}
						/>
					</>
				) : null}

				{!pageMode ? null : loading &&
					!editingProvider &&
					variant === "detail" ? (
					<div className="flex items-center justify-center gap-2 py-16 text-sm text-muted-foreground">
						<Icon name="Spinner" className="size-4 animate-spin" />
						{t("core:loading")}
					</div>
				) : pageMode && !loading && !editingProvider && variant === "detail" ? (
					<div className="flex flex-col items-center gap-4 py-16 text-center">
						<p className="text-sm text-muted-foreground">
							{t("external_auth_provider_not_found")}
						</p>
						<Button
							variant="outline"
							onClick={() =>
								navigate("/admin/external-auth", { viewTransition: false })
							}
						>
							<Icon name="ArrowLeft" className="mr-1 size-4" />
							{t("external_auth_back_to_providers")}
						</Button>
					</div>
				) : (
					<ExternalAuthProviderPage
						createStep={createStep}
						createStepDirection={createStepDirection}
						createStepTouched={createStepTouched}
						createSteps={createSteps}
						form={form}
						mode={editingProvider ? "edit" : "create"}
						onCreateBack={goCreateBack}
						onCreateNext={goCreateNext}
						onCreateStepChange={goCreateStep}
						pageBackLabel={t("external_auth_back_to_providers")}
						provider={editingProvider}
						providerKinds={providerKinds}
						submitting={submitting}
						onCopyCallbackUrl={(value) => void copyCallbackUrl(value)}
						onFieldChange={setField}
						onOpenChange={(open) => {
							if (!open) navigateBackToProviders();
						}}
						onProviderKindChange={setProviderKind}
						onSubmit={() => void submitProvider()}
						onTestConnection={testFormConnection}
						testResult={testResult}
					/>
				)}

				{variant === "create" ? (
					<ExternalAuthCreateNavigationGuard dirty={createDirty} />
				) : null}

				<ExternalAuthCallbackDialog
					provider={createdProviderCallback}
					onCopy={(value) => void copyCallbackUrl(value)}
					onOpenChange={handleCreatedProviderCallbackOpenChange}
				/>

				{!pageMode ? (
					<ConfirmDialog
						{...dialogProps}
						title={t("external_auth_provider_delete_title", {
							name: deleteProviderName,
						})}
						description={t("external_auth_provider_delete_desc")}
						confirmLabel={t("core:delete")}
						variant="destructive"
					/>
				) : null}
			</AdminPageShell>
		</AdminLayout>
	);
}

function ExternalAuthCreateNavigationGuard({ dirty }: { dirty: boolean }) {
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
			title={t("external_auth_provider_discard_title")}
			description={t("external_auth_provider_discard_desc")}
			confirmLabel={t("external_auth_provider_discard_confirm")}
			variant="destructive"
			onConfirm={() => {
				if (blocker.state === "blocked") blocker.proceed();
			}}
		/>
	);
}
