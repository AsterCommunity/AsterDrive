import { AdminFilterToolbar } from "@/components/common/AdminFilterToolbar";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";

interface AdminTaskFiltersToolbarProps {
	activeFilterCount: number;
	kindFilter: string;
	kindOptions: ReadonlyArray<{ label: string; value: string }>;
	onKindChange: (value: string | null) => void;
	onResetFilters: () => void;
	onStatusChange: (value: string | null) => void;
	statusFilter: string;
	statusOptions: ReadonlyArray<{ label: string; value: string }>;
}

export function AdminTaskFiltersToolbar({
	activeFilterCount,
	kindFilter,
	kindOptions,
	onKindChange,
	onResetFilters,
	onStatusChange,
	statusFilter,
	statusOptions,
}: AdminTaskFiltersToolbarProps) {
	return (
		<AdminFilterToolbar
			activeFilterCount={activeFilterCount}
			inline
			onResetFilters={onResetFilters}
		>
			<Select
				items={kindOptions}
				value={kindFilter}
				onValueChange={onKindChange}
			>
				<SelectTrigger width="compact">
					<SelectValue />
				</SelectTrigger>
				<SelectContent>
					{kindOptions.map((option) => (
						<SelectItem key={option.value} value={option.value}>
							{option.label}
						</SelectItem>
					))}
				</SelectContent>
			</Select>
			<Select
				items={statusOptions}
				value={statusFilter}
				onValueChange={onStatusChange}
			>
				<SelectTrigger width="compact">
					<SelectValue />
				</SelectTrigger>
				<SelectContent>
					{statusOptions.map((option) => (
						<SelectItem key={option.value} value={option.value}>
							{option.label}
						</SelectItem>
					))}
				</SelectContent>
			</Select>
		</AdminFilterToolbar>
	);
}
