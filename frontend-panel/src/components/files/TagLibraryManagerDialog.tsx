import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
	FixedDialogFooter,
	InlineConfirm,
	ManagerDialogScrollableList,
	ManagerDialogShell,
} from "@/components/common/ManagerDialogShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { handleApiError } from "@/hooks/useApiError";
import { tagService } from "@/services/tagService";
import type { TagInfo } from "@/types/api";
import { safeTagColor, TAG_COLOR_PALETTE, tagColorFromName } from "./tagColors";

const TAG_LIBRARY_MANAGER_LIMIT = 50;
const CREATE_TAG_BUSY_ID = -1;

type EditingDraft = {
	color: string;
	id: number;
	name: string;
};

function sortTags(tags: TagInfo[]) {
	return [...tags].sort((a, b) =>
		a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
	);
}

function normalizeTagName(value: string) {
	return value.trim();
}

function normalizeTagLookup(value: string) {
	return value.trim().toLocaleLowerCase();
}

function canCreateTag(tags: TagInfo[], name: string) {
	const normalized = normalizeTagLookup(name);
	if (!normalized) return false;
	return !tags.some(
		(tag) => tag.name.trim().toLocaleLowerCase() === normalized,
	);
}

export function TagLibraryManagerDialog({
	open,
	onOpenChange,
	onTagCreated,
	onTagDeleted,
	onTagUpdated,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onTagCreated?: (tag: TagInfo) => void;
	onTagDeleted?: (tagId: number) => void;
	onTagUpdated?: (tag: TagInfo) => void;
}) {
	const { t } = useTranslation(["files", "core"]);
	const [tags, setTags] = useState<TagInfo[]>([]);
	const [total, setTotal] = useState(0);
	const [query, setQuery] = useState("");
	const [loading, setLoading] = useState(false);
	const [loadingMore, setLoadingMore] = useState(false);
	const [busyId, setBusyId] = useState<number | null>(null);
	const [editingDraft, setEditingDraft] = useState<EditingDraft | null>(null);
	const [deleteTarget, setDeleteTarget] = useState<TagInfo | null>(null);
	const normalizedQuery = query.trim();
	const currentInteractionResetKeyRef = useRef(normalizedQuery);

	if (currentInteractionResetKeyRef.current !== normalizedQuery) {
		currentInteractionResetKeyRef.current = normalizedQuery;
		setEditingDraft(null);
		setDeleteTarget(null);
	}

	const resetDialogState = useCallback(() => {
		currentInteractionResetKeyRef.current = "";
		setTags([]);
		setTotal(0);
		setQuery("");
		setLoading(false);
		setLoadingMore(false);
		setBusyId(null);
		setEditingDraft(null);
		setDeleteTarget(null);
	}, []);

	const handleOpenChangeComplete = useCallback(
		(nextOpen: boolean) => {
			if (!nextOpen) resetDialogState();
		},
		[resetDialogState],
	);

	const hasMore = tags.length < total;
	const editingTag = useMemo(
		() => tags.find((tag) => tag.id === editingDraft?.id) ?? null,
		[editingDraft?.id, tags],
	);
	const canSaveEdit =
		editingTag !== null &&
		editingDraft !== null &&
		normalizeTagName(editingDraft.name).length > 0 &&
		(normalizeTagName(editingDraft.name) !== editingTag.name ||
			safeTagColor(editingDraft.color) !== safeTagColor(editingTag.color));
	const createCandidate = canCreateTag(tags, query);
	const creating = busyId === CREATE_TAG_BUSY_ID;
	const showCreateAction = !loading && createCandidate;
	const canCreate = busyId === null && showCreateAction;
	const createPreviewColor = safeTagColor(tagColorFromName(query));

	useEffect(() => {
		if (!open) {
			return;
		}

		let cancelled = false;
		setLoading(true);
		tagService
			.listTags({
				params: {
					limit: TAG_LIBRARY_MANAGER_LIMIT,
					offset: 0,
					...(normalizedQuery ? { q: normalizedQuery } : {}),
				},
			})
			.then((page) => {
				if (!cancelled) {
					setTags(sortTags(page.items));
					setTotal(page.total);
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
	}, [normalizedQuery, open]);

	const loadMore = useCallback(async () => {
		if (loadingMore || !hasMore) return;

		setLoadingMore(true);
		try {
			const page = await tagService.listTags({
				params: {
					limit: TAG_LIBRARY_MANAGER_LIMIT,
					offset: tags.length,
					...(normalizedQuery ? { q: normalizedQuery } : {}),
				},
			});
			setTags((current) => sortTags([...current, ...page.items]));
			setTotal(page.total);
		} catch (err) {
			handleApiError(err);
		} finally {
			setLoadingMore(false);
		}
	}, [hasMore, loadingMore, normalizedQuery, tags.length]);

	const startEdit = (tag: TagInfo) => {
		setDeleteTarget(null);
		setEditingDraft({
			color: safeTagColor(tag.color),
			id: tag.id,
			name: tag.name,
		});
	};

	const cancelEdit = () => {
		setEditingDraft(null);
	};

	const saveEdit = async () => {
		if (!editingTag || !editingDraft || !canSaveEdit) return;

		const name = normalizeTagName(editingDraft.name);
		const color = safeTagColor(editingDraft.color);
		setBusyId(editingTag.id);
		try {
			const updated = await tagService.patchTag(editingTag.id, { color, name });
			setTags((current) =>
				sortTags(current.map((tag) => (tag.id === updated.id ? updated : tag))),
			);
			onTagUpdated?.(updated);
			cancelEdit();
			toast.success(t("tag_updated"));
		} catch (err) {
			handleApiError(err);
		} finally {
			setBusyId(null);
		}
	};

	const deleteTag = async (tag: TagInfo) => {
		setBusyId(tag.id);
		try {
			await tagService.deleteTag(tag.id);
			setTags((current) => current.filter((item) => item.id !== tag.id));
			setTotal((current) => Math.max(0, current - 1));
			if (editingDraft?.id === tag.id) cancelEdit();
			onTagDeleted?.(tag.id);
			toast.success(t("tag_deleted"));
		} catch (err) {
			handleApiError(err);
		} finally {
			setBusyId(null);
		}
	};

	const createTag = async () => {
		if (!canCreate) return;

		const name = normalizeTagName(query);
		const color = createPreviewColor;
		setBusyId(CREATE_TAG_BUSY_ID);
		try {
			const created = await tagService.createTag({ color, name });
			setTags((current) => sortTags([...current, created]));
			setTotal((current) => current + 1);
			setQuery("");
			onTagCreated?.(created);
			toast.success(t("tag_library_created"));
		} catch (err) {
			handleApiError(err);
		} finally {
			setBusyId(null);
		}
	};

	const controls = (
		<div className="space-y-2">
			<label
				className="text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground"
				htmlFor="tag-library-manager-search"
			>
				{t("tag_search_label")}
			</label>
			<div className="relative">
				<Icon
					name="MagnifyingGlass"
					className="-translate-y-1/2 absolute top-1/2 left-2.5 size-4 text-muted-foreground"
				/>
				<Input
					id="tag-library-manager-search"
					value={query}
					onChange={(event) => setQuery(event.target.value)}
					placeholder={t("tag_search_placeholder")}
					className="pl-8"
					maxLength={64}
					onKeyDown={(event) => {
						if (event.key === "Enter" && canCreate) {
							event.preventDefault();
							void createTag();
						}
					}}
				/>
			</div>
		</div>
	);

	return (
		<ManagerDialogShell
			open={open}
			onOpenChange={onOpenChange}
			onOpenChangeComplete={handleOpenChangeComplete}
			title={t("tag_library_manage_title")}
			description={t("tag_library_manage_desc")}
			controls={controls}
			className="sm:h-[min(88vh,44rem)] sm:max-w-xl"
			footer={
				<FixedDialogFooter>
					<div className="flex items-center justify-between gap-3">
						<div className="min-w-0 text-sm text-muted-foreground">
							{t("tag_usage_count", { count: total })}
						</div>
						<Button
							type="button"
							variant="outline"
							className="shrink-0"
							onClick={() => onOpenChange(false)}
						>
							{t("core:close")}
						</Button>
					</div>
				</FixedDialogFooter>
			}
		>
			<ManagerDialogScrollableList className="space-y-4">
				<div className="flex min-h-0 flex-1 flex-col gap-4">
					<div className="space-y-2">
						{showCreateAction ? (
							<button
								type="button"
								className="flex h-10 w-full min-w-0 items-center gap-2 rounded-lg border border-dashed border-primary/45 bg-primary/5 px-2.5 text-left text-sm transition-colors hover:bg-primary/10"
								disabled={!canCreate}
								onClick={() => {
									void createTag();
								}}
							>
								<span
									className="size-2.5 shrink-0 rounded-full ring-1 ring-black/10"
									style={{ backgroundColor: createPreviewColor }}
									aria-hidden
								/>
								<span className="min-w-0 flex-1 truncate">
									{t("tag_create_named", { name: normalizeTagName(query) })}
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
						) : tags.length > 0 ? (
							tags.map((tag) => {
								const editing = editingDraft?.id === tag.id;
								const confirmingDelete = deleteTarget?.id === tag.id;
								const busy = busyId === tag.id;

								return (
									<div
										key={tag.id}
										className="rounded-lg border border-border/70 bg-card/60 p-2.5"
									>
										{editing ? (
											<div className="flex min-w-0 flex-col gap-3">
												<div className="flex min-w-0 flex-col gap-2 sm:flex-row sm:items-start">
													<div className="min-w-0 flex-1">
														<div className="relative min-w-0">
															<span
																className="-translate-y-1/2 absolute top-1/2 left-2.5 size-2.5 rounded-full ring-1 ring-black/10"
																style={{
																	backgroundColor: safeTagColor(
																		editingDraft?.color,
																	),
																}}
																aria-hidden
															/>
															<Input
																value={editingDraft?.name ?? ""}
																onChange={(event) =>
																	setEditingDraft((draft) =>
																		draft
																			? {
																					...draft,
																					name: event.target.value,
																				}
																			: draft,
																	)
																}
																className="pl-7"
																maxLength={64}
																aria-label={t("tag_name")}
																onKeyDown={(event) => {
																	if (event.key === "Enter") {
																		event.preventDefault();
																		void saveEdit();
																	}
																	if (event.key === "Escape") {
																		event.preventDefault();
																		cancelEdit();
																	}
																}}
															/>
														</div>
													</div>
													<div className="grid grid-cols-2 gap-2 sm:flex sm:shrink-0">
														<Button
															type="button"
															size="sm"
															variant="outline"
															disabled={busy}
															onClick={cancelEdit}
														>
															{t("core:cancel")}
														</Button>
														<Button
															type="button"
															size="sm"
															disabled={!canSaveEdit || busy}
															onClick={() => {
																void saveEdit();
															}}
														>
															{busy ? (
																<Icon
																	name="Spinner"
																	className="size-3.5 animate-spin"
																/>
															) : null}
															{t("core:save")}
														</Button>
													</div>
												</div>
												<fieldset className="flex flex-wrap gap-1.5 sm:flex-nowrap">
													<legend className="sr-only">{t("tag_color")}</legend>
													{TAG_COLOR_PALETTE.map((color) => {
														const selected =
															safeTagColor(editingDraft?.color) === color;
														return (
															<button
																key={color}
																type="button"
																className={`flex size-7 items-center justify-center rounded-lg border bg-background transition-[border-color,box-shadow] sm:size-6 ${
																	selected
																		? "border-ring ring-2 ring-ring/30"
																		: "border-border/70 hover:border-foreground/30"
																}`}
																aria-label={t("tag_color_option", {
																	color,
																})}
																aria-pressed={selected}
																onClick={() =>
																	setEditingDraft((draft) =>
																		draft ? { ...draft, color } : draft,
																	)
																}
															>
																<span
																	className="size-3.5 rounded-full ring-1 ring-black/10 sm:size-3"
																	style={{ backgroundColor: color }}
																	aria-hidden
																/>
															</button>
														);
													})}
												</fieldset>
											</div>
										) : confirmingDelete ? (
											<InlineConfirm className="space-y-3">
												<div className="flex min-w-0 items-center gap-2">
													<span
														className="size-2.5 shrink-0 rounded-full ring-1 ring-black/10"
														style={{ backgroundColor: safeTagColor(tag.color) }}
														aria-hidden
													/>
													<div className="min-w-0 flex-1">
														<div className="truncate text-sm font-medium">
															{t("tag_delete_confirm_title")}
														</div>
														<div className="text-xs text-muted-foreground">
															{t("tag_delete_confirm_desc", {
																name: tag.name,
															})}
														</div>
													</div>
												</div>
												<div className="grid grid-cols-2 gap-2">
													<Button
														type="button"
														size="sm"
														variant="outline"
														disabled={busy}
														onClick={() => setDeleteTarget(null)}
													>
														{t("core:cancel")}
													</Button>
													<Button
														type="button"
														size="sm"
														variant="destructive"
														disabled={busy}
														onClick={() => {
															void deleteTag(tag);
														}}
													>
														{busy ? (
															<Icon
																name="Spinner"
																className="size-3.5 animate-spin"
															/>
														) : (
															<Icon name="Trash" className="size-3.5" />
														)}
														{t("core:delete")}
													</Button>
												</div>
											</InlineConfirm>
										) : (
											<div className="flex min-w-0 items-center gap-2">
												<span
													className="size-2.5 shrink-0 rounded-full ring-1 ring-black/10"
													style={{ backgroundColor: safeTagColor(tag.color) }}
													aria-hidden
												/>
												<div className="min-w-0 flex-1">
													<div className="truncate text-sm font-medium">
														{tag.name}
													</div>
													<div className="text-xs text-muted-foreground">
														{t("tag_usage_count", {
															count: tag.usage_count,
														})}
													</div>
												</div>
												<div className="flex shrink-0 items-center gap-1">
													<Button
														type="button"
														size="icon-sm"
														variant="ghost"
														disabled={busy}
														aria-label={t("tag_edit")}
														onClick={() => startEdit(tag)}
													>
														<Icon name="PencilSimple" className="size-4" />
													</Button>
													<Button
														type="button"
														size="icon-sm"
														variant="destructive"
														disabled={busy}
														aria-label={t("tag_delete")}
														onClick={() => {
															cancelEdit();
															setDeleteTarget(tag);
														}}
													>
														<Icon name="Trash" className="size-4" />
													</Button>
												</div>
											</div>
										)}
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

						{!loading && hasMore ? (
							<Button
								type="button"
								size="sm"
								variant="outline"
								className="w-full"
								disabled={loadingMore}
								onClick={() => {
									void loadMore();
								}}
							>
								{loadingMore ? (
									<Icon name="Spinner" className="size-3.5 animate-spin" />
								) : null}
								{t("tag_library_load_more")}
							</Button>
						) : null}
					</div>
				</div>
			</ManagerDialogScrollableList>
		</ManagerDialogShell>
	);
}
