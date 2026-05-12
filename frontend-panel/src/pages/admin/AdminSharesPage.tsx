import { useEffect, useState } from "react";
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
import { useApiList } from "@/hooks/useApiList";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { usePageTitle } from "@/hooks/usePageTitle";
import {
	ADMIN_ICON_BUTTON_CLASS,
	ADMIN_TABLE_ACTIONS_WIDTH_CLASS,
} from "@/lib/constants";
import { formatDateShort } from "@/lib/format";
import {
	buildOffsetPaginationSearchParams,
	parseOffsetSearchParam,
	parsePageSizeOption,
	parsePageSizeSearchParam,
	parseSortOrderSearchParam,
	parseSortSearchParam,
	type SortOrder,
} from "@/lib/pagination";
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

export default function AdminSharesPage() {
	const { t } = useTranslation("admin");
	usePageTitle(t("shares"));
	const [searchParams, setSearchParams] = useSearchParams();
	const [offset, setOffset] = useState(
		parseOffsetSearchParam(searchParams.get("offset")),
	);
	const [pageSize, setPageSize] = useState<
		(typeof SHARE_PAGE_SIZE_OPTIONS)[number]
	>(
		parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			SHARE_PAGE_SIZE_OPTIONS,
			DEFAULT_SHARE_PAGE_SIZE,
		),
	);
	const [sortBy, setSortBy] = useState<AdminShareSortBy>(
		parseSortSearchParam(
			searchParams.get("sortBy"),
			SHARE_SORT_BY_OPTIONS,
			DEFAULT_SHARE_SORT_BY,
		),
	);
	const [sortOrder, setSortOrder] = useState<SortOrder>(
		parseSortOrderSearchParam(
			searchParams.get("sortOrder"),
			DEFAULT_SHARE_SORT_ORDER,
		),
	);
	const {
		items: shares,
		setItems: setShares,
		setTotal,
		total,
		loading,
	} = useApiList(
		() =>
			adminShareService.list({
				limit: pageSize,
				offset,
				sort_by: sortBy,
				sort_order: sortOrder,
			}),
		[offset, pageSize, sortBy, sortOrder],
	);
	const totalPages = Math.max(1, Math.ceil(total / pageSize));
	const currentPage = Math.floor(offset / pageSize) + 1;
	const prevPageDisabled = offset === 0;
	const nextPageDisabled = offset + pageSize >= total;
	const pageSizeOptions = SHARE_PAGE_SIZE_OPTIONS.map((size) => ({
		label: t("page_size_option", { count: size }),
		value: String(size),
	}));

	useEffect(() => {
		setSearchParams(
			buildOffsetPaginationSearchParams({
				offset,
				pageSize,
				defaultPageSize: DEFAULT_SHARE_PAGE_SIZE,
				extraParams: {
					sortBy: sortBy !== DEFAULT_SHARE_SORT_BY ? sortBy : undefined,
					sortOrder:
						sortOrder !== DEFAULT_SHARE_SORT_ORDER ? sortOrder : undefined,
				},
			}),
			{ replace: true },
		);
	}, [offset, pageSize, setSearchParams, sortBy, sortOrder]);

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, SHARE_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setPageSize(next);
		setOffset(0);
	};

	const handleSortChange = (
		nextSortBy: AdminShareSortBy,
		nextOrder: SortOrder,
	) => {
		setSortBy(nextSortBy);
		setSortOrder(nextOrder);
		setOffset(0);
	};

	const handleDelete = async (id: number) => {
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

	return (
		<AdminLayout>
			<AdminPageShell>
				<AdminPageHeader title={t("shares")} description={t("shares_intro")} />
				<AdminTableList
					loading={loading}
					items={shares}
					columns={8}
					rows={6}
					emptyIcon={<Icon name="LinkSimple" className="h-10 w-10" />}
					emptyTitle={t("no_shares")}
					emptyDescription={t("no_shares_desc")}
					headerRow={
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
					}
					renderRow={(s) => (
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
									<Icon name="ArrowSquareOut" className="h-3 w-3" />
								</a>
							</TableCell>
							<TableCell>
								<UserIdentity user={s.user} />
							</TableCell>
							<TableCell>
								<Badge variant="outline">
									{s.target.type === "file" ? t("core:file") : t("core:folder")}
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
								>
									<Icon name="Trash" className="h-3.5 w-3.5" />
								</Button>
							</TableCell>
						</TableRow>
					)}
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
					onPrevious={() => setOffset(Math.max(0, offset - pageSize))}
					onNext={() => setOffset(offset + pageSize)}
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
