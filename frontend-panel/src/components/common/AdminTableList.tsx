import type { ReactNode } from "react";
import {
	AdminTable,
	AdminTableBody,
	AdminTableShell,
} from "@/components/common/AdminTable";
import { EmptyState } from "@/components/common/EmptyState";
import { SkeletonTable } from "@/components/common/SkeletonTable";

interface AdminTableListProps<T> {
	loading: boolean;
	items: T[];
	columns: number;
	rows?: number;
	emptyIcon?: ReactNode;
	emptyTitle: string;
	emptyDescription?: string;
	headerRow: ReactNode;
	renderRow: (item: T) => ReactNode;
}

export function AdminTableList<T>({
	loading,
	items,
	columns,
	rows,
	emptyIcon,
	emptyTitle,
	emptyDescription,
	headerRow,
	renderRow,
}: AdminTableListProps<T>) {
	if (loading) {
		return <SkeletonTable columns={columns} rows={rows ?? 5} />;
	}

	if (items.length === 0) {
		return (
			<EmptyState
				icon={emptyIcon}
				title={emptyTitle}
				description={emptyDescription}
			/>
		);
	}

	return (
		<AdminTableShell>
			<AdminTable>
				{headerRow}
				<AdminTableBody>{items.map(renderRow)}</AdminTableBody>
			</AdminTable>
		</AdminTableShell>
	);
}
