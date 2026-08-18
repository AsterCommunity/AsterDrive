import { Skeleton } from "@/components/ui/skeleton";

interface SkeletonFileGridProps {
	count?: number;
}

export function SkeletonFileGrid({ count = 12 }: SkeletonFileGridProps) {
	return (
		<div className="space-y-4 px-4 py-3 md:p-5">
			<div className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
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
