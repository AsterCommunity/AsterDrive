import type { SetStateAction } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import { ExternalAuthCallbackDialog } from "@/components/admin/admin-external-auth-page/ExternalAuthCallbackDialog";
import { ExternalAuthProviderDialog } from "@/components/admin/admin-external-auth-page/ExternalAuthProviderDialog";
import { ExternalAuthProvidersTable } from "@/components/admin/admin-external-auth-page/ExternalAuthProvidersTable";
import {
	buildManagedExternalAuthSearchParams,
	connectionRequirementsMissing,
	createPayload,
	DEFAULT_EXTERNAL_AUTH_PAGE_SIZE,
	DEFAULT_SCOPES,
	EXTERNAL_AUTH_PAGE_SIZE_OPTIONS,
	type ExternalAuthCreateStep,
	type ExternalAuthProviderFormData,
	emptyForm,
	formatTestResultSummary,
	formConnectionChanged,
	formFromProvider,
	getManagedExternalAuthSearchString,
	mergeManagedExternalAuthSearchParams,
	normalizeOffset,
	requiredFieldsMissing,
	testParamsPayload,
	updatePayload,
} from "@/components/admin/admin-external-auth-page/shared";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { EmptyState } from "@/components/common/EmptyState";
import { SkeletonTable } from "@/components/common/SkeletonTable";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { AdminSurface } from "@/components/layout/AdminSurface";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { usePageTitle } from "@/hooks/usePageTitle";
import { writeTextToClipboard } from "@/lib/clipboard";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import {
	parseOffsetSearchParam,
	parsePageSizeOption,
	parsePageSizeSearchParam,
} from "@/lib/pagination";
import { cn } from "@/lib/utils";
import { adminExternalAuthService } from "@/services/adminService";
import type {
	AdminExternalAuthProviderInfo,
	AdminExternalAuthProviderKindInfo,
	ExternalAuthProviderKind,
	ExternalAuthProviderTestResult,
} from "@/types/api";

