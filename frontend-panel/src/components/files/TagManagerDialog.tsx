import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
	FixedDialogFooter,
	ManagerDialogScrollableList,
	ManagerDialogShell,
} from "@/components/common/ManagerDialogShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { handleApiError } from "@/hooks/useApiError";
import { cn } from "@/lib/utils";
import { tagService } from "@/services/tagService";
import type { EntityType, TagInfo, TagSummary } from "@/types/api";
import { TagChip } from "./TagChips";
import { TagLibraryManagerDialog } from "./TagLibraryManagerDialog";
import { safeTagColor, tagColorFromName } from "./tagColors";

const TAG_LIBRARY_LIMIT = 100;

type BatchTagAction = "add" | "remove" | "none";

export type TagManagerTarget =
	| {
			mode: "entity";
			entityId: number;
			entityType: EntityType;
			initialTags: TagSummary[];
			name?: string;
			onChanged?: () => void | Promise<void>;
			onTagsChange?: (tags: TagSummary[]) => void;
	  }
	| {
			mode: "batch";
			count: number;
			fileIds: number[];
			folderIds: number[];
			onChanged?: () => void | Promise<void>;
	  };

function toSummary(tag: TagInfo): TagSummary {
	return {
		id: tag.id,
		name: tag.name,
		color: tag.color,
	};
}

function sortTags<T extends TagSummary>(tags: T[]): T[] {
	return [...tags].sort((a, b) =>
		a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
	);
}

function tagRequestBody(target: Extract<TagManagerTarget, { mode: "batch" }>) {
	return {
		file_ids: target.fileIds,
		folder_ids: target.folderIds,
	};
}

function idsEqual(a: Set<number>, b: Set<number>) {
	if (a.size !== b.size) return false;
	for (const id of a) {
		if (!b.has(id)) return false;
	}
	return true;
}

function normalizeQuery(value: string) {
	return value.trim().toLocaleLowerCase();
}

function canCreateTag(tags: TagInfo[], name: string) {
	const normalized = normalizeQuery(name);
	if (!normalized) return false;
	return !tags.some(
		(tag) => tag.name.trim().toLocaleLowerCase() === normalized,
	);
}

