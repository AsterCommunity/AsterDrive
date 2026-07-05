import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { PROTECTED_POLICY_ID } from "@/components/admin/admin-policies-page/policyPresentation";
import { handleApiError } from "@/hooks/useApiError";
import { useApiList } from "@/hooks/useApiList";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { usePendingId } from "@/hooks/usePendingId";
import { invalidateAdminPolicyLookup } from "@/lib/adminPolicyLookup";
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
import type { DeletePolicyQuery } from "@/types/api";
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

export function useStoragePolicyListController() {
	const { t } = useTranslation("admin");
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
	const {
		clearPending: clearDeletingPolicy,
		pendingId: deletingPolicyId,
		runWithPending: runWithDeletingPolicy,
	} = usePendingId<number>();

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

	const totalPages = Math.max(1, Math.ceil(total / pageSize));
	const currentPage = Math.floor(offset / pageSize) + 1;
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
		nextPageDisabled: offset + pageSize >= total,
		offset,
		pageSize,
		pageSizeOptions,
		policies,
		prevPageDisabled: offset === 0,
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
