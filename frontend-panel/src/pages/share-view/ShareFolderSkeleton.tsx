import { SkeletonFileGrid } from "@/components/common/SkeletonFileGrid";
import { Skeleton } from "@/components/ui/skeleton";
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from "@/components/ui/table";
import { SharePageShell } from "./ShareViewShell";

const SHARE_TABLE_SKELETON_WIDTHS = [
	"68%",
	"52%",
	"60%",
	"48%",
	"64%",
	"56%",
	"50%",
	"62%",
];

function ShareToolbarSkeleton() {
	return (
		<div className="border-b border-border/60 bg-card/90 px-3 py-2 shadow-xs dark:bg-card/70 dark:shadow-none sm:px-4 sm:py-2.5">
			<div className="flex h-9 min-w-0 items-center gap-1.5 rounded-lg bg-background/70 px-2.5 shadow-xs ring-1 ring-border/55 dark:bg-background/25 dark:shadow-none dark:ring-border/60 sm:h-10 sm:gap-2 sm:px-3">
				<Skeleton className="size-7 shrink-0 rounded-lg sm:size-8" />
				<div className="flex min-w-0 flex-1 items-center gap-1.5 sm:gap-2">
					<Skeleton className="h-4 w-28 max-w-[35%]" />
					<Skeleton className="hidden h-4 w-20 sm:block" />
				</div>
				<div className="flex shrink-0 items-center gap-1 border-l border-border/55 pl-1.5 sm:gap-2 sm:pl-2">
					<Skeleton className="size-7 rounded-lg sm:size-8" />
					<div className="flex rounded-lg bg-muted/35 p-0.5 ring-1 ring-border/45 dark:bg-muted/25 dark:ring-border/55">
						<Skeleton className="size-7 rounded-sm sm:size-8" />
						<Skeleton className="size-7 rounded-sm sm:size-8" />
					</div>
				</div>
			</div>
		</div>
	);
}

function ShareFolderTableSkeleton() {
	return (
		<Table>
			<TableHeader>
				<TableRow>
					<TableHead>
						<Skeleton className="h-4 w-24" />
					</TableHead>
					<TableHead className="w-[100px]">
						<Skeleton className="h-4 w-14" />
					</TableHead>
					<TableHead>
						<Skeleton className="h-4 w-20" />
					</TableHead>
					<TableHead className="w-12" />
				</TableRow>
			</TableHeader>
			<TableBody>
				{SHARE_TABLE_SKELETON_WIDTHS.map((width) => (
					<TableRow key={`share-row-${width}`}>
						<TableCell>
							<div className="flex items-center gap-3">
								<Skeleton className="size-6 shrink-0 rounded-md" />
								<Skeleton className="h-4" style={{ width }} />
							</div>
						</TableCell>
						<TableCell>
							<Skeleton className="h-4 w-14" />
						</TableCell>
						<TableCell>
							<Skeleton className="h-4 w-20" />
						</TableCell>
						<TableCell className="w-12" />
					</TableRow>
				))}
			</TableBody>
		</Table>
	);
}

export function ShareLoadingSkeleton() {
	return (
		<SharePageShell>
			<main className="flex min-h-0 flex-1 flex-col overflow-hidden">
				<ShareToolbarSkeleton />
				<section className="min-h-0 flex-1 overflow-auto">
					<ShareFolderContentSkeleton viewMode="grid" />
				</section>
			</main>
		</SharePageShell>
	);
}

export function ShareFolderContentSkeleton({
	viewMode,
}: {
	viewMode: "grid" | "list";
}) {
	return viewMode === "grid" ? (
		<SkeletonFileGrid />
	) : (
		<ShareFolderTableSkeleton />
	);
}
