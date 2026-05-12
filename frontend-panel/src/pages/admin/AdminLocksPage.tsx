import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import {
	AdminSortableTableHead,
	AdminTableCell as TableCell,
	AdminTableHead as TableHead,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { AdminTableList } from "@/components/common/AdminTableList";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { StatusBadge } from "@/components/common/StatusBadge";
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
	parseSortOrderSearchParam,
	parseSortSearchParam,
	type SortOrder,
} from "@/lib/pagination";
import type { WebdavLock } from "@/services/adminService";
import { adminLockService } from "@/services/adminService";
import type { AdminLockSortBy } from "@/types/adminSort";

const LOCK_SORT_BY_OPTIONS = [
	"id",
	"path",
	"entity_type",
	"owner_id",
	"timeout_at",
	"shared",
	"deep",
	"created_at",
] as const satisfies readonly AdminLockSortBy[];
const DEFAULT_LOCK_SORT_BY = "id" as const satisfies AdminLockSortBy;
const DEFAULT_LOCK_SORT_ORDER = "asc" as const satisfies SortOrder;

function formatLockOwnerInfo(lock: WebdavLock) {
	if (!lock.owner_info) {
		return null;
	}

	switch (lock.owner_info.kind) {
		case "wopi":
			return `WOPI (${lock.owner_info.app_key})`;
		case "webdav":
			return lock.owner_info.xml;
		case "text":
			return lock.owner_info.value;
	}
}

export default function AdminLocksPage() {
	const { t } = useTranslation("admin");
	usePageTitle(t("webdav_locks"));
	const [searchParams, setSearchParams] = useSearchParams();
	const [sortBy, setSortBy] = useState<AdminLockSortBy>(
		parseSortSearchParam(
			searchParams.get("sortBy"),
			LOCK_SORT_BY_OPTIONS,
			DEFAULT_LOCK_SORT_BY,
		),
	);
	const [sortOrder, setSortOrder] = useState<SortOrder>(
		parseSortOrderSearchParam(
			searchParams.get("sortOrder"),
			DEFAULT_LOCK_SORT_ORDER,
		),
	);
	const {
		items: locks,
		setItems: setLocks,
		loading,
		reload,
	} = useApiList(
		() =>
			adminLockService.list({
				limit: 100,
				offset: 0,
				sort_by: sortBy,
				sort_order: sortOrder,
			}),
		[sortBy, sortOrder],
	);

	useEffect(() => {
		setSearchParams(
			buildOffsetPaginationSearchParams({
				offset: 0,
				pageSize: 100,
				defaultPageSize: 100,
				extraParams: {
					sortBy: sortBy !== DEFAULT_LOCK_SORT_BY ? sortBy : undefined,
					sortOrder:
						sortOrder !== DEFAULT_LOCK_SORT_ORDER ? sortOrder : undefined,
				},
			}),
			{ replace: true },
		);
	}, [setSearchParams, sortBy, sortOrder]);

	const handleForceUnlock = async (id: number) => {
		try {
			await adminLockService.forceUnlock(id);
			setLocks((prev) => prev.filter((l) => l.id !== id));
			toast.success(t("lock_released"));
		} catch (e) {
			handleApiError(e);
		}
	};

	const {
		confirmId: unlockId,
		requestConfirm: requestUnlock,
		dialogProps,
	} = useConfirmDialog(handleForceUnlock);

	const handleCleanupExpired = async () => {
		try {
			const result = await adminLockService.cleanupExpired();
			toast.success(t("expired_locks_cleaned", { count: result.removed }));
			void reload();
		} catch (e) {
			handleApiError(e);
		}
	};

	const handleSortChange = (
		nextSortBy: AdminLockSortBy,
		nextOrder: SortOrder,
	) => {
		setSortBy(nextSortBy);
		setSortOrder(nextOrder);
	};

	const isExpired = (l: WebdavLock) =>
		l.timeout_at != null && new Date(l.timeout_at) < new Date();

	const unlockPath =
		unlockId !== null ? (locks.find((l) => l.id === unlockId)?.path ?? "") : "";

	return (
		<AdminLayout>
			<AdminPageShell>
				<AdminPageHeader
					title={t("webdav_locks")}
					description={t("locks_intro")}
					actions={
						<Button variant="outline" size="sm" onClick={handleCleanupExpired}>
							{t("clean_expired")}
						</Button>
					}
				/>
				<AdminTableList
					loading={loading}
					items={locks}
					columns={7}
					rows={6}
					emptyIcon={<Icon name="Lock" className="h-10 w-10" />}
					emptyTitle={t("no_active_locks")}
					emptyDescription={t("no_active_locks_desc")}
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
									sortKey="path"
									sortBy={sortBy}
									sortOrder={sortOrder}
									onSortChange={handleSortChange}
								>
									{t("path")}
								</AdminSortableTableHead>
								<AdminSortableTableHead
									sortKey="owner_id"
									sortBy={sortBy}
									sortOrder={sortOrder}
									onSortChange={handleSortChange}
								>
									{t("owner")}
								</AdminSortableTableHead>
								<AdminSortableTableHead
									sortKey="entity_type"
									sortBy={sortBy}
									sortOrder={sortOrder}
									onSortChange={handleSortChange}
								>
									{t("core:type")}
								</AdminSortableTableHead>
								<AdminSortableTableHead
									sortKey="timeout_at"
									sortBy={sortBy}
									sortOrder={sortOrder}
									onSortChange={handleSortChange}
								>
									{t("core:status")}
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
					renderRow={(l) => (
						<TableRow key={l.id}>
							<TableCell className="font-mono text-xs">{l.id}</TableCell>
							<TableCell className="font-mono text-xs max-w-[200px] truncate">
								{l.path}
							</TableCell>
							<TableCell>
								{formatLockOwnerInfo(l) ?? <UserIdentity user={l.owner} />}
							</TableCell>
							<TableCell>
								<div className="flex gap-1">
									<Badge variant="outline">
										{l.shared ? t("shared_lock") : t("exclusive")}
									</Badge>
									{l.deep && <Badge variant="outline">{t("deep")}</Badge>}
								</div>
							</TableCell>
							<TableCell>
								{isExpired(l) ? (
									<StatusBadge status="expired" />
								) : (
									<StatusBadge status="active" />
								)}
							</TableCell>
							<TableCell className="text-muted-foreground text-xs">
								{formatDateShort(l.created_at)}
							</TableCell>
							<TableCell>
								<Button
									variant="ghost"
									size="icon"
									className={`${ADMIN_ICON_BUTTON_CLASS} text-destructive`}
									onClick={() => requestUnlock(l.id)}
								>
									<Icon name="Trash" className="h-3.5 w-3.5" />
								</Button>
							</TableCell>
						</TableRow>
					)}
				/>
			</AdminPageShell>

			<ConfirmDialog
				{...dialogProps}
				title={`Force unlock "${unlockPath}"?`}
				description={t("force_unlock_desc")}
				confirmLabel={t("core:confirm")}
				variant="destructive"
			/>
		</AdminLayout>
	);
}
