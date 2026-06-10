import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import { PoliciesTable } from "@/components/admin/admin-policies-page/PoliciesTable";
import { PolicyDialogs } from "@/components/admin/admin-policies-page/PolicyDialogs";
import { PROTECTED_POLICY_ID } from "@/components/admin/admin-policies-page/policyPresentation";
import { StoragePolicyMigrationDialog } from "@/components/admin/admin-policies-page/StoragePolicyMigrationDialog";
import {
	buildCreatePolicyPayload,
	buildPolicyTestPayload,
	buildUpdatePolicyPayload,
	DEFAULT_STORAGE_NATIVE_THUMBNAIL_EXTENSIONS,
	emptyForm,
	getEndpointValidationMessage,
	getPolicyConnectionTestKey,
	getPolicyForm,
	getS3CompatibleDriverPromotionTarget,
	hasConnectionFieldChanges,
	isS3CompatibleDriver,
	normalizePolicyForm,
	type PolicyFormData,
} from "@/components/admin/storagePolicyDialogShared";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { useApiList } from "@/hooks/useApiList";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { usePageTitle } from "@/hooks/usePageTitle";
import { usePendingAction } from "@/hooks/usePendingAction";
import { usePendingId } from "@/hooks/usePendingId";
import { invalidateAdminPolicyLookup } from "@/lib/adminPolicyLookup";
import {
	loadAdminRemoteNodeLookup,
	readAdminRemoteNodeLookup,
} from "@/lib/adminRemoteNodeLookup";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import {
	buildOffsetPaginationSearchParams,
	parseOffsetSearchParam,
	parsePageSizeOption,
	parsePageSizeSearchParam,
	parseSortOrderSearchParam,
	parseSortSearchParam,
	type SortOrder,
} from "@/lib/pagination";
import { adminPolicyService } from "@/services/adminService";
import { ApiError } from "@/services/http";
import type { AdminPolicySortBy } from "@/types/adminSort";
import type {
	DeletePolicyQuery,
	DriverType,
	RemoteNodeInfo,
	StoragePolicy,
	StoragePolicyCapacityInfo,
	StoragePolicyMigrationDryRun,
} from "@/types/api";
import { ApiErrorCode } from "@/types/api-helpers";

const POLICY_PAGE_SIZE_OPTIONS = [10, 20, 50] as const;
const DEFAULT_POLICY_PAGE_SIZE = 20 as const;
const POLICY_SORT_BY_OPTIONS = [
	"id",
	"name",
	"driver_type",
	"endpoint",
	"bucket",
	"is_default",
	"created_at",
	"updated_at",
] as const satisfies readonly AdminPolicySortBy[];
const DEFAULT_POLICY_SORT_BY =
	"created_at" as const satisfies AdminPolicySortBy;
const DEFAULT_POLICY_SORT_ORDER = "desc" as const satisfies SortOrder;
const CREATE_LAST_STEP = 2;
const POLICY_UPLOAD_SESSION_BLOCKER_CODE =
	ApiErrorCode.PolicyUploadSessionsExist;

function policyFormValueEquals(left: unknown, right: unknown): boolean {
	if (Object.is(left, right)) {
		return true;
	}
	if (Array.isArray(left) || Array.isArray(right)) {
		if (!Array.isArray(left) || !Array.isArray(right)) {
			return false;
		}
		return (
			left.length === right.length &&
			left.every((item, index) => policyFormValueEquals(item, right[index]))
		);
	}
	if (
		left === null ||
		right === null ||
		typeof left !== "object" ||
		typeof right !== "object"
	) {
		return false;
	}

	const leftRecord = left as Record<string, unknown>;
	const rightRecord = right as Record<string, unknown>;
	const leftKeys = Object.keys(leftRecord);
	if (leftKeys.length !== Object.keys(rightRecord).length) {
		return false;
	}

	return leftKeys.every(
		(key) =>
			Object.hasOwn(rightRecord, key) &&
			policyFormValueEquals(leftRecord[key], rightRecord[key]),
	);
}

