import { useTranslation } from "react-i18next";
import { useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
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
import { usePageTitle } from "@/hooks/usePageTitle";
import { usePendingId } from "@/hooks/usePendingId";
import { invalidateAdminRemoteNodeLookup } from "@/lib/adminRemoteNodeLookup";
import { parsePageSizeOption, type SortOrder } from "@/lib/pagination";
import { adminRemoteNodeService } from "@/services/adminService";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import type { AdminRemoteNodeSortBy } from "@/types/adminSort";
import type { RemoteNodeInfo } from "@/types/api";

export const REMOTE_NODE_PAGE_SIZE_OPTIONS = [10, 20, 50] as const;
const DEFAULT_REMOTE_NODE_PAGE_SIZE = 20 as const;
const REMOTE_NODE_SORT_BY_OPTIONS = [
	"id",
	"name",
	"base_url",
	"is_enabled",
	"last_probe_at",
	"created_at",
	"updated_at",
] as const satisfies readonly AdminRemoteNodeSortBy[];
const DEFAULT_REMOTE_NODE_SORT_BY =
	"created_at" as const satisfies AdminRemoteNodeSortBy;
const DEFAULT_REMOTE_NODE_SORT_ORDER = "desc" as const satisfies SortOrder;

type ManagedRemoteNodeQuery = {
	offset: number;
	pageSize: (typeof REMOTE_NODE_PAGE_SIZE_OPTIONS)[number];
	sortBy: AdminRemoteNodeSortBy;
	sortOrder: SortOrder;
};

const QUERY_DEFAULTS = {
	offset: 0,
	pageSize: DEFAULT_REMOTE_NODE_PAGE_SIZE,
	sortBy: DEFAULT_REMOTE_NODE_SORT_BY,
	sortOrder: DEFAULT_REMOTE_NODE_SORT_ORDER,
} satisfies ManagedRemoteNodeQuery;

const QUERY_SCHEMA = {
	offset: managedOffsetQueryField(),
	pageSize: managedPageSizeQueryField(
		REMOTE_NODE_PAGE_SIZE_OPTIONS,
		DEFAULT_REMOTE_NODE_PAGE_SIZE,
	),
	sortBy: managedSortByQueryField(
		REMOTE_NODE_SORT_BY_OPTIONS,
		DEFAULT_REMOTE_NODE_SORT_BY,
	),
	sortOrder: managedSortOrderQueryField(DEFAULT_REMOTE_NODE_SORT_ORDER),
} satisfies ManagedListQuerySchema<ManagedRemoteNodeQuery>;

export function useAdminRemoteNodesPageController() {
	const { t } = useTranslation("admin");
	const navigate = useNavigate();
	const primarySiteUrl = useFrontendConfigStore((state) => state.siteUrl);
	const [searchParams, setSearchParams] = useSearchParams();
	const { query, setQuery } = useManagedListQueryState({
		defaults: QUERY_DEFAULTS,
		schema: QUERY_SCHEMA,
		searchParams,
		setSearchParams,
	});
	const setOffset = useManagedOffset(setQuery);
	const {
		currentPage,
		items: remoteNodes,
		loading,
		reload,
		nextPageDisabled,
		prevPageDisabled,
		total,
		totalPages,
	} = useManagedAdminList<RemoteNodeInfo, ManagedRemoteNodeQuery>({
		loadPage: (currentQuery) =>
			adminRemoteNodeService.list({
				limit: currentQuery.pageSize,
				offset: currentQuery.offset,
				sort_by: currentQuery.sortBy,
				sort_order: currentQuery.sortOrder,
			}),
		query,
		setOffset,
	});
	const {
		pendingId: deletingRemoteNodeId,
		runWithPending: runWithDeletingRemoteNode,
	} = usePendingId<number>();
	const {
		confirmId: deleteId,
		requestConfirm,
		dialogProps: deleteDialogProps,
	} = useConfirmDialog(async (id: number) => {
		await runWithDeletingRemoteNode(id, async () => {
			try {
				await adminRemoteNodeService.delete(id);
				invalidateAdminRemoteNodeLookup();
				if (remoteNodes.length === 1 && query.offset > 0) {
					setOffset(Math.max(0, query.offset - query.pageSize));
				} else {
					await reload();
				}
				toast.success(t("remote_node_deleted"));
			} catch (error) {
				handleApiError(error);
			}
		});
	});

	usePageTitle(t("remote_nodes"));

	return {
		createButtonTitle: primarySiteUrl
			? undefined
			: t("remote_node_primary_site_url_required"),
		currentPage,
		deleteDialogProps,
		deleteNodeName:
			deleteId == null
				? ""
				: (remoteNodes.find((node) => node.id === deleteId)?.name ?? ""),
		deletingRemoteNodeId,
		handlePageSizeChange: (value: string | null) => {
			const next = parsePageSizeOption(value, REMOTE_NODE_PAGE_SIZE_OPTIONS);
			if (next != null) setQuery({ offset: 0, pageSize: next });
		},
		handleRefresh: async () => {
			try {
				invalidateAdminRemoteNodeLookup();
				await reload();
			} catch (error) {
				handleApiError(error);
			}
		},
		handleSortChange: (sortBy: AdminRemoteNodeSortBy, sortOrder: SortOrder) =>
			setQuery({ offset: 0, sortBy, sortOrder }),
		loading,
		nextPageDisabled,
		openCreate: () => {
			if (!primarySiteUrl) {
				toast.error(t("remote_node_primary_site_url_required"));
				return;
			}
			navigate("/admin/remote-nodes/new", { viewTransition: false });
		},
		openEdit: (node: RemoteNodeInfo) =>
			navigate(`/admin/remote-nodes/${node.id}`, { viewTransition: false }),
		pageSize: query.pageSize,
		pageSizeOptions: REMOTE_NODE_PAGE_SIZE_OPTIONS.map((size) => ({
			label: t("page_size_option", { count: size }),
			value: String(size),
		})),
		prevPageDisabled,
		remoteNodes,
		requestConfirm,
		setOffset,
		sortBy: query.sortBy,
		sortOrder: query.sortOrder,
		t,
		total,
		totalPages,
	};
}
