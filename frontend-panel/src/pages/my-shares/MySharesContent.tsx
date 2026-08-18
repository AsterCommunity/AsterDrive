import { EmptyState } from "@/components/common/EmptyState";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import type { MyShareInfo } from "@/types/api";
import { MyShareCard } from "./MyShareCard";

interface MySharesContentLabels {
	active: string;
	copy: string;
	created: (date: string) => string;
	delete: string;
	deleted: string;
	edit: string;
	emptyDescription: string;
	emptyTitle: string;
	exhausted: string;
	expire: (date: string) => string;
	expired: string;
	never: string;
	next: string;
	open: string;
	pageDescription: string;
	prev: string;
}

interface MySharesContentProps {
	labels: MySharesContentLabels;
	loading: boolean;
	onCopy: (share: MyShareInfo) => void;
	onDelete: (share: MyShareInfo) => void;
	onEdit: (share: MyShareInfo) => void;
	onNextPage: () => void;
	onOpen: (share: MyShareInfo) => void;
	onPrevPage: () => void;
	onToggleSelect: (shareId: number) => void;
	page: number;
	selectedShareIds: Set<number>;
	shares: MyShareInfo[];
	totalPages: number;
}

export function MySharesContent({
	labels,
	loading,
	onCopy,
	onDelete,
	onEdit,
	onNextPage,
	onOpen,
	onPrevPage,
	onToggleSelect,
	page,
	selectedShareIds,
	shares,
	totalPages,
}: MySharesContentProps) {
	if (loading) {
		return (
			<div className="flex flex-col gap-2">
				{["s1", "s2", "s3", "s4", "s5", "s6"].map((key) => (
					<div
						key={key}
						className="h-16 animate-pulse rounded-xl bg-muted/30"
					/>
				))}
			</div>
		);
	}

	if (shares.length === 0) {
		return (
			<div className="py-12">
				<EmptyState
					icon={<Icon name="Link" className="size-10" />}
					title={labels.emptyTitle}
					description={labels.emptyDescription}
				/>
			</div>
		);
	}

	return (
		<>
			<div className="flex flex-col gap-2">
				{shares.map((share) => (
					<MyShareCard
						key={share.id}
						share={share}
						selected={selectedShareIds.has(share.id)}
						labels={labels}
						onCopy={onCopy}
						onDelete={onDelete}
						onEdit={onEdit}
						onOpen={onOpen}
						onToggleSelect={onToggleSelect}
					/>
				))}
			</div>

			{/* D9：分页条去框，与 trash 计数条同为裸工具行 */}
			<div className="flex items-center justify-between px-1 py-2">
				<p className="text-sm text-muted-foreground">
					{labels.pageDescription}
				</p>
				<div className="flex items-center gap-2">
					<Button
						variant="outline"
						size="sm"
						disabled={page === 0}
						onClick={onPrevPage}
					>
						{labels.prev}
					</Button>
					<Button
						variant="outline"
						size="sm"
						disabled={page + 1 >= totalPages}
						onClick={onNextPage}
					>
						{labels.next}
					</Button>
				</div>
			</div>
		</>
	);
}