export function TagManagerDialog({
	open,
	onOpenChange,
	target,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	target: TagManagerTarget | null;
}) {
	const { t } = useTranslation(["files", "core"]);
	const [tags, setTags] = useState<TagInfo[]>([]);
	const [tagsTotal, setTagsTotal] = useState(0);
	const [entityTags, setEntityTags] = useState<TagSummary[]>([]);
	const [batchActions, setBatchActions] = useState<Map<number, BatchTagAction>>(
		() => new Map(),
	);
	const [query, setQuery] = useState("");
	const [loading, setLoading] = useState(false);
	const [loadingMoreTags, setLoadingMoreTags] = useState(false);
	const [busyKey, setBusyKey] = useState<string | null>(null);
	const [libraryManagerOpen, setLibraryManagerOpen] = useState(false);

	useEffect(() => {
		if (!open || !target) {
			setTags([]);
			setTagsTotal(0);
			setEntityTags([]);
			setBatchActions(new Map());
			setQuery("");
			setLoading(false);
			setLoadingMoreTags(false);
			setBusyKey(null);
			setLibraryManagerOpen(false);
			return;
		}

		setTagsTotal(0);
		setEntityTags(target.mode === "entity" ? sortTags(target.initialTags) : []);
		setBatchActions(new Map());
		setQuery("");
	}, [open, target]);

	useEffect(() => {
		if (!open || !target) {
			return;
		}

		let cancelled = false;
		const searchTerm = query.trim();
		setLoading(true);
		setTags([]);
		setTagsTotal(0);
		tagService
			.listTags({
				params: {
					limit: TAG_LIBRARY_LIMIT,
					offset: 0,
					...(searchTerm ? { q: searchTerm } : {}),
				},
			})
			.then((page) => {
				if (!cancelled) {
					setTags(sortTags(page.items));
					setTagsTotal(page.total);
				}
			})
			.catch((err) => {
				if (!cancelled) handleApiError(err);
			})
			.finally(() => {
				if (!cancelled) setLoading(false);
			});

		return () => {
			cancelled = true;
		};
	}, [open, query, target]);

	const hasMoreTags = tags.length < tagsTotal;
	const normalizedQuery = normalizeQuery(query);
	const selectedIds = useMemo(
		() => new Set(entityTags.map((tag) => tag.id)),
		[entityTags],
	);
	const initialSelectedIds = useMemo(
		() =>
			new Set(
				target?.mode === "entity"
					? target.initialTags.map((tag) => tag.id)
					: [],
			),
		[target],
	);
	const filteredTags = tags;
	const pendingAddIds = useMemo(
		() =>
			new Set(
				[...batchActions.entries()]
					.filter(([, action]) => action === "add")
					.map(([tagId]) => tagId),
			),
		[batchActions],
	);
	const pendingRemoveIds = useMemo(
		() =>
			new Set(
				[...batchActions.entries()]
					.filter(([, action]) => action === "remove")
					.map(([tagId]) => tagId),
			),
		[batchActions],
	);
	const pendingAddTags = useMemo(
		() => tags.filter((tag) => pendingAddIds.has(tag.id)),
		[pendingAddIds, tags],
	);
	const pendingRemoveTags = useMemo(
		() => tags.filter((tag) => pendingRemoveIds.has(tag.id)),
		[pendingRemoveIds, tags],
	);
	const entityChanged =
		target?.mode === "entity" && !idsEqual(initialSelectedIds, selectedIds);
	const batchChanged = target?.mode === "batch" && batchActions.size > 0;
	const hasDraftChanges = entityChanged || batchChanged;
	const canCreate = !loading && canCreateTag(tags, query);
	const createPreviewColor = tagColorFromName(query);
	const saving = busyKey === "save";
	const creating = busyKey === "create";

	const loadMoreTags = useCallback(async () => {
		if (loadingMoreTags || !hasMoreTags) return;

		setLoadingMoreTags(true);
		try {
			const page = await tagService.listTags({
				params: {
					limit: TAG_LIBRARY_LIMIT,
					offset: tags.length,
					...(query.trim() ? { q: query.trim() } : {}),
				},
			});
			setTags((current) => sortTags([...current, ...page.items]));
			setTagsTotal(page.total);
		} catch (err) {
			handleApiError(err);
		} finally {
			setLoadingMoreTags(false);
		}
	}, [hasMoreTags, loadingMoreTags, query, tags.length]);

	const toggleEntityTag = (tag: TagInfo) => {
		if (target?.mode !== "entity") return;

		if (selectedIds.has(tag.id)) {
			setEntityTags((current) =>
				current.filter((currentTag) => currentTag.id !== tag.id),
			);
			return;
		}

		setEntityTags((current) => sortTags([...current, toSummary(tag)]));
	};

	const setBatchAction = (tagId: number, action: BatchTagAction) => {
		if (target?.mode !== "batch") return;
		setBatchActions((current) => {
			const next = new Map(current);
			if (action === "none") next.delete(tagId);
			else next.set(tagId, action);
			return next;
		});
	};

	const handleLibraryTagUpdated = (tag: TagInfo) => {
		const summary = toSummary(tag);
		setTags((current) =>
			sortTags(
				current.some((currentTag) => currentTag.id === tag.id)
					? current.map((currentTag) =>
							currentTag.id === tag.id ? tag : currentTag,
						)
					: [...current, tag],
			),
		);
		setEntityTags((current) =>
			sortTags(
				current.map((currentTag) =>
					currentTag.id === tag.id ? summary : currentTag,
				),
			),
		);
	};

	const handleLibraryTagDeleted = (tagId: number) => {
		setTags((current) => current.filter((tag) => tag.id !== tagId));
		setTagsTotal((current) => Math.max(0, current - 1));
		setEntityTags((current) => current.filter((tag) => tag.id !== tagId));
		setBatchActions((current) => {
			if (!current.has(tagId)) return current;
			const next = new Map(current);
			next.delete(tagId);
			return next;
		});
	};

	const handleCreateTag = async () => {
		if (!target || !canCreate) return;

		const name = query.trim();
		setBusyKey("create");
		try {
			const created = await tagService.createTag({
				name,
				color: safeTagColor(createPreviewColor),
			});
			setTags((current) => sortTags([...current, created]));
			setTagsTotal((current) => current + 1);
			setQuery("");

			if (target.mode === "entity") {
				setEntityTags((current) => sortTags([...current, toSummary(created)]));
			} else if (target.mode === "batch") {
				setBatchActions((current) => {
					const next = new Map(current);
					next.set(created.id, "add");
					return next;
				});
			}
			toast.success(t("tag_created"));
		} catch (err) {
			handleApiError(err);
		} finally {
			setBusyKey(null);
		}
	};

	const handleSaveChanges = async () => {
		if (!target || !hasDraftChanges) return;

		setBusyKey("save");
		try {
			if (target.mode === "entity") {
				const nextTags = sortTags(entityTags);
				await tagService.replaceEntityTags(
					target.entityType,
					target.entityId,
					nextTags.map((tag) => tag.id),
				);
				target.onTagsChange?.(nextTags);
				await target.onChanged?.();
				toast.success(t("tag_saved"));
				onOpenChange(false);
				return;
			}

			if (target.mode === "batch") {
				const request = tagRequestBody(target);
				await Promise.all(
					[...pendingAddIds].map((tagId) =>
						tagService.batchAttachTag(tagId, request),
					),
				);
				await Promise.all(
					[...pendingRemoveIds].map((tagId) =>
						tagService.batchDetachTag(tagId, request),
					),
				);
				await target.onChanged?.();
				toast.success(t("tag_batch_saved"));
				onOpenChange(false);
			}
		} catch (err) {
			handleApiError(err);
		} finally {
			setBusyKey(null);
		}
	};

	const showSaveActions = target?.mode === "entity" || target?.mode === "batch";
	const title =
		target?.mode === "batch"
			? t("tag_manage_batch_title", { count: target.count })
			: t("tag_manage_title");
	const description =
		target?.mode === "batch"
			? t("tag_manage_batch_desc")
			: target?.name
				? target.name
				: t("tag_manage_desc");
	const draftSummary =
		target?.mode === "entity" && entityChanged
			? t("tag_draft_entity_summary")
			: target?.mode === "batch" && batchChanged
				? t("tag_draft_batch_summary", {
						add: pendingAddIds.size,
						remove: pendingRemoveIds.size,
					})
				: showSaveActions
					? t("tag_draft_empty")
					: null;
	const controls = (
		<div className="space-y-2">
			<label
				className="text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground"
				htmlFor="tag-manager-search"
			>
				{t("tag_search_label")}
			</label>
			<div className="relative">
				<Icon
					name="MagnifyingGlass"
					className="-translate-y-1/2 absolute top-1/2 left-2.5 size-4 text-muted-foreground"
				/>
				<Input
					id="tag-manager-search"
					value={query}
					onChange={(event) => setQuery(event.target.value)}
					placeholder={t("tag_search_placeholder")}
					className="pl-8"
					maxLength={64}
					onKeyDown={(event) => {
						if (event.key === "Enter" && canCreate) {
							event.preventDefault();
							void handleCreateTag();
						}
					}}
				/>
			</div>
		</div>
	);
	const footer = (
		<FixedDialogFooter>
			<div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
				{draftSummary ? (
					<p
						className={cn(
							"min-w-0 flex-1 text-sm",
							hasDraftChanges
								? "font-medium text-foreground"
								: "text-muted-foreground",
						)}
					>
						{draftSummary}
					</p>
				) : (
					<span className="min-w-0 flex-1" />
				)}
				<div className="flex w-full flex-col-reverse gap-2 sm:w-auto sm:flex-row">
					<Button
						type="button"
						variant="outline"
						className="w-full sm:w-auto"
						disabled={saving}
						onClick={() => onOpenChange(false)}
					>
						{showSaveActions ? t("core:cancel") : t("core:close")}
					</Button>
					{showSaveActions ? (
						<Button
							type="button"
							variant={hasDraftChanges || saving ? "default" : "outline"}
							className="w-full sm:w-auto"
							disabled={!hasDraftChanges || saving}
							onClick={() => {
								void handleSaveChanges();
							}}
						>
							{saving ? (
								<Icon name="Spinner" className="size-3.5 animate-spin" />
							) : null}
							{t("core:save")}
						</Button>
					) : null}
				</div>
			</div>
		</FixedDialogFooter>
	);

	return (
		<>
			<ManagerDialogShell
				open={open}
				onOpenChange={onOpenChange}
				title={title}
				description={description}
				controls={controls}
				footer={footer}
				className="sm:max-w-2xl"
			>
				<ManagerDialogScrollableList className="space-y-4">
					<div className="flex min-h-0 flex-1 flex-col gap-4">
						{target?.mode === "entity" ? (
							<section className="space-y-2">
								<h3 className="text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground">
									{t("tag_selected")}
								</h3>
								<div className="flex max-h-24 min-h-11 flex-wrap content-start gap-1.5 overflow-y-auto rounded-lg border border-border/70 bg-muted/25 p-3">
									{entityTags.length > 0 ? (
										entityTags.map((tag) => (
											<TagChip
												key={tag.id}
												tag={tag}
												removeLabel={t("tag_remove")}
												onRemove={() => {
													setEntityTags((current) =>
														current.filter(
															(currentTag) => currentTag.id !== tag.id,
														),
													);
												}}
											/>
										))
									) : (
										<span className="text-sm text-muted-foreground">
											{t("tag_no_tags")}
										</span>
									)}
								</div>
							</section>
						) : null}

						{target?.mode === "batch" && batchChanged ? (
							<section className="space-y-2 rounded-lg border border-border/70 bg-muted/20 p-2.5">
								{pendingAddTags.length > 0 ? (
									<div className="flex min-w-0 items-start gap-2">
										<div className="inline-flex h-5 shrink-0 items-center gap-1 rounded-md bg-primary/10 px-1.5 text-[11px] font-medium text-primary">
											<Icon name="Plus" className="size-3" />
											{t("tag_pending_add")}
										</div>
										<div className="flex min-w-0 flex-1 flex-wrap gap-1.5">
											{pendingAddTags.map((tag) => (
												<TagChip
													key={tag.id}
													tag={toSummary(tag)}
													removeLabel={t("tag_remove")}
													onRemove={() => setBatchAction(tag.id, "none")}
												/>
											))}
										</div>
									</div>
								) : null}
								{pendingRemoveTags.length > 0 ? (
									<div className="flex min-w-0 items-start gap-2">
										<div className="inline-flex h-5 shrink-0 items-center gap-1 rounded-md bg-destructive/10 px-1.5 text-[11px] font-medium text-destructive">
											<Icon name="Minus" className="size-3" />
											{t("tag_pending_remove")}
										</div>
										<div className="flex min-w-0 flex-1 flex-wrap gap-1.5">
											{pendingRemoveTags.map((tag) => (
												<TagChip
													key={tag.id}
													tag={toSummary(tag)}
													removeLabel={t("tag_remove")}
													onRemove={() => setBatchAction(tag.id, "none")}
												/>
											))}
										</div>
									</div>
								) : null}
							</section>
						) : null}

						<section className="flex min-h-0 flex-1 flex-col gap-2">
							<div className="flex items-center justify-between gap-3">
								<h3 className="text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground">
									{t("tag_library")}
								</h3>
								<Button
									type="button"
									size="sm"
									variant="ghost"
									onClick={() => setLibraryManagerOpen(true)}
								>
									<Icon name="PencilSimple" className="size-3.5" />
									{t("tag_library_manage")}
								</Button>
							</div>
							<div className="min-h-32 flex-1 space-y-1.5 overflow-y-auto pr-1">
								{canCreate ? (
									<button
										type="button"
										className="flex h-10 w-full min-w-0 items-center gap-2 rounded-lg border border-dashed border-primary/45 bg-primary/5 px-2.5 text-left text-sm transition-colors hover:bg-primary/10"
										disabled={creating}
										onClick={() => {
											void handleCreateTag();
										}}
									>
										<span
											className="size-2.5 shrink-0 rounded-full ring-1 ring-black/10"
											style={{ backgroundColor: createPreviewColor }}
											aria-hidden
										/>
										<span className="min-w-0 flex-1 truncate">
											{t("tag_create_named", { name: query.trim() })}
										</span>
										{creating ? (
											<Icon
												name="Spinner"
												className="size-4 shrink-0 animate-spin"
											/>
										) : (
											<Icon name="Plus" className="size-4 shrink-0" />
										)}
									</button>
								) : null}

								{loading ? (
									<p className="rounded-lg border border-border/70 bg-muted/25 p-3 text-sm text-muted-foreground">
										{t("info_loading")}
									</p>
								) : filteredTags.length > 0 ? (
									filteredTags.map((tag) => {
										const selected = selectedIds.has(tag.id);
										const batchAction = batchActions.get(tag.id) ?? "none";

										if (target?.mode === "entity") {
											return (
												<button
													key={tag.id}
													type="button"
													className={cn(
														"flex min-h-10 w-full min-w-0 items-center gap-2 rounded-lg border border-border/70 bg-card/60 px-2.5 py-2 text-left transition-colors hover:bg-accent/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/35",
														selected && "border-primary/45 bg-primary/5",
													)}
													aria-pressed={selected}
													onClick={() => toggleEntityTag(tag)}
												>
													<span
														className="size-2.5 shrink-0 rounded-full ring-1 ring-black/10"
														style={{ backgroundColor: safeTagColor(tag.color) }}
														aria-hidden
													/>
													<span className="min-w-0 flex-1 truncate text-sm">
														{tag.name}
													</span>
													<span
														className={cn(
															"inline-flex size-7 shrink-0 items-center justify-center rounded-lg text-muted-foreground transition-colors",
															selected &&
																"bg-secondary/90 text-secondary-foreground",
														)}
														aria-hidden
													>
														<Icon
															name={selected ? "Check" : "Plus"}
															className="size-4"
														/>
													</span>
												</button>
											);
										}

										return (
											<div
												key={tag.id}
												className={cn(
													"min-w-0 rounded-lg border border-border/70 bg-card/60 px-2.5 py-2 transition-colors",
													"flex flex-col items-stretch gap-2 sm:flex-row sm:items-center",
													batchAction === "add" &&
														"border-primary/45 bg-primary/5",
													batchAction === "remove" &&
														"border-destructive/35 bg-destructive/5",
												)}
											>
												<button
													type="button"
													className="flex min-w-0 flex-1 items-center gap-2 text-left"
													onClick={() => {
														if (target?.mode === "batch") {
															setBatchAction(
																tag.id,
																batchAction === "add" ? "none" : "add",
															);
														}
													}}
												>
													<span
														className="size-2.5 shrink-0 rounded-full ring-1 ring-black/10"
														style={{ backgroundColor: safeTagColor(tag.color) }}
														aria-hidden
													/>
													<span className="min-w-0 flex-1 truncate text-sm">
														{tag.name}
													</span>
												</button>
												{target?.mode === "batch" ? (
													<div className="grid w-full grid-cols-2 gap-1 sm:w-auto sm:shrink-0">
														<Button
															type="button"
															size="xs"
															variant={
																batchAction === "add" ? "secondary" : "outline"
															}
															aria-pressed={batchAction === "add"}
															onClick={() =>
																setBatchAction(
																	tag.id,
																	batchAction === "add" ? "none" : "add",
																)
															}
														>
															<Icon name="Plus" className="size-3" />
															{t("tag_add")}
														</Button>
														<Button
															type="button"
															size="xs"
															variant={
																batchAction === "remove"
																	? "destructive"
																	: "outline"
															}
															aria-pressed={batchAction === "remove"}
															onClick={() =>
																setBatchAction(
																	tag.id,
																	batchAction === "remove" ? "none" : "remove",
																)
															}
														>
															<Icon name="Minus" className="size-3" />
															{t("tag_remove")}
														</Button>
													</div>
												) : null}
											</div>
										);
									})
								) : (
									<p className="rounded-lg border border-border/70 bg-muted/25 p-3 text-sm text-muted-foreground">
										{normalizedQuery
											? t("tag_search_empty")
											: t("tag_library_empty")}
									</p>
								)}

								{!loading && hasMoreTags ? (
									<Button
										type="button"
										size="sm"
										variant="outline"
										className="w-full"
										disabled={loadingMoreTags}
										onClick={() => {
											void loadMoreTags();
										}}
									>
										{loadingMoreTags ? (
											<Icon name="Spinner" className="size-3.5 animate-spin" />
										) : null}
										{t("tag_library_load_more")}
									</Button>
								) : null}
							</div>
						</section>
					</div>
				</ManagerDialogScrollableList>
			</ManagerDialogShell>
			<TagLibraryManagerDialog
				open={libraryManagerOpen}
				onOpenChange={setLibraryManagerOpen}
				onTagDeleted={handleLibraryTagDeleted}
				onTagUpdated={handleLibraryTagUpdated}
			/>
		</>
	);
}
