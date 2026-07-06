import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { PROTECTED_POLICY_ID } from "@/components/admin/admin-policies-page/policyPresentation";
import { handleApiError } from "@/hooks/useApiError";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import {
	useManagedAdminList,
	useManagedOffset,
} from "@/hooks/useManagedAdminList";
import {
	type ManagedListQuerySchema,
	managedOffsetQueryField,
	managedPageSizeQueryField,
	managedSortByQueryField,
	managedSortOrderQueryField,
	useManagedListQueryState,
} from "@/hooks/useManagedListQueryState";
import { usePendingId } from "@/hooks/usePendingId";
import { invalidateAdminPolicyLookup } from "@/lib/adminPolicyLookup";
import { parsePageSizeOption, type SortOrder } from "@/lib/pagination";
import { adminPolicyService } from "@/services/adminService";
import { ApiError } from "@/services/http";
import type { AdminPolicySortBy } from "@/types/adminSort";
import type { DeletePolicyQuery, StoragePolicy } from "@/types/api";
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
const POLICY_UPLOAD_SESSION_BLOCKER_CODE =
	ApiErrorCode.PolicyUploadSessionsExist;

type ManagedPolicyQuery = {
	offset: number;
	pageSize: (typeof POLICY_PAGE_SIZE_OPTIONS)[number];
	sortBy: AdminPolicySortBy;
	sortOrder: SortOrder;
};

const MANAGED_POLICY_QUERY_DEFAULTS = {
	offset: 0,
	pageSize: DEFAULT_POLICY_PAGE_SIZE,
	sortBy: DEFAULT_POLICY_SORT_BY,
	sortOrder: DEFAULT_POLICY_SORT_ORDER,
} satisfies ManagedPolicyQuery;

const MANAGED_POLICY_QUERY_SCHEMA = {
	offset: managedOffsetQueryField(),
	pageSize: managedPageSizeQueryField(
		POLICY_PAGE_SIZE_OPTIONS,
		DEFAULT_POLICY_PAGE_SIZE,
	),
	sortBy: managedSortByQueryField(
		POLICY_SORT_BY_OPTIONS,
		DEFAULT_POLICY_SORT_BY,
	),
	sortOrder: managedSortOrderQueryField(DEFAULT_POLICY_SORT_ORDER),
} satisfies ManagedListQuerySchema<ManagedPolicyQuery>;

export function useStoragePolicyListController() {
	const { t } = useTranslation("admin");
	const [searchParams, setSearchParams] = useSearchParams();
	const { query, setQuery } = useManagedListQueryState({
		defaults: MANAGED_POLICY_QUERY_DEFAULTS,
		schema: MANAGED_POLICY_QUERY_SCHEMA,
		searchParams,
		setSearchParams,
	});
	const { offset, pageSize, sortBy, sortOrder } = query;
	const setOffset = useManagedOffset(setQuery);
	const {
		currentPage,
		items: policies,
		setItems: setPolicies,
		total,
		totalPages,
		setTotal,
		loading,
		reload,
		nextPageDisabled,
		prevPageDisabled,
	} = useManagedAdminList<StoragePolicy, ManagedPolicyQuery>({
		loadPage: (query) =>
			adminPolicyService.list({
				limit: query.pageSize,
				offset: query.offset,
				sort_by: query.sortBy,
				sort_order: query.sortOrder,
			}),
		query,
		setOffset,
	});
	const {
		clearPending: clearDeletingPolicy,
		pendingId: deletingPolicyId,
		runWithPending: runWithDeletingPolicy,
	} = usePendingId<number>();

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, POLICY_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setQuery({ offset: 0, pageSize: next });
	};

	const handleSortChange = (
		nextSortBy: AdminPolicySortBy,
		nextOrder: SortOrder,
	) => {
		setQuery({ offset: 0, sortBy: nextSortBy, sortOrder: nextOrder });
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
		dialogProps: deleteDialogProps,
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

	const pageSizeOptions = POLICY_PAGE_SIZE_OPTIONS.map((size) => ({
		label: t("page_size_option", { count: size }),
		value: String(size),
	}));

	return {
		currentPage,
		deleteDialogProps,
		deleteId,
		deletingPolicyId,
		forceDeleteDialogProps,
		forceDeleteId,
		handlePageSizeChange,
		handleSortChange,
		loading,
		nextPageDisabled,
		offset,
		pageSize,
		pageSizeOptions,
		policies,
		prevPageDisabled,
		reload,
		requestDeleteConfirm,
		setOffset,
		setPolicies,
		setTotal,
		sortBy,
		sortOrder,
		total,
		totalPages,
	};
}
