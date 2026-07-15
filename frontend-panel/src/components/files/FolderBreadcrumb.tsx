import { type DragEvent, Fragment } from "react";
import { useTranslation } from "react-i18next";
import {
	Breadcrumb,
	BreadcrumbEllipsis,
	BreadcrumbItem,
	BreadcrumbLink,
	BreadcrumbList,
	BreadcrumbPage,
	BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Icon } from "@/components/ui/icon";

export interface FolderBreadcrumbItem {
	id: number | null;
	name: string;
}

type VisibleBreadcrumbEntry =
	| {
			type: "item";
			item: FolderBreadcrumbItem;
			sourceIndex: number;
	  }
	| {
			type: "ellipsis";
			key: string;
			items: Array<{
				item: FolderBreadcrumbItem;
				sourceIndex: number;
			}>;
	  };

interface FolderBreadcrumbProps {
	items: FolderBreadcrumbItem[];
	compact?: boolean;
	dragOverIndex?: number | null;
	onDragLeave?: (event: DragEvent) => void;
	onDragOver?: (event: DragEvent, index: number) => void;
	onDrop?: (
		event: DragEvent,
		index: number,
		targetFolderId: number | null,
	) => Promise<void>;
	onNavigate: (folderId: number | null, folderName: string) => void;
}

export function FolderBreadcrumb({
	items,
	compact = false,
	dragOverIndex = null,
	onDragLeave,
	onDragOver,
	onDrop,
	onNavigate,
}: FolderBreadcrumbProps) {
	const { t } = useTranslation();
	const visibleItems: VisibleBreadcrumbEntry[] =
		compact && items.length > 2
			? [
					{ type: "item", item: items[0], sourceIndex: 0 },
					{
						type: "ellipsis",
						key: "ellipsis",
						items: items.slice(1, -1).map((item, index) => ({
							item,
							sourceIndex: index + 1,
						})),
					},
					{
						type: "item",
						item: items[items.length - 1],
						sourceIndex: items.length - 1,
					},
				]
			: items.map((item, index) => ({
					type: "item" as const,
					item,
					sourceIndex: index,
				}));

	return (
		<Breadcrumb className="min-w-0">
			<BreadcrumbList className="min-w-0 gap-1.5 text-xs sm:gap-2 sm:text-sm">
				{visibleItems.map((entry, index) => (
					<Fragment
						key={
							entry.type === "ellipsis"
								? entry.key
								: `${entry.item.id ?? "root"}-${entry.sourceIndex}`
						}
					>
						{index > 0 ? (
							<BreadcrumbSeparator className="mx-0.5 text-muted-foreground/45" />
						) : null}
						{entry.type === "ellipsis" ? (
							<BreadcrumbItem className="shrink-0">
								<DropdownMenu>
									<DropdownMenuTrigger
										render={
											<button
												type="button"
												className="flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent/70 hover:text-foreground sm:h-7 sm:w-7"
												aria-label={t("core:more")}
											>
												<BreadcrumbEllipsis />
											</button>
										}
									/>
									<DropdownMenuContent
										align="start"
										className="w-auto min-w-40"
									>
										{entry.items.map(({ item: hiddenItem, sourceIndex }) => (
											<DropdownMenuItem
												key={`${hiddenItem.id ?? "root"}-${sourceIndex}`}
												onDragOver={
													onDragOver
														? (event) => onDragOver(event, sourceIndex)
														: undefined
												}
												onDragLeave={onDragLeave}
												onDrop={
													onDrop
														? (event) => {
																void onDrop(event, sourceIndex, hiddenItem.id);
															}
														: undefined
												}
												onClick={() =>
													onNavigate(hiddenItem.id, hiddenItem.name)
												}
											>
												<Icon
													name="FolderOpen"
													className="size-4 text-muted-foreground"
												/>
												<span className="truncate">{hiddenItem.name}</span>
											</DropdownMenuItem>
										))}
									</DropdownMenuContent>
								</DropdownMenu>
							</BreadcrumbItem>
						) : (
							<BreadcrumbItem
								className={
									entry.sourceIndex === items.length - 1
										? "min-w-0 flex-1"
										: "shrink-0"
								}
							>
								{entry.sourceIndex < items.length - 1 ? (
									<BreadcrumbLink
										className={[
											"cursor-pointer rounded-md px-1 py-0.5 text-[13px] text-muted-foreground transition-colors hover:bg-accent/45 hover:text-foreground sm:px-1.5 sm:text-sm",
											dragOverIndex === entry.sourceIndex &&
												"ring-2 ring-primary bg-accent/30 text-foreground",
										]
											.filter(Boolean)
											.join(" ")}
										onDragOver={
											onDragOver
												? (event) => onDragOver(event, entry.sourceIndex)
												: undefined
										}
										onDragLeave={onDragLeave}
										onDrop={
											onDrop
												? (event) => {
														void onDrop(
															event,
															entry.sourceIndex,
															entry.item.id,
														);
													}
												: undefined
										}
										onClick={() => onNavigate(entry.item.id, entry.item.name)}
									>
										{entry.item.name}
									</BreadcrumbLink>
								) : (
									<BreadcrumbPage className="truncate text-sm font-semibold text-foreground sm:text-[0.95rem]">
										{entry.item.name}
									</BreadcrumbPage>
								)}
							</BreadcrumbItem>
						)}
					</Fragment>
				))}
			</BreadcrumbList>
		</Breadcrumb>
	);
}
