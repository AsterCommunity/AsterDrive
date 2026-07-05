import { type SetStateAction, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import {
	AdminSortableTableHead,
	AdminTableCell as TableCell,
	AdminTableHead as TableHead,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { AdminTableList } from "@/components/common/AdminTableList";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { UserIdentity } from "@/components/common/UserIdentity";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { useManagedAdminList } from "@/hooks/useManagedAdminList";
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
import {
	ADMIN_ICON_BUTTON_CLASS,
	ADMIN_TABLE_ACTIONS_WIDTH_CLASS,
} from "@/lib/constants";
import { formatDateShort } from "@/lib/format";
import { parsePageSizeOption, type SortOrder } from "@/lib/pagination";
import { adminShareService } from "@/services/adminService";
import type { AdminShareSortBy } from "@/types/adminSort";
import type { ShareInfo } from "@/types/api";

const SHARE_PAGE_SIZE_OPTIONS = [10, 20, 50] as const;
const DEFAULT_SHARE_PAGE_SIZE = 20 as const;
const SHARE_SORT_BY_OPTIONS = [
	"id",
	"token",
	"user_id",
	"download_count",
	"max_downloads",
	"expires_at",
	"created_at",
	"updated_at",
] as const satisfies readonly AdminShareSortBy[];
const DEFAULT_SHARE_SORT_BY = "created_at" as const satisfies AdminShareSortBy;
const DEFAULT_SHARE_SORT_ORDER = "desc" as const satisfies SortOrder;

type ManagedShareQuery = {
	offset: number;
	pageSize: (typeof SHARE_PAGE_SIZE_OPTIONS)[number];
	sortBy: AdminShareSortBy;
	sortOrder: SortOrder;
};

const MANAGED_SHARE_QUERY_DEFAULTS = {
	offset: 0,
	pageSize: DEFAULT_SHARE_PAGE_SIZE,
	sortBy: DEFAULT_SHARE_SORT_BY,
	sortOrder: DEFAULT_SHARE_SORT_ORDER,
} satisfies ManagedShareQuery;

const MANAGED_SHARE_QUERY_SCHEMA = {
	offset: managedOffsetQueryField(),
	pageSize: managedPageSizeQueryField(
		SHARE_PAGE_SIZE_OPTIONS,
		DEFAULT_SHARE_PAGE_SIZE,
	),
	sortBy: managedSortByQueryField(SHARE_SORT_BY_OPTIONS, DEFAULT_SHARE_SORT_BY),
	sortOrder: managedSortOrderQueryField(DEFAULT_SHARE_SORT_ORDER),
} satisfies ManagedListQuerySchema<ManagedShareQuery>;

export default function AdminSharesPage() {
	const { t } = useTranslation("admin");
	usePageTitle(t("shares"));
	const [searchParams, setSearchParams] = useSearchParams();
	const { query, setQuery } = useManagedListQueryState({
		defaults: MANAGED_SHARE_QUERY_DEFAULTS,
		schema: MANAGED_SHARE_QUERY_SCHEMA,
		searchParams,
		setSearchParams,
	});
	const { offset, pageSize, sortBy, sortOrder } = query;
	const setOffset = useCallback(
		(value: SetStateAction<number>) => {
			setQuery((current) => ({
				offset: typeof value === "function" ? value(current.offset) : value,
			}));
		},
		[setQuery],
	);
	const {
		currentPage,
		items: shares,
		setItems: setShares,
		setTotal,
		total,
		totalPages,
		loading,
		nextPageDisabled,
		prevPageDisabled,
	} = useManagedAdminList<ShareInfo, ManagedShareQuery>({
		deps: [offset, pageSize, sortBy, sortOrder],
		loadPage: (query) =>
			adminShareService.list({
				limit: query.pageSize,
				offset: query.offset,
				sort_by: query.sortBy,
				sort_order: query.sortOrder,
			}),
		query,
		setOffset,
	});
	const pageSizeOptions = SHARE_PAGE_SIZE_OPTIONS.map((size) => ({
		label: t("page_size_option", { count: size }),
		value: String(size),
	}));
	const { pendingId: deletingShareId, runWithPending: runWithDeletingShare } =
		usePendingId<number>();

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, SHARE_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setQuery({ offset: 0, pageSize: next });
	};

	const handleSortChange = (
		nextSortBy: AdminShareSortBy,
		nextOrder: SortOrder,
	) => {
		setQuery({ offset: 0, sortBy: nextSortBy, sortOrder: nextOrder });
	};

	const handleDelete = async (id: number) => {
		await runWithDeletingShare(id, async () => {
			try {
				await adminShareService.delete(id);
				const isLastItemOnPage = shares.length === 1;
				const nextOffset =
					isLastItemOnPage && offset > 0
						? Math.max(0, offset - pageSize)
						: offset;
				if (nextOffset !== offset) {
					setOffset(nextOffset);
				} else {
					setShares((prev) => prev.filter((s) => s.id !== id));
					setTotal((prev) => Math.max(0, prev - 1));
				}
				toast.success(t("share_deleted"));
			} catch (e) {
				handleApiError(e);
			}
		});
	};

	const {
		confirmId: deleteId,
		requestConfirm,
		dialogProps,
	} = useConfirmDialog(handleDelete);

	const isExpired = (s: ShareInfo) =>
		s.expires_at != null && new Date(s.expires_at) < new Date();

	const isLimitReached = (s: ShareInfo) =>
		s.max_downloads > 0 && s.download_count >= s.max_downloads;

	const deleteToken =
		deleteId !== null
			? (shares.find((s) => s.id === deleteId)?.token ?? "")
			: "";
	const sharesEmptyIcon = <Icon name="LinkSimple" className="size-10" />;
	const sharesPagination = (
		<AdminOffsetPagination
			total={total}
			currentPage={currentPage}
			totalPages={totalPages}
			pageSize={String(pageSize)}
			pageSizeOptions={pageSizeOptions}
			onPageSizeChange={handlePageSizeChange}
			prevDisabled={prevPageDisabled}
			nextDisabled={nextPageDisabled}
			onPrevious={() => setOffset((current) => Math.max(0, current - pageSize))}
			onNext={() => setOffset((current) => current + pageSize)}
		/>
	);
	const sharesTableHeader = (
		<TableHeader>
			<TableRow>
				<AdminSortableTableHead
					className="w-16"
					sortKey="id"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={handleSortChange}
				>
					{t("id")}
				</AdminSortableTableHead>
				<AdminSortableTableHead
					sortKey="token"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={handleSortChange}
				>
					{t("token")}
				</AdminSortableTableHead>
				<AdminSortableTableHead
					sortKey="user_id"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={handleSortChange}
				>
					{t("audit_user")}
				</AdminSortableTableHead>
				<TableHead>{t("core:type")}</TableHead>
				<AdminSortableTableHead
					sortKey="expires_at"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={handleSortChange}
				>
					{t("core:status")}
				</AdminSortableTableHead>
				<AdminSortableTableHead
					sortKey="download_count"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={handleSortChange}
				>
					{t("downloads")}
				</AdminSortableTableHead>
				<AdminSortableTableHead
					sortKey="created_at"
					sortBy={sortBy}
					sortOrder={sortOrder}
					onSortChange={handleSortChange}
				>
					{t("core:created_at")}
				</AdminSortableTableHead>
				<TableHead className={ADMIN_TABLE_ACTIONS_WIDTH_CLASS}>
					{t("core:actions")}
				</TableHead>
			</TableRow>
		</TableHeader>
	);

	return (
		<AdminLayout>
			<AdminPageShell>
				<AdminPageHeader title={t("shares")} description={t("shares_intro")} />
				<AdminTableList
					loading={loading}
					items={shares}
					columns={8}
					rows={6}
					emptyIcon={sharesEmptyIcon}
					emptyTitle={t("no_shares")}
					emptyDescription={t("no_shares_desc")}
					pagination={sharesPagination}
					headerRow={sharesTableHeader}
					renderRow={(s) => {
						const isDeleting = deletingShareId === s.id;
						const deleteLabel = isDeleting
							? t("share_deleting")
							: t("core:delete");

						return (
							<TableRow key={s.id}>
								<TableCell className="font-mono text-xs">{s.id}</TableCell>
								<TableCell>
									<a
										href={`/s/${s.token}`}
										target="_blank"
										rel="noreferrer"
										className="font-mono text-xs text-primary hover:underline inline-flex items-center gap-1"
									>
										{s.token}
										<Icon name="ArrowSquareOut" className="size-3" />
									</a>
								</TableCell>
								<TableCell>
									<UserIdentity user={s.user} />
								</TableCell>
								<TableCell>
									<Badge variant="outline">
										{s.target.type === "file"
											? t("core:file")
											: t("core:folder")}
									</Badge>
								</TableCell>
								<TableCell>
									{isExpired(s) ? (
										<Badge
											variant="outline"
											className="text-red-600 dark:text-red-400 border-red-600 dark:border-red-400"
										>
											{t("core:expired")}
										</Badge>
									) : isLimitReached(s) ? (
										<Badge
											variant="outline"
											className="text-orange-600 dark:text-orange-400 border-orange-600 dark:border-orange-400"
										>
											{t("limit_reached")}
										</Badge>
									) : (
										<Badge
											variant="outline"
											className="text-green-600 dark:text-green-400 border-green-600 dark:border-green-400"
										>
											{t("core:active")}
										</Badge>
									)}
								</TableCell>
								<TableCell className="text-xs">
									{s.download_count}
									{s.max_downloads > 0 ? ` / ${s.max_downloads}` : ""}
								</TableCell>
								<TableCell className="text-muted-foreground text-xs">
									{formatDateShort(s.created_at)}
								</TableCell>
								<TableCell>
									<Button
										variant="ghost"
										size="icon"
										className={`${ADMIN_ICON_BUTTON_CLASS} text-destructive`}
										onClick={() => requestConfirm(s.id)}
										aria-label={deleteLabel}
										title={deleteLabel}
										disabled={isDeleting}
									>
										<Icon
											name={isDeleting ? "Spinner" : "Trash"}
											className={`size-3.5 ${isDeleting ? "animate-spin" : ""}`}
										/>
									</Button>
								</TableCell>
							</TableRow>
						);
					}}
				/>
			</AdminPageShell>

			<ConfirmDialog
				{...dialogProps}
				title={`${t("core:delete")} "${deleteToken}"?`}
				description={t("delete_share_desc")}
				confirmLabel={t("core:delete")}
				variant="destructive"
			/>
		</AdminLayout>
	);
}
