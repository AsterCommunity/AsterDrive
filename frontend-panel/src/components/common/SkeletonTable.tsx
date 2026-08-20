import { Skeleton } from "@/components/ui/skeleton";
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from "@/components/ui/table";

interface SkeletonTableProps {
	columns?: number;
	rows?: number;
	/** D9 去框化页面：骨架与加载后的 frameless 表格保持同一形态 */
	frameless?: boolean;
}

export function SkeletonTable({
	columns = 4,
	rows = 8,
	frameless = false,
}: SkeletonTableProps) {
	return (
		<Table frameless={frameless}>
			<TableHeader>
				<TableRow>
					{Array.from({ length: columns }).map((_, i) => (
						<TableHead
							// biome-ignore lint/suspicious/noArrayIndexKey: static skeleton placeholders never reorder
							key={`skeleton-head-${i}`}
							className={frameless ? "bg-muted/35" : undefined}
						>
							<Skeleton
								className="h-4"
								style={{ width: `${50 + (i % 4) * 15}%` }}
							/>
						</TableHead>
					))}
				</TableRow>
			</TableHeader>
			<TableBody>
				{Array.from({ length: rows }).map((_, rowIdx) => (
					// biome-ignore lint/suspicious/noArrayIndexKey: static skeleton placeholders never reorder
					<TableRow key={`skeleton-row-${rowIdx}`}>
						{Array.from({ length: columns }).map((_, colIdx) => (
							// biome-ignore lint/suspicious/noArrayIndexKey: static skeleton placeholders never reorder
							<TableCell key={`skeleton-cell-${colIdx}`}>
								<Skeleton
									className="h-4"
									style={{
										width: `${60 + ((rowIdx + colIdx) % 4) * 10}%`,
									}}
								/>
							</TableCell>
						))}
					</TableRow>
				))}
			</TableBody>
		</Table>
	);
}