export default function AdminExternalAuthPage() {
	const { t } = useTranslation("admin");
	usePageTitle(t("external_auth"));
	const [searchParams, setSearchParams] = useSearchParams();
	const [offset, setOffsetState] = useState(
		normalizeOffset(parseOffsetSearchParam(searchParams.get("offset"))),
	);
	const [pageSize, setPageSize] = useState<
		(typeof EXTERNAL_AUTH_PAGE_SIZE_OPTIONS)[number]
	>(
		parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			EXTERNAL_AUTH_PAGE_SIZE_OPTIONS,
			DEFAULT_EXTERNAL_AUTH_PAGE_SIZE,
		),
	);
	const [providers, setProviders] = useState<AdminExternalAuthProviderInfo[]>(
		[],
	);
	const [providerKinds, setProviderKinds] = useState<
		AdminExternalAuthProviderKindInfo[]
	>([]);
	const [total, setTotal] = useState(0);
	const [loading, setLoading] = useState(true);
	const [dialogOpen, setDialogOpen] = useState(false);
	const [editingProvider, setEditingProvider] =
		useState<AdminExternalAuthProviderInfo | null>(null);
	const [form, setForm] = useState<ExternalAuthProviderFormData>(emptyForm);
	const [createStep, setCreateStep] = useState(0);
	const [createStepTouched, setCreateStepTouched] = useState(false);
	const [submitting, setSubmitting] = useState(false);
	const [testingId, setTestingId] = useState<number | null>(null);
	const [deletingId, setDeletingId] = useState<number | null>(null);
	const [testResult, setTestResult] = useState<string | null>(null);
	const [createdProviderCallback, setCreatedProviderCallback] =
		useState<AdminExternalAuthProviderInfo | null>(null);
	const lastWrittenSearchRef = useRef<string | null>(null);
	const setOffset = useCallback((value: SetStateAction<number>) => {
		setOffsetState((current) =>
			normalizeOffset(typeof value === "function" ? value(current) : value),
		);
	}, []);
	const enabledCount = useMemo(
		() => providers.filter((provider) => provider.enabled).length,
		[providers],
	);
	const providerKindCount = providerKinds.length;
	const totalPages = Math.max(1, Math.ceil(total / pageSize));
	const currentPage = Math.floor(offset / pageSize) + 1;
	const prevPageDisabled = offset === 0;
	const nextPageDisabled = offset + pageSize >= total;
	const pageSizeOptions = EXTERNAL_AUTH_PAGE_SIZE_OPTIONS.map((size) => ({
		label: t("page_size_option", { count: size }),
		value: String(size),
	}));
	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, EXTERNAL_AUTH_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setPageSize(next);
		setOffset(0);
	};
	const createSteps: ExternalAuthCreateStep[] = useMemo(
		() => [
			{
				title: t("external_auth_provider_wizard_step_type_title"),
				description: t("external_auth_provider_wizard_step_type_desc"),
			},
			{
				title: t("external_auth_provider_wizard_step_connection_title"),
				description: t("external_auth_provider_wizard_step_connection_desc"),
			},
			{
				title: t("external_auth_provider_wizard_step_rules_title"),
				description: t("external_auth_provider_wizard_step_rules_desc"),
			},
		],
		[t],
	);
	const previousCreateStepRef = useRef(createStep);
	const stepAnimationRef = useRef<{
		direction: "idle" | "forward" | "backward";
		step: number;
	}>({
		direction: "idle",
		step: createStep,
	});
	if (createStep !== previousCreateStepRef.current) {
		stepAnimationRef.current = {
			direction:
				createStep > previousCreateStepRef.current ? "forward" : "backward",
			step: createStep,
		};
	}
	const createStepDirection = stepAnimationRef.current.direction;

	useEffect(() => {
		const managedSearch = getManagedExternalAuthSearchString(searchParams);
		if (managedSearch === lastWrittenSearchRef.current) {
			return;
		}

		const nextOffset = normalizeOffset(
			parseOffsetSearchParam(searchParams.get("offset")),
		);
		const nextPageSize = parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			EXTERNAL_AUTH_PAGE_SIZE_OPTIONS,
			DEFAULT_EXTERNAL_AUTH_PAGE_SIZE,
		);

		setOffsetState((prev) => (prev === nextOffset ? prev : nextOffset));
		setPageSize((prev) => (prev === nextPageSize ? prev : nextPageSize));
	}, [searchParams]);

	useEffect(() => {
		const nextManagedSearchParams = buildManagedExternalAuthSearchParams({
			offset,
			pageSize,
		});
		const nextSearch = nextManagedSearchParams.toString();
		const currentSearch = getManagedExternalAuthSearchString(searchParams);
		if (
			currentSearch !== lastWrittenSearchRef.current &&
			currentSearch !== nextSearch
		) {
			return;
		}

		lastWrittenSearchRef.current = nextSearch;
		if (nextSearch === currentSearch) {
			return;
		}

		setSearchParams(
			mergeManagedExternalAuthSearchParams(
				searchParams,
				nextManagedSearchParams,
			),
			{ replace: true },
		);
	}, [offset, pageSize, searchParams, setSearchParams]);

	const loadProviders = useCallback(async () => {
		try {
			setLoading(true);
			const [kinds, providerList] = await Promise.all([
				adminExternalAuthService.listKinds(),
				adminExternalAuthService.list({
					limit: pageSize,
					offset,
				}),
			]);
			if (providerList.items.length === 0 && providerList.total > 0) {
				const maxOffset =
					Math.floor((providerList.total - 1) / pageSize) * pageSize;
				if (offset > maxOffset) {
					setOffset(maxOffset);
					return;
				}
			}
			setProviderKinds(kinds);
			setProviders(providerList.items);
			setTotal(providerList.total);
		} catch (error) {
			handleApiError(error);
		} finally {
			setLoading(false);
		}
	}, [offset, pageSize, setOffset]);

	useEffect(() => {
		void loadProviders();
	}, [loadProviders]);

	useEffect(() => {
		if (!dialogOpen || editingProvider) {
			previousCreateStepRef.current = 0;
			stepAnimationRef.current = {
				direction: "idle",
				step: 0,
			};
			return;
		}

		previousCreateStepRef.current = createStep;
	}, [createStep, dialogOpen, editingProvider]);

	const setField = <K extends keyof ExternalAuthProviderFormData>(
		key: K,
		value: ExternalAuthProviderFormData[K],
	) => {
		setTestResult(null);
		setForm((prev) => ({ ...prev, [key]: value }));
	};

	const setProviderKind = (kind: ExternalAuthProviderKind) => {
		const descriptor = providerKinds.find((item) => item.kind === kind);
		setTestResult(null);
		setForm((prev) => ({
			...prev,
			providerKind: kind,
			scopes: descriptor?.default_scopes || prev.scopes || DEFAULT_SCOPES,
		}));
	};

	const copyCallbackUrl = async (value: string) => {
		try {
			await writeTextToClipboard(value);
			toast.success(t("core:copied_to_clipboard"));
		} catch {
			toast.error(t("errors:unexpected_error"));
		}
	};

	const openCreate = () => {
		setEditingProvider(null);
		const firstKind = providerKinds[0];
		setForm({
			...emptyForm,
			providerKind: firstKind?.kind ?? "oidc",
			scopes: firstKind?.default_scopes ?? DEFAULT_SCOPES,
		});
		setCreateStep(0);
		setCreateStepTouched(false);
		setTestResult(null);
		setDialogOpen(true);
		if (providerKinds.length === 0) {
			void adminExternalAuthService
				.listKinds()
				.then((kinds) => {
					setProviderKinds(kinds);
					const nextKind = kinds[0];
					if (nextKind) {
						setForm((prev) => ({
							...prev,
							providerKind: nextKind.kind,
							scopes: nextKind.default_scopes || DEFAULT_SCOPES,
						}));
					}
				})
				.catch(handleApiError);
		}
	};

	const openEdit = (provider: AdminExternalAuthProviderInfo) => {
		setEditingProvider(provider);
		setForm(formFromProvider(provider));
		setCreateStep(0);
		setCreateStepTouched(false);
		setTestResult(null);
		setDialogOpen(true);
	};

	const handleDialogOpenChange = (open: boolean) => {
		setDialogOpen(open);
		if (!open) {
			setEditingProvider(null);
			setForm(emptyForm);
			setCreateStep(0);
			setCreateStepTouched(false);
			setSubmitting(false);
		}
	};

	const canAdvanceCreateStep = () => {
		if (createStep === 0) {
			return providerKinds.length > 0;
		}
		if (createStep === 1) {
			const selectedKind =
				providerKinds.find((kind) => kind.kind === form.providerKind) ??
				providerKinds[0] ??
				null;
			return !requiredFieldsMissing(form, selectedKind);
		}
		return true;
	};

	const goCreateNext = () => {
		setCreateStepTouched(true);
		if (!canAdvanceCreateStep()) {
			return;
		}
		setCreateStep((step) => Math.min(step + 1, createSteps.length - 1));
		setCreateStepTouched(false);
	};

	const goCreateBack = () => {
		setCreateStep((step) => Math.max(step - 1, 0));
		setCreateStepTouched(false);
	};

	const goCreateStep = (step: number) => {
		setCreateStep(Math.max(0, Math.min(step, createSteps.length - 1)));
		setCreateStepTouched(false);
	};

	const submitProvider = async () => {
		if (submitting) return;

		setSubmitting(true);
		try {
			if (editingProvider) {
				const updated = await adminExternalAuthService.update(
					editingProvider.id,
					updatePayload(form),
				);
				setProviders((prev) =>
					prev.map((provider) =>
						provider.id === updated.id ? updated : provider,
					),
				);
				toast.success(t("external_auth_provider_updated"));
			} else {
				const created = await adminExternalAuthService.create(
					createPayload(form),
				);
				toast.success(t("external_auth_provider_created"));
				setCreatedProviderCallback(created);
			}
			await loadProviders();
			handleDialogOpenChange(false);
		} catch (error) {
			handleApiError(error);
		} finally {
			setSubmitting(false);
		}
	};

	const applyTestResult = (
		result: ExternalAuthProviderTestResult,
		options: { touchedProviderId?: number } = {},
	) => {
		setTestResult(formatTestResultSummary(t, result));
		toast.success(t("external_auth_provider_test_success"));
		if (options.touchedProviderId != null) {
			setProviders((prev) =>
				prev.map((item) =>
					item.id === options.touchedProviderId
						? { ...item, updated_at: new Date().toISOString() }
						: item,
				),
			);
		}
	};

	const testFormConnection = async () => {
		const selectedKind =
			providerKinds.find((kind) => kind.kind === form.providerKind) ??
			providerKinds[0] ??
			null;
		if (connectionRequirementsMissing(form, selectedKind)) {
			setCreateStepTouched(true);
			return false;
		}

		try {
			if (editingProvider && !formConnectionChanged(form, editingProvider)) {
				const result = await adminExternalAuthService.test(editingProvider.id);
				applyTestResult(result, { touchedProviderId: editingProvider.id });
				return true;
			}

			const result = await adminExternalAuthService.testParams(
				testParamsPayload(form),
			);
			applyTestResult(result);
			return true;
		} catch (error) {
			handleApiError(error);
			return false;
		}
	};

	const testProvider = async (provider: AdminExternalAuthProviderInfo) => {
		try {
			setTestingId(provider.id);
			const result = await adminExternalAuthService.test(provider.id);
			applyTestResult(result, { touchedProviderId: provider.id });
		} catch (error) {
			handleApiError(error);
		} finally {
			setTestingId(null);
		}
	};

	const deleteProvider = async (id: number) => {
		try {
			setDeletingId(id);
			await adminExternalAuthService.delete(id);
			const isLastItemOnPage = providers.length === 1;
			const nextOffset =
				isLastItemOnPage && offset > 0
					? Math.max(0, offset - pageSize)
					: offset;
			if (nextOffset !== offset) {
				setOffset(nextOffset);
			} else {
				await loadProviders();
			}
			toast.success(t("external_auth_provider_deleted"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setDeletingId(null);
		}
	};

	const {
		confirmId: deleteId,
		requestConfirm,
		dialogProps,
	} = useConfirmDialog<number>(deleteProvider);
	const deleteProviderName =
		deleteId == null
			? ""
			: (providers.find((provider) => provider.id === deleteId)?.display_name ??
				"");

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

				<div className="grid gap-4 md:grid-cols-3">
					<AdminSurface className="flex-none p-4">
						<p className="text-xs text-muted-foreground">
							{t("external_auth_providers_total")}
						</p>
						<p className="mt-1 text-2xl font-semibold">{total}</p>
					</AdminSurface>
					<AdminSurface className="flex-none p-4">
						<p className="text-xs text-muted-foreground">
							{t("external_auth_providers_enabled_page")}
						</p>
						<p className="mt-1 text-2xl font-semibold">{enabledCount}</p>
					</AdminSurface>
					<AdminSurface className="flex-none p-4">
						<p className="text-xs text-muted-foreground">
							{t("external_auth_provider_kinds_supported")}
						</p>
						<p className="mt-1 text-2xl font-semibold">{providerKindCount}</p>
					</AdminSurface>
				</div>

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
