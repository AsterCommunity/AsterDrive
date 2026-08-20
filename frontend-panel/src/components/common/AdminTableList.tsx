import type { ReactNode } from "react";
import {
	AdminTable,
	AdminTableBody,
	AdminTableShell,
} from "@/components/common/AdminTable";
import { EmptyState } from "@/components/common/EmptyState";
import { SkeletonTable } from "@/components/common/SkeletonTable";
import { AdminSurface } from "@/components/layout/AdminSurface";
import { cn } from "@/lib/utils";

interface AdminTableListProps<T> {
	loading: boolean;
	items: T[];
	columns: number;
	rows?: number;
	emptyIcon?: ReactNode;
	emptyTitle: string;
	emptyDescription?: string;
	emptyAction?: ReactNode;
	filtered?: boolean;
	filteredEmptyTitle?: string;
	filteredEmptyDescription?: string;
	filteredEmptyAction?: ReactNode;
	headerRow: ReactNode;
	pagination?: ReactNode;
	renderRow: (item: T) => ReactNode;
	toolbar?: ReactNode;
	className?: string;
	/** D9 用户页去框化：toolbar/空态不用 AdminSurface 带框容器，直接坐页面背景。
	    后台页面不传，行为不变 */
	frameless?: boolean;
}

export function AdminTableList<T>({
	loading,
	items,
	columns,
	rows,
	emptyIcon,
	emptyTitle,
	emptyDescription,
	emptyAction,
	filtered = false,
	filteredEmptyTitle,
	filteredEmptyDescription,
	filteredEmptyAction,
	headerRow,
	pagination,
	renderRow,
	toolbar,
	className,
	frameless = false,
}: AdminTableListProps<T>) {
	return (
		<div className={cn("flex min-h-0 flex-col gap-3", className)}>
			{toolbar ? (
				frameless ? (
					<div className="flex flex-wrap items-center gap-2 px-1 py-1">
						{toolbar}
					</div>
				) : (
					<AdminSurface
						padded={false}
						className="flex-none rounded-lg px-3 py-2"
					>
						<div className="flex flex-wrap items-center gap-2">{toolbar}</div>
					</AdminSurface>
				)
			) : null}
			{loading ? (
				frameless ? (
					<div className="min-h-0">
						<SkeletonTable frameless columns={columns} rows={rows ?? 5} />
					</div>
				) : (
					<AdminTableShell>
						<SkeletonTable columns={columns} rows={rows ?? 5} />
					</AdminTableShell>
				)
			) : items.length === 0 ? (
				frameless ? (
					<div className="py-12">
						<EmptyState
							icon={emptyIcon}
							title={filtered ? (filteredEmptyTitle ?? emptyTitle) : emptyTitle}
							description={
								filtered
									? (filteredEmptyDescription ?? emptyDescription)
									: emptyDescription
							}
							action={
								filtered ? (filteredEmptyAction ?? emptyAction) : emptyAction
							}
						/>
					</div>
				) : (
					<AdminSurface padded={false} className="rounded-lg">
						<EmptyState
							icon={emptyIcon}
							title={filtered ? (filteredEmptyTitle ?? emptyTitle) : emptyTitle}
							description={
								filtered
									? (filteredEmptyDescription ?? emptyDescription)
									: emptyDescription
							}
							action={
								filtered ? (filteredEmptyAction ?? emptyAction) : emptyAction
							}
						/>
					</AdminSurface>
				)
			) : frameless ? (
				<AdminTable frameless>
					{headerRow}
					<AdminTableBody>{items.map(renderRow)}</AdminTableBody>
				</AdminTable>
			) : (
				<AdminTableShell>
					<AdminTable>
						{headerRow}
						<AdminTableBody>{items.map(renderRow)}</AdminTableBody>
					</AdminTable>
				</AdminTableShell>
			)}
			{pagination ? <div className="flex-none">{pagination}</div> : null}
		</div>
	);
}