function policyFormHasUnsavedChanges(
	form: PolicyFormData,
	policy: StoragePolicy | null,
) {
	if (!policy) {
		return false;
	}

	return !policyFormValueEquals(
		normalizePolicyForm(form),
		normalizePolicyForm(getPolicyForm(policy)),
	);
}

function useAdminPoliciesPageContent() {
	const { t } = useTranslation("admin");
	usePageTitle(t("policies"));
	const navigate = useNavigate();
	const [searchParams, setSearchParams] = useSearchParams();
	const [offset, setOffset] = useState(() =>
		parseOffsetSearchParam(searchParams.get("offset")),
	);
	const [pageSize, setPageSize] = useState<
		(typeof POLICY_PAGE_SIZE_OPTIONS)[number]
	>(() =>
		parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			POLICY_PAGE_SIZE_OPTIONS,
			DEFAULT_POLICY_PAGE_SIZE,
		),
	);
	const [sortBy, setSortBy] = useState<AdminPolicySortBy>(() =>
		parseSortSearchParam(
			searchParams.get("sortBy"),
			POLICY_SORT_BY_OPTIONS,
			DEFAULT_POLICY_SORT_BY,
		),
	);
	const [sortOrder, setSortOrder] = useState<SortOrder>(() =>
		parseSortOrderSearchParam(
			searchParams.get("sortOrder"),
			DEFAULT_POLICY_SORT_ORDER,
		),
	);
	const {
		items: policies,
		setItems: setPolicies,
		total,
		setTotal,
		loading,
		reload,
	} = useApiList(
		() =>
			adminPolicyService.list({
				limit: pageSize,
				offset,
				sort_by: sortBy,
				sort_order: sortOrder,
			}),
		[offset, pageSize, sortBy, sortOrder],
	);
	const [dialogOpen, setDialogOpen] = useState(false);
	const [editingId, setEditingId] = useState<number | null>(null);
	const [editingPolicy, setEditingPolicy] = useState<StoragePolicy | null>(
		null,
	);
	const [policyCapacity, setPolicyCapacity] =
		useState<StoragePolicyCapacityInfo | null>(null);
	const [policyCapacityLoading, setPolicyCapacityLoading] = useState(false);
	const policyCapacityRequestSerial = useRef(0);
	const [remoteNodes, setRemoteNodes] = useState<RemoteNodeInfo[]>(
		() => readAdminRemoteNodeLookup() ?? [],
	);
	const [form, setForm] = useState<PolicyFormData>(emptyForm);
	const [submitting, setSubmitting] = useState(false);
	const [saveAnywayConfirmOpen, setSaveAnywayConfirmOpen] = useState(false);
	const [s3DriverPromotionConfirmOpen, setS3DriverPromotionConfirmOpen] =
		useState(false);
	const [migrationDialogOpen, setMigrationDialogOpen] = useState(false);
	const [migrationPolicies, setMigrationPolicies] = useState<StoragePolicy[]>(
		[],
	);
	const [migrationSourcePolicyId, setMigrationSourcePolicyId] = useState("");
	const [migrationTargetPolicyId, setMigrationTargetPolicyId] = useState("");
	const [migrationDryRun, setMigrationDryRun] =
		useState<StoragePolicyMigrationDryRun | null>(null);
	const [migrationDryRunLoading, setMigrationDryRunLoading] = useState(false);
	const [migrationSubmitting, setMigrationSubmitting] = useState(false);
	const [validatedConnectionKey, setValidatedConnectionKey] = useState<
		string | null
	>(null);
	const [createStep, setCreateStep] = useState(0);
	const [createStepTouched, setCreateStepTouched] = useState(false);
	const {
		clearPending: clearDeletingPolicy,
		pendingId: deletingPolicyId,
		runWithPending: runWithDeletingPolicy,
	} = usePendingId<number>();
	const {
		pending: s3DriverPromotionSubmitting,
		runWithPending: runWithS3DriverPromotion,
	} = usePendingAction();
	const endpointValidationMessage = getEndpointValidationMessage(form, t);
	const getS3CompatiblePromotionDriverLabel = (driverType: "tencent_cos") =>
		driverType === "tencent_cos" ? t("driver_type_tencent_cos") : driverType;
	const savedS3DriverPromotionTarget = getS3CompatibleDriverPromotionTarget(
		editingPolicy,
		getS3CompatiblePromotionDriverLabel,
	);
	// Draft detection gives immediate feedback while editing; only the saved
	// target is allowed to submit the in-place promotion request.
	const draftS3DriverPromotionTarget = getS3CompatibleDriverPromotionTarget(
		editingId !== null
			? { driver_type: form.driver_type, endpoint: form.endpoint }
			: null,
		getS3CompatiblePromotionDriverLabel,
	);
	const s3DriverPromotionTarget =
		draftS3DriverPromotionTarget ?? savedS3DriverPromotionTarget;
	const s3CompatibleDriverSuggestionTarget =
		getS3CompatibleDriverPromotionTarget(
			{ driver_type: form.driver_type, endpoint: form.endpoint },
			getS3CompatiblePromotionDriverLabel,
		);
	const s3DriverPromotionBlocked =
		s3DriverPromotionTarget != null &&
		policyFormHasUnsavedChanges(form, editingPolicy);
	const totalPages = Math.max(1, Math.ceil(total / pageSize));
	const currentPage = Math.floor(offset / pageSize) + 1;
	const prevPageDisabled = offset === 0;
	const nextPageDisabled = offset + pageSize >= total;
	const pageSizeOptions = POLICY_PAGE_SIZE_OPTIONS.map((size) => ({
		label: t("page_size_option", { count: size }),
		value: String(size),
	}));
	const remoteNodeNameById = new Map(
		remoteNodes.map((node) => [node.id, node.name] as const),
	);

	useEffect(() => {
		setSearchParams(
			buildOffsetPaginationSearchParams({
				offset,
				pageSize,
				defaultPageSize: DEFAULT_POLICY_PAGE_SIZE,
				extraParams: {
					sortBy: sortBy !== DEFAULT_POLICY_SORT_BY ? sortBy : undefined,
					sortOrder:
						sortOrder !== DEFAULT_POLICY_SORT_ORDER ? sortOrder : undefined,
				},
			}),
			{ replace: true },
		);
	}, [offset, pageSize, setSearchParams, sortBy, sortOrder]);

	useEffect(() => {
		let active = true;

		void loadAdminRemoteNodeLookup()
			.then((nodes) => {
				if (active) {
					setRemoteNodes(nodes);
				}
			})
			.catch((error) => {
				if (active) {
					handleApiError(error);
				}
			});

		return () => {
			active = false;
		};
	}, []);

	const refreshRemoteNodeLookup = async (options?: { force?: boolean }) => {
		try {
			setRemoteNodes(await loadAdminRemoteNodeLookup(options));
		} catch (error) {
			handleApiError(error);
		}
	};

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, POLICY_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setPageSize(next);
		setOffset(0);
	};

	const handleSortChange = (
		nextSortBy: AdminPolicySortBy,
		nextOrder: SortOrder,
	) => {
		setSortBy(nextSortBy);
		setSortOrder(nextOrder);
		setOffset(0);
	};

	const finalizePolicyDelete = async () => {
		invalidateAdminPolicyLookup();
		if (policies.length === 1 && offset > 0) {
			setOffset(Math.max(0, offset - pageSize));
		} else {
			await reload();
		}
	};

	const handleDelete = async (id: number, options?: DeletePolicyQuery) => {
		if (id === PROTECTED_POLICY_ID) return;
		await runWithDeletingPolicy(id, async () => {
			try {
				if (options) {
					await adminPolicyService.delete(id, options);
				} else {
					await adminPolicyService.delete(id);
				}
				await finalizePolicyDelete();
				toast.success(
					options?.force ? t("policy_force_deleted") : t("policy_deleted"),
				);
			} catch (error) {
				if (
					!options?.force &&
					error instanceof ApiError &&
					error.code === POLICY_UPLOAD_SESSION_BLOCKER_CODE
				) {
					clearDeletingPolicy();
					requestForceDeleteConfirm(id);
					return;
				}
				handleApiError(error);
			}
		});
	};

	const {
		confirmId: deleteId,
		requestConfirm,
		dialogProps,
	} = useConfirmDialog(handleDelete);
	const {
		confirmId: forceDeleteId,
		requestConfirm: requestForceDeleteConfirm,
		dialogProps: forceDeleteDialogProps,
	} = useConfirmDialog<number>(async (id) => {
		await handleDelete(id, { force: true });
	});
	const requestDeleteConfirm = (id: number) => {
		if (id === PROTECTED_POLICY_ID) return;
		requestConfirm(id);
	};

	const resetDialogState = () => {
		policyCapacityRequestSerial.current += 1;
		setSaveAnywayConfirmOpen(false);
		setS3DriverPromotionConfirmOpen(false);
		setPolicyCapacity(null);
		setPolicyCapacityLoading(false);
		setValidatedConnectionKey(null);
		setCreateStep(0);
		setCreateStepTouched(false);
	};

	const openCreate = () => {
		setEditingId(null);
		setEditingPolicy(null);
		resetDialogState();
		setForm(emptyForm);
		void refreshRemoteNodeLookup();
		setDialogOpen(true);
	};

	const openMigrationDialog = async () => {
		try {
			const allPolicies = await adminPolicyService.listAll();
			const firstPolicy = allPolicies[0];
			const secondPolicy = allPolicies.find(
				(policy) => policy.id !== firstPolicy?.id,
			);
			setMigrationPolicies(allPolicies);
			setMigrationSourcePolicyId(firstPolicy ? String(firstPolicy.id) : "");
			setMigrationTargetPolicyId(secondPolicy ? String(secondPolicy.id) : "");
			setMigrationDryRun(null);
			setMigrationDialogOpen(true);
		} catch (error) {
			handleApiError(error);
		}
	};

	const handleMigrationSourceChange = (policyId: string) => {
		setMigrationSourcePolicyId(policyId);
		setMigrationDryRun(null);
		if (policyId === migrationTargetPolicyId) {
			setMigrationTargetPolicyId("");
		}
	};

	const handleMigrationTargetChange = (policyId: string) => {
		if (policyId === migrationSourcePolicyId) {
			setMigrationTargetPolicyId("");
			setMigrationDryRun(null);
			toast.error(t("policy_migration_same_policy_error"));
			return;
		}
		setMigrationTargetPolicyId(policyId);
		setMigrationDryRun(null);
	};

	const openEdit = (policy: StoragePolicy) => {
		setEditingId(policy.id);
		setEditingPolicy(policy);
		resetDialogState();
		const capacityRequestSerial = ++policyCapacityRequestSerial.current;
		setPolicyCapacityLoading(true);
		setForm(getPolicyForm(policy));
		void refreshRemoteNodeLookup();
		void adminPolicyService
			.getCapacity(policy.id)
			.then((capacity) => {
				if (capacityRequestSerial === policyCapacityRequestSerial.current) {
					setPolicyCapacity(capacity);
				}
			})
			.catch((error) => {
				if (capacityRequestSerial === policyCapacityRequestSerial.current) {
					handleApiError(error);
					setPolicyCapacity(null);
				}
			})
			.finally(() => {
				if (capacityRequestSerial === policyCapacityRequestSerial.current) {
					setPolicyCapacityLoading(false);
				}
			});
		setDialogOpen(true);
	};

	const handleDialogOpenChange = (open: boolean) => {
		setDialogOpen(open);
		if (!open) {
			resetDialogState();
		}
	};

	const setField = <K extends keyof PolicyFormData>(
		key: K,
		value: PolicyFormData[K],
	) => {
		setSaveAnywayConfirmOpen(false);
		setS3DriverPromotionConfirmOpen(false);
		setForm((prev) => {
			if (key === "storage_native_processing_enabled") {
				const enabled = value as boolean;
				return {
					...prev,
					storage_native_processing_enabled: enabled,
					thumbnail_processor: enabled ? "storage_native" : null,
					// Enabling storage-native processing seeds thumbnail_extensions with
					// DEFAULT_STORAGE_NATIVE_THUMBNAIL_EXTENSIONS, but leaves
					// media_metadata_extensions empty to avoid accidental billable metadata calls.
					thumbnail_extensions: enabled
						? prev.thumbnail_extensions.length > 0
							? prev.thumbnail_extensions
							: [...DEFAULT_STORAGE_NATIVE_THUMBNAIL_EXTENSIONS]
						: [],
					storage_native_media_metadata_enabled: enabled
						? prev.storage_native_media_metadata_enabled
						: false,
					media_metadata_extensions: enabled
						? (prev.media_metadata_extensions ?? [])
						: [],
				};
			}

			return { ...prev, [key]: value };
		});
	};

	const setDriverType = (driverType: DriverType) => {
		setSaveAnywayConfirmOpen(false);
		setS3DriverPromotionConfirmOpen(false);
		setValidatedConnectionKey(null);
		setCreateStepTouched(false);
		setForm((prev) => {
			const { s3_path_style: previousS3PathStyle, ...prevWithoutS3PathStyle } =
				prev;
			if (isS3CompatibleDriver(driverType)) {
				return {
					...prevWithoutS3PathStyle,
					driver_type: driverType,
					remote_node_id: "",
					storage_native_processing_enabled:
						driverType === "tencent_cos"
							? prev.storage_native_processing_enabled
							: false,
					thumbnail_processor:
						driverType === "tencent_cos" ? prev.thumbnail_processor : null,
					thumbnail_extensions:
						driverType === "tencent_cos" ? prev.thumbnail_extensions : [],
					storage_native_media_metadata_enabled:
						driverType === "tencent_cos"
							? prev.storage_native_media_metadata_enabled
							: false,
					media_metadata_extensions:
						driverType === "tencent_cos"
							? (prev.media_metadata_extensions ?? [])
							: [],
					...(driverType === "s3"
						? { s3_path_style: previousS3PathStyle ?? true }
						: {}),
				};
			}

			if (driverType === "remote") {
				return {
					...prevWithoutS3PathStyle,
					driver_type: driverType,
					endpoint: "",
					bucket: "",
					access_key: "",
					secret_key: "",
					content_dedup: false,
					storage_native_processing_enabled: false,
					thumbnail_processor: null,
					thumbnail_extensions: [],
					storage_native_media_metadata_enabled: false,
					media_metadata_extensions: [],
					remote_download_strategy: "relay_stream",
					remote_upload_strategy: "relay_stream",
				};
			}

			return {
				...prevWithoutS3PathStyle,
				driver_type: driverType,
				endpoint: "",
				bucket: "",
				access_key: "",
				secret_key: "",
				remote_node_id: "",
				storage_native_processing_enabled: false,
				thumbnail_processor: null,
				thumbnail_extensions: [],
				storage_native_media_metadata_enabled: false,
				media_metadata_extensions: [],
				remote_download_strategy: "relay_stream",
				remote_upload_strategy: "relay_stream",
				s3_upload_strategy: "relay_stream",
				s3_download_strategy: "relay_stream",
			};
		});
	};

	const syncNormalizedS3Form = () => {
		const normalizedForm = normalizePolicyForm(form);
		if (normalizedForm !== form) {
			setForm(normalizedForm);
		}
		return normalizedForm;
	};

	const runConnectionTest = async ({
		showSuccessToast = true,
		showFailureError = true,
	}: {
		showSuccessToast?: boolean;
		showFailureError?: boolean;
	} = {}) => {
		const currentForm = syncNormalizedS3Form();
		const currentEndpointValidationMessage = getEndpointValidationMessage(
			currentForm,
			t,
		);
		if (currentEndpointValidationMessage) {
			if (showFailureError) {
				toast.error(currentEndpointValidationMessage);
			}
			setValidatedConnectionKey(null);
			return false;
		}

		const shouldUseParamTest =
			editingId === null ||
			hasConnectionFieldChanges(currentForm, editingPolicy);

		try {
			if (shouldUseParamTest) {
				await adminPolicyService.testParams(
					buildPolicyTestPayload(currentForm),
				);
			} else {
				await adminPolicyService.testConnection(editingId);
			}

			if (
				isS3CompatibleDriver(currentForm.driver_type) ||
				currentForm.driver_type === "remote"
			) {
				setValidatedConnectionKey(getPolicyConnectionTestKey(currentForm));
			}
			if (showSuccessToast) {
				toast.success(t("connection_success"));
			}
			return true;
		} catch (e) {
			setValidatedConnectionKey(null);
			if (showFailureError) {
				handleApiError(e);
			}
			return false;
		}
	};

	const persistPolicy = async () => {
		try {
			const currentForm = syncNormalizedS3Form();
			if (editingId) {
				const updated = await adminPolicyService.update(
					editingId,
					buildUpdatePolicyPayload(currentForm),
				);
				invalidateAdminPolicyLookup();
				setEditingId(updated.id);
				setEditingPolicy(updated);
				setForm(getPolicyForm(updated));
				setValidatedConnectionKey(null);
				setPolicies((prev) =>
					prev.map((policy) => (policy.id === editingId ? updated : policy)),
				);
				toast.success(t("policy_updated"));
			} else {
				await adminPolicyService.create(buildCreatePolicyPayload(currentForm));
				invalidateAdminPolicyLookup();
				const nextTotal = total + 1;
				const nextLastOffset = Math.max(
					0,
					Math.floor((nextTotal - 1) / pageSize) * pageSize,
				);
				if (nextLastOffset !== offset) {
					setOffset(nextLastOffset);
				} else {
					await reload();
				}
				toast.success(t("policy_created"));
				handleDialogOpenChange(false);
			}
		} catch (e) {
			handleApiError(e);
		}
	};

	const shouldRunConnectionSaveTest = () => {
		if (
			!isS3CompatibleDriver(form.driver_type) &&
			form.driver_type !== "remote"
		) {
			return false;
		}

		if (editingId !== null && !hasConnectionFieldChanges(form, editingPolicy)) {
			return false;
		}

		return validatedConnectionKey !== getPolicyConnectionTestKey(form);
	};

	const submitPolicy = async (forceSave = false) => {
		if (submitting) {
			return;
		}

		setSubmitting(true);
		try {
			if (!forceSave && shouldRunConnectionSaveTest()) {
				const testPassed = await runConnectionTest({
					showSuccessToast: false,
					showFailureError: false,
				});
				if (!testPassed) {
					setSaveAnywayConfirmOpen(true);
					return;
				}
			}

			setSaveAnywayConfirmOpen(false);
			await persistPolicy();
		} finally {
			setSubmitting(false);
		}
	};

	const cancelSaveAnyway = () => {
		setSaveAnywayConfirmOpen(false);
	};

	const confirmSaveAnyway = () => {
		setSaveAnywayConfirmOpen(false);
		void submitPolicy(true);
	};

	const requestS3DriverPromotion = () => {
		if (!savedS3DriverPromotionTarget || s3DriverPromotionBlocked) {
			return;
		}
		setSaveAnywayConfirmOpen(false);
		setS3DriverPromotionConfirmOpen(true);
	};

	const cancelS3DriverPromotion = () => {
		setS3DriverPromotionConfirmOpen(false);
	};

	const confirmS3DriverPromotion = () => {
		if (
			!editingPolicy ||
			!savedS3DriverPromotionTarget ||
			s3DriverPromotionBlocked
		) {
			return;
		}

		void runWithS3DriverPromotion(async () => {
			try {
				const updated = await adminPolicyService.promoteS3CompatibleDriver(
					editingPolicy.id,
					{
						target_driver_type: savedS3DriverPromotionTarget.driverType,
						endpoint: editingPolicy.endpoint,
						bucket: editingPolicy.bucket,
					},
				);
				setS3DriverPromotionConfirmOpen(false);
				setEditingId(updated.id);
				setEditingPolicy(updated);
				setForm(getPolicyForm(updated));
				setPolicies((prev) =>
					prev.map((policy) => (policy.id === updated.id ? updated : policy)),
				);
				setPolicyCapacity((prev) =>
					prev == null ? prev : { ...prev, driver_type: updated.driver_type },
				);
				invalidateAdminPolicyLookup();
				toast.success(
					t("policy_s3_driver_promotion_success", {
						driver: savedS3DriverPromotionTarget.driverLabel,
					}),
				);
			} catch (error) {
				handleApiError(error);
			}
		});
	};

	const applyS3CompatibleDriverSuggestion = () => {
		if (!s3CompatibleDriverSuggestionTarget) {
			return;
		}
		setDriverType(s3CompatibleDriverSuggestionTarget.driverType);
	};

	const handleCreateBack = () => {
		setCreateStepTouched(false);
		setCreateStep((prev) => Math.max(0, prev - 1));
	};

	const handleCreateStepChange = (step: number) => {
		setCreateStepTouched(false);
		setCreateStep(Math.max(0, Math.min(CREATE_LAST_STEP, step)));
	};

	const handleCreateNext = () => {
		if (createStep >= CREATE_LAST_STEP) {
			return;
		}

		if (createStep === 0) {
			setCreateStep(1);
			return;
		}

		setCreateStepTouched(true);

		if (!form.name.trim()) {
			return;
		}

		if (isS3CompatibleDriver(form.driver_type) && !form.bucket.trim()) {
			return;
		}

		if (isS3CompatibleDriver(form.driver_type) && !form.endpoint.trim()) {
			return;
		}

		if (form.driver_type === "remote" && !form.remote_node_id) {
			return;
		}

		if (endpointValidationMessage) {
			return;
		}

		syncNormalizedS3Form();
		setCreateStepTouched(false);
		setCreateStep(CREATE_LAST_STEP);
	};

	const handleSubmit = () => {
		if (editingId === null && createStep < CREATE_LAST_STEP) {
			handleCreateNext();
			return;
		}
		void submitPolicy();
	};

	const deletePolicyName =
		deleteId !== null
			? (policies.find((policy) => policy.id === deleteId)?.name ?? "")
			: "";
	const forceDeletePolicyName =
		forceDeleteId !== null
			? (policies.find((policy) => policy.id === forceDeleteId)?.name ?? "")
			: "";
	const handleRefresh = async () => {
		try {
			const [policyPage, remoteNodeLookup] = await Promise.all([
				adminPolicyService.list({
					limit: pageSize,
					offset,
					sort_by: sortBy,
					sort_order: sortOrder,
				}),
				loadAdminRemoteNodeLookup({ force: true }),
			]);
			setPolicies(policyPage.items);
			setTotal(policyPage.total);
			setRemoteNodes(remoteNodeLookup);
			invalidateAdminPolicyLookup();
		} catch (error) {
			handleApiError(error);
		}
	};

	const handleCreateMigration = async () => {
		if (migrationSubmitting) return;
		const sourcePolicyId = Number(migrationSourcePolicyId);
		const targetPolicyId = Number(migrationTargetPolicyId);
		if (
			!Number.isSafeInteger(sourcePolicyId) ||
			!Number.isSafeInteger(targetPolicyId) ||
			sourcePolicyId <= 0 ||
			targetPolicyId <= 0
		) {
			return;
		}
		if (sourcePolicyId === targetPolicyId) {
			toast.error(t("policy_migration_same_policy_error"));
			return;
		}
		if (
			migrationDryRun?.source_policy_id !== sourcePolicyId ||
			migrationDryRun?.target_policy_id !== targetPolicyId ||
			!migrationDryRun.can_start ||
			migrationDryRunLoading
		) {
			return;
		}

		setMigrationSubmitting(true);
		try {
			const task = await adminPolicyService.createMigration({
				source_policy_id: sourcePolicyId,
				target_policy_id: targetPolicyId,
				delete_source_after_success: false,
			});
			setMigrationDialogOpen(false);
			toast.success(t("policy_migration_created", { id: task.id }));
			navigate("/admin/tasks?kind=storage_policy_migration", {
				viewTransition: false,
			});
		} catch (error) {
			handleApiError(error);
		} finally {
			setMigrationSubmitting(false);
		}
	};

	const handleDryRunMigration = async () => {
		if (migrationDryRunLoading || migrationSubmitting) return;
		const sourcePolicyId = Number(migrationSourcePolicyId);
		const targetPolicyId = Number(migrationTargetPolicyId);
		if (
			!Number.isSafeInteger(sourcePolicyId) ||
			!Number.isSafeInteger(targetPolicyId) ||
			sourcePolicyId <= 0 ||
			targetPolicyId <= 0
		) {
			return;
		}
		if (sourcePolicyId === targetPolicyId) {
			setMigrationDryRun(null);
			toast.error(t("policy_migration_same_policy_error"));
			return;
		}

		setMigrationDryRunLoading(true);
		try {
			const result = await adminPolicyService.dryRunMigration({
				source_policy_id: sourcePolicyId,
				target_policy_id: targetPolicyId,
				delete_source_after_success: false,
			});
			setMigrationDryRun(result);
		} catch (error) {
			setMigrationDryRun(null);
			handleApiError(error);
		} finally {
			setMigrationDryRunLoading(false);
		}
	};

	return (
		<AdminLayout>
			<AdminPageShell>
				<AdminPageHeader
					title={t("policies")}
					description={t("policies_intro")}
					actions={
						<>
							<Button
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={openCreate}
							>
								<Icon name="Plus" className="mr-1 size-4" />
								{t("new_policy")}
							</Button>
							<Button
								variant="outline"
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() => void openMigrationDialog()}
								disabled={total < 2}
							>
								<Icon name="ArrowsClockwise" className="mr-1 size-3.5" />
								{t("policy_migration_action")}
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

				<PoliciesTable
					loading={loading}
					deletingPolicyId={deletingPolicyId}
					onDeletePolicy={requestDeleteConfirm}
					onEditPolicy={openEdit}
					policies={policies}
					remoteNodeNameById={remoteNodeNameById}
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

				<PolicyDialogs
					deleteDialogProps={dialogProps}
					deletePolicyName={deletePolicyName}
					forceDeleteDialogProps={forceDeleteDialogProps}
					forceDeletePolicyName={forceDeletePolicyName}
					dialogOpen={dialogOpen}
					editMode={editingId !== null}
					form={form}
					policyCapacity={policyCapacity}
					policyCapacityLoading={policyCapacityLoading}
					s3CompatibleDriverSuggestionTargetLabel={
						s3CompatibleDriverSuggestionTarget?.driverLabel ?? null
					}
					s3DriverPromotionBlocked={s3DriverPromotionBlocked}
					s3DriverPromotionConfirmOpen={s3DriverPromotionConfirmOpen}
					s3DriverPromotionSubmitting={s3DriverPromotionSubmitting}
					s3DriverPromotionTargetLabel={
						s3DriverPromotionTarget?.driverLabel ?? null
					}
					remoteNodes={remoteNodes}
					submitting={submitting}
					createStep={createStep}
					createStepTouched={createStepTouched}
					endpointValidationMessage={endpointValidationMessage}
					saveAnywayConfirmOpen={saveAnywayConfirmOpen}
					onApplyS3CompatibleDriverSuggestion={
						applyS3CompatibleDriverSuggestion
					}
					onCancelSaveAnyway={cancelSaveAnyway}
					onCancelS3DriverPromotion={cancelS3DriverPromotion}
					onConfirmSaveAnyway={confirmSaveAnyway}
					onConfirmS3DriverPromotion={confirmS3DriverPromotion}
					onDialogOpenChange={handleDialogOpenChange}
					onSubmit={handleSubmit}
					onRequestS3DriverPromotion={requestS3DriverPromotion}
					onRunConnectionTest={() => runConnectionTest()}
					onFieldChange={setField}
					onDriverTypeChange={setDriverType}
					onCreateBack={handleCreateBack}
					onCreateStepChange={handleCreateStepChange}
					onCreateNext={handleCreateNext}
					onSyncNormalizedS3Form={syncNormalizedS3Form}
				/>
				<StoragePolicyMigrationDialog
					dryRun={migrationDryRun}
					dryRunLoading={migrationDryRunLoading}
					open={migrationDialogOpen}
					policies={migrationPolicies}
					sourcePolicyId={migrationSourcePolicyId}
					targetPolicyId={migrationTargetPolicyId}
					submitting={migrationSubmitting}
					onDryRun={() => void handleDryRunMigration()}
					onOpenChange={setMigrationDialogOpen}
					onSourcePolicyChange={handleMigrationSourceChange}
					onTargetPolicyChange={handleMigrationTargetChange}
					onSubmit={() => void handleCreateMigration()}
				/>
			</AdminPageShell>
		</AdminLayout>
	);
}

export default function AdminPoliciesPage() {
	return useAdminPoliciesPageContent();
}
