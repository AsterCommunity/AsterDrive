import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import { PoliciesTable } from "@/components/admin/admin-policies-page/PoliciesTable";
import { PolicyDialogs } from "@/components/admin/admin-policies-page/PolicyDialogs";
import { PROTECTED_POLICY_ID } from "@/components/admin/admin-policies-page/policyPresentation";
import { StoragePolicyMigrationDialog } from "@/components/admin/admin-policies-page/StoragePolicyMigrationDialog";
import { microsoftGraphCredentials } from "@/components/admin/storage-policy-dialog/applicationCredentials";
import {
	getEndpointValidationMessage,
	getPolicyConnectionTestKey,
	getS3CompatibleDriverPromotionTarget,
	hasConnectionFieldChanges,
	normalizePolicyForm,
} from "@/components/admin/storage-policy-dialog/connectionNormalization";
import {
	supportsApplicationCredentials,
	supportsDraftConnectionTest,
	supportsMicrosoftGraphApplicationConfig,
	supportsObjectStorageConnection,
	supportsOneDrivePolicyOptions,
	supportsRemoteNodeBinding,
	supportsS3TransferStrategy,
	supportsSavedConnectionTest,
	supportsStorageCredentialLifecycle,
	supportsStorageNativeProcessing,
	supportsStoragePolicyAction,
} from "@/components/admin/storage-policy-dialog/descriptorPredicates";
import {
	DEFAULT_STORAGE_NATIVE_THUMBNAIL_EXTENSIONS,
	emptyForm,
	getPolicyForm,
	type PolicyFormData,
} from "@/components/admin/storage-policy-dialog/formTypes";
import { MICROSOFT_GRAPH_PROVIDER } from "@/components/admin/storage-policy-dialog/onedriveFieldUtils";
import {
	buildCreatePolicyPayload,
	buildPolicyTestPayload,
	buildTencentCosCorsPayload,
	buildUpdatePolicyPayload,
	parseMicrosoftGraphScopes,
} from "@/components/admin/storage-policy-dialog/payloadBuilders";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { config } from "@/config/app";
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
import {
	getStorageDriverDescriptor,
	loadAdminStorageDriverDescriptors,
	readAdminStorageDriverDescriptors,
} from "@/lib/adminStorageDriverDescriptors";
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
	StorageConnectorDescriptor,
	StoragePolicy,
	StoragePolicyCapacityInfo,
	StoragePolicyCredentialInfo,
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

function getStorageAuthorizationCallbackUrl() {
	const apiBaseUrl = new URL(config.apiBaseUrl, window.location.origin);
	return new URL(
		"admin/policies/storage-authorization/callback",
		apiBaseUrl.href.endsWith("/") ? apiBaseUrl.href : `${apiBaseUrl.href}/`,
	).toString();
}

function consumeStorageAuthorizationSearchParams(
	searchParams: URLSearchParams,
) {
	const status = searchParams.get("storage_authorization");
	if (!status) {
		return null;
	}

	const nextSearchParams = new URLSearchParams(searchParams);
	const policyId = nextSearchParams.get("policy_id");
	const reason = nextSearchParams.get("reason");
	nextSearchParams.delete("storage_authorization");
	nextSearchParams.delete("policy_id");
	nextSearchParams.delete("reason");
	return {
		policyId,
		reason,
		status,
		nextSearchParams,
	};
}

function storageAuthorizationFailureI18nKey(reason: string | null) {
	switch (reason) {
		case "invalid_state":
			return "onedrive_authorization_failed_invalid_state";
		case "provider_error":
			return "onedrive_authorization_failed_provider";
		case "token_exchange_failed":
			return "onedrive_authorization_failed_token_exchange";
		case "drive_resolution_failed":
			return "onedrive_authorization_failed_drive_resolution";
		case "unsupported_provider":
			return "onedrive_authorization_failed_unsupported_provider";
		case "invalid_request":
			return "onedrive_authorization_failed_invalid_request";
		case "server_error":
			return "onedrive_authorization_failed_server";
		default:
			return "onedrive_authorization_failed";
	}
}

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
	descriptor?: StorageConnectorDescriptor | null,
) {
	if (!policy) {
		return false;
	}

	const comparableForm = normalizePolicyComparableForm(form, descriptor);
	const comparablePolicyForm = normalizePolicyComparableForm(
		getPolicyForm(policy),
		descriptor,
	);

	return !policyFormValueEquals(comparableForm, comparablePolicyForm);
}

