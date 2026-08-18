import { GRID_GAP, GRID_ITEM_MAX_WIDTH } from "@/components/files/gridLayout";
import { Skeleton } from "@/components/ui/skeleton";

interface SkeletonFileGridProps {
	count?: number;
}

// FileGrid 的列数由 JS 按容器宽度接管（卡片到上限就裂列，ceil 语义）；
// 骨架屏是纯展示组件没有测宽，用静态 auto-fill（floor 语义）近似——
// 列数与真实网格最多差一列，加载态可接受。
const SKELETON_GRID_STYLE = {
	gridTemplateColumns: `repeat(auto-fill, minmax(min(${GRID_ITEM_MAX_WIDTH}px, 100%), 1fr))`,
	gap: GRID_GAP,
};

export function SkeletonFileGrid({ count = 12 }: SkeletonFileGridProps) {
	return (
		<div className="space-y-4 px-4 py-3 md:p-5">
			<div className="grid" style={SKELETON_GRID_STYLE}>
				{Array.from({ length: count }).map((_, i) => (
					<div
						// biome-ignore lint/suspicious/noArrayIndexKey: static skeleton placeholders never reorder
						key={`skeleton-card-${i}`}
						className="flex min-h-[166px] flex-col rounded-xl px-2.5 py-2.5"
					>
						<Skeleton className="mb-2 h-20 w-full rounded-xl" />
						<Skeleton className="mb-1 h-4 w-3/4" />
						<Skeleton className="mb-2 h-3 w-1/2" />
						<Skeleton className="h-4 w-20 rounded-full" />
					</div>
				))}
			</div>
		</div>
	);
}
