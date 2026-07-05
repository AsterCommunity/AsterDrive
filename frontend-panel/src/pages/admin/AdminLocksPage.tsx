import { type SetStateAction, useCallback } from "react";
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
import type { SortOrder } from "@/lib/pagination";
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
const LOCK_PAGE_SIZE_OPTIONS = [100] as const;
const DEFAULT_LOCK_PAGE_SIZE = 100 as const;

type ManagedLockQuery = {
	offset: number;
	pageSize: (typeof LOCK_PAGE_SIZE_OPTIONS)[number];
	sortBy: AdminLockSortBy;
	sortOrder: SortOrder;
};

const MANAGED_LOCK_QUERY_DEFAULTS = {
	offset: 0,
	pageSize: DEFAULT_LOCK_PAGE_SIZE,
	sortBy: DEFAULT_LOCK_SORT_BY,
	sortOrder: DEFAULT_LOCK_SORT_ORDER,
} satisfies ManagedLockQuery;

const MANAGED_LOCK_QUERY_SCHEMA = {
	offset: managedOffsetQueryField(),
	pageSize: managedPageSizeQueryField(
		LOCK_PAGE_SIZE_OPTIONS,
		DEFAULT_LOCK_PAGE_SIZE,
	),
	sortBy: managedSortByQueryField(LOCK_SORT_BY_OPTIONS, DEFAULT_LOCK_SORT_BY),
	sortOrder: managedSortOrderQueryField(DEFAULT_LOCK_SORT_ORDER),
} satisfies ManagedListQuerySchema<ManagedLockQuery>;

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
	const { query, setQuery } = useManagedListQueryState({
		defaults: MANAGED_LOCK_QUERY_DEFAULTS,
		schema: MANAGED_LOCK_QUERY_SCHEMA,
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
		items: locks,
		setItems: setLocks,
		loading,
		reload,
	} = useManagedAdminList<WebdavLock, ManagedLockQuery>({
		deps: [offset, pageSize, sortBy, sortOrder],
		loadPage: (query) =>
			adminLockService.list({
				limit: query.pageSize,
				offset: query.offset,
				sort_by: query.sortBy,
				sort_order: query.sortOrder,
			}),
		query,
		setOffset,
	});
	const { pendingId: unlockingLockId, runWithPending: runWithUnlockingLock } =
		usePendingId<number>();

	const handleForceUnlock = async (id: number) => {
		await runWithUnlockingLock(id, async () => {
			try {
				await adminLockService.forceUnlock(id);
				setLocks((prev) => prev.filter((l) => l.id !== id));
				toast.success(t("lock_released"));
			} catch (e) {
				handleApiError(e);
			}
		});
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
		setQuery({ offset: 0, sortBy: nextSortBy, sortOrder: nextOrder });
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
					emptyIcon={<Icon name="Lock" className="size-10" />}
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
					renderRow={(l) => {
						const isUnlocking = unlockingLockId === l.id;
						const unlockLabel = isUnlocking
							? t("lock_releasing")
							: t("force_unlock");

						return (
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
										aria-label={unlockLabel}
										title={unlockLabel}
										disabled={isUnlocking}
									>
										<Icon
											name={isUnlocking ? "Spinner" : "Trash"}
											className={`size-3.5 ${isUnlocking ? "animate-spin" : ""}`}
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
				title={`Force unlock "${unlockPath}"?`}
				description={t("force_unlock_desc")}
				confirmLabel={t("core:confirm")}
				variant="destructive"
			/>
		</AdminLayout>
	);
}
