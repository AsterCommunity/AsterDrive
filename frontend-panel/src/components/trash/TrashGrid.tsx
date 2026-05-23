import { useTranslation } from "react-i18next";
import { FileTypeIcon } from "@/components/files/FileTypeIcon";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardFooter } from "@/components/ui/card";
import { Icon } from "@/components/ui/icon";
import { ItemCheckbox } from "@/components/ui/item-checkbox";
import {
	formatBytes,
	formatDateTimeWithOffset,
	formatDateUntil,
} from "@/lib/format";
import { cn } from "@/lib/utils";
import type { TrashItem } from "@/types/api-helpers";

interface TrashGridProps {
	items: TrashItem[];
	actionsDisabled?: boolean;
	pendingKeys?: Set<string>;
	pendingOperation?: "restore" | "purge" | "purge-all" | null;
	selectedKeys: Set<string>;
	onToggleSelect: (item: TrashItem) => void;
	onRestore: (item: TrashItem) => void;
	onPurge: (item: TrashItem) => void;
}

function getItemKey(item: TrashItem) {
	return `${item.entity_type}:${item.id}`;
}

export function TrashGrid({
	items,
	actionsDisabled = false,
	pendingKeys = new Set<string>(),
	pendingOperation = null,
	selectedKeys,
	onToggleSelect,
	onRestore,
	onPurge,
}: TrashGridProps) {
	const { t, i18n } = useTranslation(["core", "files", "admin"]);

	return (
		<div className="grid grid-cols-1 gap-3 p-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
			{items.map((item) => {
				const itemKey = getItemKey(item);
				const selected = selectedKeys.has(itemKey);
				const itemPending = pendingKeys.has(itemKey);
				const restorePending = itemPending && pendingOperation === "restore";
				const purgePending = itemPending && pendingOperation === "purge";
				const restoreLabel = restorePending
					? t("files:trash_restoring")
					: t("admin:restore");
				const purgeLabel = purgePending
					? t("files:trash_purging")
					: t("files:trash_delete_permanently");
				const originalPath =
					item.original_path === "/" ? t("files:root") : item.original_path;

				return (
					<Card
						key={itemKey}
						size="sm"
						role="button"
						tabIndex={0}
						onClick={() => {
							if (!itemPending) onToggleSelect(item);
						}}
						onKeyDown={(e) => {
							if (e.key === "Enter" || e.key === " ") {
								e.preventDefault();
								if (!itemPending) onToggleSelect(item);
							}
						}}
						className={cn(
							"cursor-pointer border transition-colors hover:bg-accent/30",
							selected && "bg-accent/40 ring-2 ring-primary/40",
							itemPending && "opacity-75",
						)}
					>
						<CardContent className="space-y-3">
							<div className="flex items-start justify-between gap-3">
								<div className="flex items-center gap-3">
									<div className="flex size-11 items-center justify-center rounded-xl bg-muted/70">
										{item.entity_type === "folder" ? (
											<Icon name="Folder" className="size-6 text-amber-500" />
										) : (
											<FileTypeIcon
												mimeType={item.mime_type}
												fileName={item.name}
												className="size-6"
											/>
										)}
									</div>
									<div className="min-w-0">
										<p className="line-clamp-2 font-medium" title={item.name}>
											{item.name}
										</p>
										<p
											className="truncate text-xs text-muted-foreground"
											title={originalPath}
										>
											{originalPath}
										</p>
									</div>
								</div>
								<ItemCheckbox
									checked={selected}
									onChange={() => {
										if (!itemPending) onToggleSelect(item);
									}}
								/>
							</div>

							<div className="grid grid-cols-2 gap-3 text-xs text-muted-foreground">
								<div className="space-y-1">
									<p>{t("type")}</p>
									<p className="font-medium text-foreground">
										{item.entity_type === "folder" ? t("folder") : t("file")}
									</p>
								</div>
								<div className="space-y-1">
									<p>{t("files:trash_expires_at")}</p>
									<p
										className="font-medium text-foreground"
										title={formatDateTimeWithOffset(item.expires_at)}
									>
										{formatDateUntil(item.expires_at, i18n)}
									</p>
								</div>
								<div className="space-y-1">
									<p>{t("size")}</p>
									<p className="font-medium text-foreground">
										{item.entity_type === "file" ? formatBytes(item.size) : "—"}
									</p>
								</div>
							</div>
						</CardContent>

						<CardFooter className="gap-2">
							<Button
								size="sm"
								variant="outline"
								className="flex-1"
								disabled={actionsDisabled || itemPending}
								onClick={(e) => {
									e.stopPropagation();
									onRestore(item);
								}}
							>
								<Icon
									name={restorePending ? "Spinner" : "ArrowCounterClockwise"}
									className={`mr-1 size-3.5 ${restorePending ? "animate-spin" : ""}`}
								/>
								{restoreLabel}
							</Button>
							<Button
								size="sm"
								variant="destructive"
								className="flex-1"
								disabled={actionsDisabled || itemPending}
								onClick={(e) => {
									e.stopPropagation();
									onPurge(item);
								}}
							>
								<Icon
									name={purgePending ? "Spinner" : "Trash"}
									className={`mr-1 size-3.5 ${purgePending ? "animate-spin" : ""}`}
								/>
								{purgeLabel}
							</Button>
						</CardFooter>
					</Card>
				);
			})}
		</div>
	);
}
