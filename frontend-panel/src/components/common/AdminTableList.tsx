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
}: AdminTableListProps<T>) {
	return (
		<div className={cn("flex min-h-0 flex-col gap-3", className)}>
			{toolbar ? (
				<AdminSurface padded={false} className="flex-none rounded-lg px-3 py-2">
					<div className="flex flex-wrap items-center gap-2">{toolbar}</div>
				</AdminSurface>
			) : null}
			{loading ? (
				<AdminTableShell>
					<SkeletonTable columns={columns} rows={rows ?? 5} />
				</AdminTableShell>
			) : items.length === 0 ? (
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