function normalizePolicyComparableForm(
	form: PolicyFormData,
	descriptor?: StorageConnectorDescriptor | null,
) {
	const normalized = normalizePolicyForm(form, descriptor);
	const usesMicrosoftGraph =
		descriptor != null
			? supportsOneDrivePolicyOptions(descriptor) ||
				supportsMicrosoftGraphApplicationConfig(descriptor)
			: hasMicrosoftGraphFormFields(normalized);

	if (!usesMicrosoftGraph) {
		const {
			onedrive_account_mode: _accountMode,
			onedrive_cloud: _cloud,
			onedrive_drive_id: _driveId,
			onedrive_group_id: _groupId,
			onedrive_root_item_id: _rootItemId,
			onedrive_site_id: _siteId,
			onedrive_tenant: _tenant,
			application_credentials: _applicationCredentials,
			...comparable
		} = normalized;
		return comparable;
	}

	const microsoftGraph = microsoftGraphCredentials(normalized);
	if (microsoftGraph.client_id.trim() || microsoftGraph.client_secret.trim()) {
		return normalized;
	}

	const { application_credentials: _applicationCredentials, ...comparable } =
		normalized;
	return comparable;
}

function hasMicrosoftGraphFormFields(form: PolicyFormData) {
	const microsoftGraph = microsoftGraphCredentials(form);
	return (
		Boolean(form.onedrive_account_mode) ||
		Boolean(form.onedrive_cloud) ||
		Boolean(form.onedrive_tenant.trim()) ||
		Boolean(form.onedrive_drive_id.trim()) ||
		Boolean(form.onedrive_root_item_id.trim()) ||
		Boolean(microsoftGraph.client_id.trim()) ||
		Boolean(microsoftGraph.client_secret.trim()) ||
		Boolean(microsoftGraph.scopes.trim())
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
	const currentEditingIdRef = useRef<number | null>(null);
	const [editingPolicy, setEditingPolicy] = useState<StoragePolicy | null>(
		null,
	);
	const [policyCapacity, setPolicyCapacity] =
		useState<StoragePolicyCapacityInfo | null>(null);
	const [policyCapacityLoading, setPolicyCapacityLoading] = useState(false);
	const policyCapacityRequestSerial = useRef(0);
	const [storageCredentials, setStorageCredentials] = useState<
		StoragePolicyCredentialInfo[]
	>([]);
	const [storageCredentialsLoading, setStorageCredentialsLoading] =
		useState(false);
	const storageCredentialsRequestSerial = useRef(0);
	const storageCredentialValidationRequestSerial = useRef(0);
	const consumedStorageAuthorizationSearchRef = useRef<string | null>(null);
	const [remoteNodes, setRemoteNodes] = useState<RemoteNodeInfo[]>(
		() => readAdminRemoteNodeLookup() ?? [],
	);
	const [storageDriverDescriptors, setStorageDriverDescriptors] = useState<
		StorageConnectorDescriptor[]
	>(() => readAdminStorageDriverDescriptors() ?? []);
	const [storageDriverDescriptorsLoading, setStorageDriverDescriptorsLoading] =
		useState(() => readAdminStorageDriverDescriptors() == null);
	const [storageDriverDescriptorsError, setStorageDriverDescriptorsError] =
		useState<string | null>(null);
	const [form, setForm] = useState<PolicyFormData>(emptyForm);
	const [submitting, setSubmitting] = useState(false);

	currentEditingIdRef.current = editingId;
	const [saveAnywayConfirmOpen, setSaveAnywayConfirmOpen] = useState(false);
	const [s3DriverPromotionConfirmOpen, setS3DriverPromotionConfirmOpen] =
		useState(false);
	const [cosCorsConfirmOpen, setCosCorsConfirmOpen] = useState(false);
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
	const {
		pending: cosCorsSubmitting,
		runWithPending: runWithCosCorsConfigure,
	} = usePendingAction();
	const {
		pending: storageAuthorizationSubmitting,
		runWithPending: runWithStorageAuthorization,
	} = usePendingAction();
	const {
		pending: storageCredentialValidationSubmitting,
		runWithPending: runWithStorageCredentialValidation,
	} = usePendingAction();
	const currentStorageDriverDescriptor = getStorageDriverDescriptor(
		storageDriverDescriptors,
		form.driver_type,
	);
	const endpointValidationMessage = getEndpointValidationMessage(
		form,
		t,
		currentStorageDriverDescriptor,
	);
	const canConfigureTencentCosCors = supportsStoragePolicyAction(
		currentStorageDriverDescriptor,
		"configure_tencent_cos_cors",
	);
	const currentAuthorizationProvider =
		currentStorageDriverDescriptor?.authorization_provider ?? null;
	const isMicrosoftGraphAuthorizationProvider =
		currentAuthorizationProvider === MICROSOFT_GRAPH_PROVIDER;
	const getS3CompatiblePromotionDriverLabel = (driverType: DriverType) => {
		const descriptor = getStorageDriverDescriptor(
			storageDriverDescriptors,
			driverType,
		);
		return descriptor?.ui ? t(descriptor.ui.label_key) : driverType;
	};
	const savedS3DriverPromotionTarget = getS3CompatibleDriverPromotionTarget(
		editingPolicy,
		getStorageDriverDescriptor(
			storageDriverDescriptors,
			editingPolicy?.driver_type ?? form.driver_type,
		),
		getS3CompatiblePromotionDriverLabel,
	);
	// Draft detection gives immediate feedback while editing; only the saved
	// target is allowed to submit the in-place promotion request.
	const draftS3DriverPromotionTarget = getS3CompatibleDriverPromotionTarget(
		editingId !== null
			? { driver_type: form.driver_type, endpoint: form.endpoint }
			: null,
		currentStorageDriverDescriptor,
		getS3CompatiblePromotionDriverLabel,
	);
	const s3DriverPromotionTarget =
		draftS3DriverPromotionTarget ?? savedS3DriverPromotionTarget;
	const s3CompatibleDriverSuggestionTarget =
		getS3CompatibleDriverPromotionTarget(
			{ driver_type: form.driver_type, endpoint: form.endpoint },
			currentStorageDriverDescriptor,
			getS3CompatiblePromotionDriverLabel,
		);
	const s3DriverPromotionBlocked =
		s3DriverPromotionTarget != null &&
		policyFormHasUnsavedChanges(
			form,
			editingPolicy,
			currentStorageDriverDescriptor,
		);
	const cosCorsUsesDraftValues =
		editingId === null ||
		hasConnectionFieldChanges(
			form,
			editingPolicy,
			currentStorageDriverDescriptor,
		);
	const totalPages = Math.max(1, Math.ceil(total / pageSize));
	const storageAuthorizationRedirectUri = getStorageAuthorizationCallbackUrl();
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

	useEffect(() => {
		let active = true;

		setStorageDriverDescriptorsLoading(true);
		setStorageDriverDescriptorsError(null);
		void loadAdminStorageDriverDescriptors()
			.then((descriptors) => {
				if (active) {
					setStorageDriverDescriptors(descriptors);
					setStorageDriverDescriptorsError(null);
				}
			})
			.catch((error) => {
				if (active) {
					setStorageDriverDescriptorsError(
						t("policy_driver_options_load_failed"),
					);
					handleApiError(error);
				}
			})
			.finally(() => {
				if (active) {
					setStorageDriverDescriptorsLoading(false);
				}
			});

		return () => {
			active = false;
		};
	}, [t]);

	const refreshRemoteNodeLookup = useCallback(
		async (options?: { force?: boolean }) => {
			try {
				setRemoteNodes(await loadAdminRemoteNodeLookup(options));
			} catch (error) {
				handleApiError(error);
			}
		},
		[],
	);

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

	const resetDialogState = useCallback(() => {
		policyCapacityRequestSerial.current += 1;
		storageCredentialsRequestSerial.current += 1;
		storageCredentialValidationRequestSerial.current += 1;
		setSaveAnywayConfirmOpen(false);
		setS3DriverPromotionConfirmOpen(false);
		setCosCorsConfirmOpen(false);
		setPolicyCapacity(null);
		setPolicyCapacityLoading(false);
		setStorageCredentials([]);
		setStorageCredentialsLoading(false);
		setValidatedConnectionKey(null);
		setCreateStep(0);
		setCreateStepTouched(false);
	}, []);

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

	const loadPolicyCapacity = useCallback((policyId: number) => {
		const capacityRequestSerial = ++policyCapacityRequestSerial.current;
		setPolicyCapacityLoading(true);
		void adminPolicyService
			.getCapacity(policyId)
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
	}, []);

	const loadStorageCredentials = useCallback(
		(policyId: number, driverType: DriverType) => {
			const descriptor = getStorageDriverDescriptor(
				storageDriverDescriptors,
				driverType,
			);
			if (!supportsStorageCredentialLifecycle(descriptor)) {
				setStorageCredentials([]);
				setStorageCredentialsLoading(false);
				return;
			}

			const credentialsRequestSerial =
				++storageCredentialsRequestSerial.current;
			setStorageCredentialsLoading(true);
			void adminPolicyService
				.listStorageCredentials(policyId)
				.then((credentials) => {
					if (
						credentialsRequestSerial === storageCredentialsRequestSerial.current
					) {
						setStorageCredentials(credentials);
					}
				})
				.catch((error) => {
					if (
						credentialsRequestSerial === storageCredentialsRequestSerial.current
					) {
						handleApiError(error);
						setStorageCredentials([]);
					}
				})
				.finally(() => {
					if (
						credentialsRequestSerial === storageCredentialsRequestSerial.current
					) {
						setStorageCredentialsLoading(false);
					}
				});
		},
		[storageDriverDescriptors],
	);

	useEffect(() => {
		if (!editingPolicy) {
			return;
		}
		loadStorageCredentials(editingPolicy.id, editingPolicy.driver_type);
	}, [editingPolicy, loadStorageCredentials]);

	const openEdit = useCallback(
		(policy: StoragePolicy) => {
			setEditingId(policy.id);
			setEditingPolicy(policy);
			resetDialogState();
			setForm(getPolicyForm(policy));
			void refreshRemoteNodeLookup();
			loadPolicyCapacity(policy.id);
			setDialogOpen(true);
		},
		[loadPolicyCapacity, refreshRemoteNodeLookup, resetDialogState],
	);

	const openPolicyById = useCallback(
		async (policyId: number) => {
			const policy = await adminPolicyService.get(policyId);
			openEdit(policy);
			setPolicies((prev) => {
				const exists = prev.some((item) => item.id === policy.id);
				return exists
					? prev.map((item) => (item.id === policy.id ? policy : item))
					: prev;
			});
		},
		[openEdit, setPolicies],
	);

	useEffect(() => {
		const callback = consumeStorageAuthorizationSearchParams(searchParams);
		if (!callback) {
			consumedStorageAuthorizationSearchRef.current = null;
			return;
		}

		const callbackKey = searchParams.toString();
		if (consumedStorageAuthorizationSearchRef.current === callbackKey) {
			return;
		}
		consumedStorageAuthorizationSearchRef.current = callbackKey;

		setSearchParams(callback.nextSearchParams, { replace: true });
		if (callback.status === "success") {
			toast.success(t("onedrive_authorization_completed"), {
				description: callback.policyId
					? t("onedrive_authorization_completed_policy", {
							id: callback.policyId,
						})
					: undefined,
			});
			void reload().catch(handleApiError);
			const policyId = Number(callback.policyId);
			if (Number.isSafeInteger(policyId) && policyId > 0) {
				void openPolicyById(policyId).catch(handleApiError);
			}
			return;
		}

		toast.error(t(storageAuthorizationFailureI18nKey(callback.reason)));
	}, [openPolicyById, reload, searchParams, setSearchParams, t]);

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
		setCosCorsConfirmOpen(false);
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
		setCosCorsConfirmOpen(false);
		setValidatedConnectionKey(null);
		setCreateStepTouched(false);
		setForm((prev) => {
			const { s3_path_style: previousS3PathStyle, ...prevWithoutS3PathStyle } =
				prev;
			const nextDriverDescriptor = getStorageDriverDescriptor(
				storageDriverDescriptors,
				driverType,
			);
			const nextSupportsStorageNativeProcessing =
				supportsStorageNativeProcessing(nextDriverDescriptor);
			if (supportsObjectStorageConnection(nextDriverDescriptor)) {
				return {
					...prevWithoutS3PathStyle,
					driver_type: driverType,
					remote_node_id: "",
					storage_native_processing_enabled: nextSupportsStorageNativeProcessing
						? prev.storage_native_processing_enabled
						: false,
					thumbnail_processor: nextSupportsStorageNativeProcessing
						? prev.thumbnail_processor
						: null,
					thumbnail_extensions: nextSupportsStorageNativeProcessing
						? prev.thumbnail_extensions
						: [],
					storage_native_media_metadata_enabled:
						nextSupportsStorageNativeProcessing
							? prev.storage_native_media_metadata_enabled
							: false,
					media_metadata_extensions: nextSupportsStorageNativeProcessing
						? (prev.media_metadata_extensions ?? [])
						: [],
					...(supportsS3TransferStrategy(nextDriverDescriptor)
						? { s3_path_style: previousS3PathStyle ?? true }
						: {}),
				};
			}

			if (supportsRemoteNodeBinding(nextDriverDescriptor)) {
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

			if (supportsOneDrivePolicyOptions(nextDriverDescriptor)) {
				return {
					...prevWithoutS3PathStyle,
					driver_type: driverType,
					endpoint: "",
					bucket: "",
					access_key: "",
					secret_key: "",
					remote_node_id: "",
					content_dedup: false,
					onedrive_cloud: prev.onedrive_cloud || "global",
					onedrive_account_mode: prev.onedrive_account_mode || "work_or_school",
					onedrive_tenant: prev.onedrive_tenant || "common",
					onedrive_drive_id: prev.onedrive_drive_id,
					onedrive_root_item_id: prev.onedrive_root_item_id,
					onedrive_site_id: prev.onedrive_site_id,
					onedrive_group_id: prev.onedrive_group_id,
					application_credentials: {
						microsoft_graph: {
							cloud: prev.onedrive_cloud || "global",
							tenant: prev.onedrive_tenant || "common",
							client_id: "",
							client_secret: "",
							scopes: "",
						},
					},
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

	const syncNormalizedPolicyForm = () => {
		const descriptor = getStorageDriverDescriptor(
			storageDriverDescriptors,
			form.driver_type,
		);
		const normalizedForm = normalizePolicyForm(form, descriptor);
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
		const currentForm = syncNormalizedPolicyForm();
		const descriptor = getStorageDriverDescriptor(
			storageDriverDescriptors,
			currentForm.driver_type,
		);
		const currentEndpointValidationMessage = getEndpointValidationMessage(
			currentForm,
			t,
			descriptor,
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
			hasConnectionFieldChanges(currentForm, editingPolicy, descriptor);
		if (shouldUseParamTest && !supportsDraftConnectionTest(descriptor)) {
			setValidatedConnectionKey(null);
			return false;
		}
		if (!shouldUseParamTest && !supportsSavedConnectionTest(descriptor)) {
			setValidatedConnectionKey(null);
			return false;
		}

		try {
			const result = shouldUseParamTest
				? await adminPolicyService.testParams(
						buildPolicyTestPayload(currentForm, descriptor),
					)
				: await adminPolicyService.testConnection(editingId);

			if (result?.ok === false) {
				setValidatedConnectionKey(null);
				if (showFailureError) {
					toast.error(result.diagnostic?.message ?? t("connection_failed"));
				}
				return false;
			}

			if (
				supportsObjectStorageConnection(descriptor) ||
				supportsRemoteNodeBinding(descriptor)
			) {
				setValidatedConnectionKey(
					getPolicyConnectionTestKey(currentForm, descriptor),
				);
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
			const currentForm = syncNormalizedPolicyForm();
			const descriptor = getStorageDriverDescriptor(
				storageDriverDescriptors,
				currentForm.driver_type,
			);
			if (editingId) {
				const updated = await adminPolicyService.update(
					editingId,
					buildUpdatePolicyPayload(currentForm, descriptor),
				);
				invalidateAdminPolicyLookup();
				setEditingId(updated.id);
				setEditingPolicy(updated);
				setForm(getPolicyForm(updated));
				setValidatedConnectionKey(null);
				loadPolicyCapacity(updated.id);
				setPolicies((prev) =>
					prev.map((policy) => (policy.id === editingId ? updated : policy)),
				);
				toast.success(t("policy_updated"));
			} else {
				const created = await adminPolicyService.create(
					buildCreatePolicyPayload(currentForm, descriptor),
				);
				invalidateAdminPolicyLookup();
				if (supportsStorageCredentialLifecycle(descriptor)) {
					setEditingId(created.id);
					setEditingPolicy(created);
					setForm(getPolicyForm(created));
					setValidatedConnectionKey(null);
					setCreateStep(0);
					setCreateStepTouched(false);
					setPolicies((prev) => {
						const existing = prev.some((policy) => policy.id === created.id);
						return existing
							? prev.map((policy) =>
									policy.id === created.id ? created : policy,
								)
							: [created, ...prev];
					});
					setTotal((current) => current + 1);
					loadPolicyCapacity(created.id);
					toast.success(t("policy_onedrive_created_authorize_next"));
					return;
				}
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
		const descriptor = currentStorageDriverDescriptor;
		if (!supportsDraftConnectionTest(descriptor)) {
			return false;
		}

		if (
			editingId !== null &&
			!hasConnectionFieldChanges(
				form,
				editingPolicy,
				currentStorageDriverDescriptor,
			)
		) {
			return false;
		}

		return (
			validatedConnectionKey !==
			getPolicyConnectionTestKey(form, currentStorageDriverDescriptor)
		);
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

	const cancelCosCorsConfigure = () => {
		setCosCorsConfirmOpen(false);
	};

	const requestOrConfirmCosCorsConfigure = () => {
		if (cosCorsConfirmOpen) {
			void configureTencentCosCors();
			return;
		}
		setSaveAnywayConfirmOpen(false);
		setCosCorsConfirmOpen(true);
	};

	const configureTencentCosCors = async () => {
		if (!canConfigureTencentCosCors) {
			return;
		}

		await runWithCosCorsConfigure(async () => {
			try {
				const currentForm = syncNormalizedPolicyForm();
				const descriptor = getStorageDriverDescriptor(
					storageDriverDescriptors,
					currentForm.driver_type,
				);
				const currentEndpointValidationMessage = getEndpointValidationMessage(
					currentForm,
					t,
					descriptor,
				);
				if (currentEndpointValidationMessage) {
					toast.error(currentEndpointValidationMessage);
					return;
				}

				const shouldUseDraft =
					editingId === null ||
					hasConnectionFieldChanges(currentForm, editingPolicy, descriptor);
				const result =
					editingId !== null && !shouldUseDraft
						? await adminPolicyService.executeSavedPolicyAction(editingId, {
								action: "configure_tencent_cos_cors",
							})
						: await adminPolicyService.executeDraftPolicyAction(
								buildTencentCosCorsPayload(currentForm, editingId, descriptor),
							);
				const requestId = result.tencent_cos_cors?.request_id;
				setCosCorsConfirmOpen(false);
				toast.success(t("policy_cos_cors_success"), {
					description: requestId
						? t("policy_cos_cors_success_request_id", {
								requestId,
							})
						: undefined,
				});
			} catch (error) {
				handleApiError(error);
			}
		});
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

	const startStorageAuthorization = () => {
		if (
			editingId === null ||
			!editingPolicy ||
			!isMicrosoftGraphAuthorizationProvider
		) {
			return;
		}
		if (
			policyFormHasUnsavedChanges(
				form,
				editingPolicy,
				currentStorageDriverDescriptor,
			)
		) {
			toast.error(t("onedrive_save_before_authorize"));
			return;
		}
		const currentForm = normalizePolicyForm(form);
		const microsoftGraph = microsoftGraphCredentials(currentForm);
		const scopes = parseMicrosoftGraphScopes(microsoftGraph.scopes);

		void runWithStorageAuthorization(async () => {
			try {
				const result = await adminPolicyService.startStorageAuthorization(
					editingId,
					{
						provider: MICROSOFT_GRAPH_PROVIDER,
						microsoft_graph: {
							cloud: currentForm.onedrive_cloud,
							tenant: currentForm.onedrive_tenant || undefined,
							client_id: microsoftGraph.client_id || undefined,
							client_secret: microsoftGraph.client_secret || undefined,
							scopes: scopes.length > 0 ? scopes : undefined,
						},
					},
				);
				toast.success(t("onedrive_authorization_started"));
				const opened = window.open(result.authorization_url, "_blank");
				if (opened) {
					opened.opener = null;
				} else {
					window.location.assign(result.authorization_url);
				}
			} catch (error) {
				handleApiError(error);
			}
		});
	};

	const validateStorageCredential = () => {
		if (editingId === null || !isMicrosoftGraphAuthorizationProvider) {
			return;
		}
		if (
			policyFormHasUnsavedChanges(
				form,
				editingPolicy,
				currentStorageDriverDescriptor,
			)
		) {
			toast.error(t("onedrive_save_before_validate"));
			return;
		}

		const policyId = editingId;
		const validationRequestSerial =
			++storageCredentialValidationRequestSerial.current;

		void runWithStorageCredentialValidation(async () => {
			try {
				const isCurrentValidationRequest = () =>
					validationRequestSerial ===
						storageCredentialValidationRequestSerial.current &&
					policyId === currentEditingIdRef.current;
				if (!isCurrentValidationRequest()) {
					return;
				}

				const result = await adminPolicyService.validateStorageCredential(
					policyId,
					MICROSOFT_GRAPH_PROVIDER,
				);
				if (isCurrentValidationRequest()) {
					setStorageCredentials((prev) => {
						const nextCredential = result.credential;
						const hasExisting = prev.some(
							(credential) => credential.provider === nextCredential.provider,
						);
						return hasExisting
							? prev.map((credential) =>
									credential.provider === nextCredential.provider
										? nextCredential
										: credential,
								)
							: [nextCredential, ...prev];
					});
					loadPolicyCapacity(policyId);
					toast.success(t("onedrive_validation_success"), {
						description: result.root_item_name
							? t("onedrive_validation_success_root", {
									name: result.root_item_name,
								})
							: undefined,
					});
				}
			} catch (error) {
				if (
					validationRequestSerial ===
						storageCredentialValidationRequestSerial.current &&
					policyId === currentEditingIdRef.current
				) {
					handleApiError(error);
				}
			}
		});
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

		if (
			supportsObjectStorageConnection(currentStorageDriverDescriptor) &&
			!form.bucket.trim()
		) {
			return;
		}

		if (
			supportsObjectStorageConnection(currentStorageDriverDescriptor) &&
			!form.endpoint.trim()
		) {
			return;
		}

		if (
			supportsRemoteNodeBinding(currentStorageDriverDescriptor) &&
			!form.remote_node_id
		) {
			return;
		}

		if (
			supportsApplicationCredentials(currentStorageDriverDescriptor) &&
			!microsoftGraphCredentials(form).client_id.trim()
		) {
			return;
		}

		if (
			supportsApplicationCredentials(currentStorageDriverDescriptor) &&
			!microsoftGraphCredentials(form).client_secret.trim()
		) {
			return;
		}

		if (endpointValidationMessage) {
			return;
		}

		syncNormalizedPolicyForm();
		setCreateStepTouched(false);
		setCreateStep(CREATE_LAST_STEP);
	};

	const handleSubmit = () => {
		if (editingId === null && createStep < CREATE_LAST_STEP) {
			handleCreateNext();
			return;
		}
		if (
			editingId === null &&
			supportsApplicationCredentials(currentStorageDriverDescriptor) &&
			(!microsoftGraphCredentials(form).client_id.trim() ||
				!microsoftGraphCredentials(form).client_secret.trim())
		) {
			setCreateStepTouched(true);
			setCreateStep(1);
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
			const [policyPage, remoteNodeLookup, descriptors] = await Promise.all([
				adminPolicyService.list({
					limit: pageSize,
					offset,
					sort_by: sortBy,
					sort_order: sortOrder,
				}),
				loadAdminRemoteNodeLookup({ force: true }),
				loadAdminStorageDriverDescriptors({ force: true }),
			]);
			setPolicies(policyPage.items);
			setTotal(policyPage.total);
			setRemoteNodes(remoteNodeLookup);
			setStorageDriverDescriptors(descriptors);
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
					storageDriverDescriptors={storageDriverDescriptors}
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
					storageDriverDescriptor={currentStorageDriverDescriptor}
					storageDriverDescriptors={storageDriverDescriptors}
					storageDriverDescriptorsError={storageDriverDescriptorsError}
					storageDriverDescriptorsLoading={storageDriverDescriptorsLoading}
					policyCapacity={policyCapacity}
					policyCapacityLoading={policyCapacityLoading}
					storageCredentials={storageCredentials}
					storageCredentialsLoading={storageCredentialsLoading}
					storageAuthorizationSubmitting={storageAuthorizationSubmitting}
					storageCredentialValidationSubmitting={
						storageCredentialValidationSubmitting
					}
					storageAuthorizationRedirectUri={storageAuthorizationRedirectUri}
					cosCorsConfirmOpen={cosCorsConfirmOpen}
					cosCorsSubmitting={cosCorsSubmitting}
					cosCorsUsesDraftValues={cosCorsUsesDraftValues}
					canConfigureTencentCosCors={canConfigureTencentCosCors}
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
					onCancelCosCorsConfigure={cancelCosCorsConfigure}
					onCancelSaveAnyway={cancelSaveAnyway}
					onCancelS3DriverPromotion={cancelS3DriverPromotion}
					onConfirmSaveAnyway={confirmSaveAnyway}
					onConfirmCosCorsConfigure={requestOrConfirmCosCorsConfigure}
					onConfirmS3DriverPromotion={confirmS3DriverPromotion}
					onStartStorageAuthorization={startStorageAuthorization}
					onValidateStorageCredential={validateStorageCredential}
					onDialogOpenChange={handleDialogOpenChange}
					onSubmit={handleSubmit}
					onRequestS3DriverPromotion={requestS3DriverPromotion}
					onRunConnectionTest={() => runConnectionTest()}
					onFieldChange={setField}
					onDriverTypeChange={setDriverType}
					onCreateBack={handleCreateBack}
					onCreateStepChange={handleCreateStepChange}
					onCreateNext={handleCreateNext}
					onSyncNormalizedS3Form={syncNormalizedPolicyForm}
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
