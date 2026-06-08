import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { handleApiError } from "@/hooks/useApiError";
import { tagService } from "@/services/tagService";
import type { TagInfo } from "@/types/api";
import { safeTagColor } from "./TagChips";

const TAG_LIBRARY_MANAGER_LIMIT = 50;

function sortTags(tags: TagInfo[]) {
	return [...tags].sort((a, b) =>
		a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
	);
}

function normalizeTagName(value: string) {
	return value.trim();
}

export function TagLibraryManagerDialog({
	open,
	onOpenChange,
	onTagDeleted,
	onTagUpdated,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
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
	const [editingId, setEditingId] = useState<number | null>(null);
	const [editingName, setEditingName] = useState("");
	const [deleteTarget, setDeleteTarget] = useState<TagInfo | null>(null);

	const normalizedQuery = query.trim();
	const hasMore = tags.length < total;
	const editingTag = useMemo(
		() => tags.find((tag) => tag.id === editingId) ?? null,
		[editingId, tags],
	);
	const canSaveEdit =
		editingTag !== null &&
		normalizeTagName(editingName).length > 0 &&
		normalizeTagName(editingName) !== editingTag.name;

	useEffect(() => {
		if (!open) {
			setTags([]);
			setTotal(0);
			setQuery("");
			setLoading(false);
			setLoadingMore(false);
			setBusyId(null);
			setEditingId(null);
			setEditingName("");
			setDeleteTarget(null);
			return;
		}

		let cancelled = false;
		setLoading(true);
		setEditingId(null);
		setEditingName("");
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
		setEditingId(tag.id);
		setEditingName(tag.name);
	};

	const cancelEdit = () => {
		setEditingId(null);
		setEditingName("");
	};

	const saveEdit = async () => {
		if (!editingTag || !canSaveEdit) return;

		const name = normalizeTagName(editingName);
		setBusyId(editingTag.id);
		try {
			const updated = await tagService.patchTag(editingTag.id, { name });
			setTags((current) =>
				sortTags(current.map((tag) => (tag.id === updated.id ? updated : tag))),
			);
			onTagUpdated?.(updated);
			cancelEdit();
			toast.success(t("tag_renamed"));
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
			if (editingId === tag.id) cancelEdit();
			onTagDeleted?.(tag.id);
			toast.success(t("tag_deleted"));
		} catch (err) {
			handleApiError(err);
		} finally {
			setBusyId(null);
		}
	};

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="flex max-h-[min(88vh,42rem)] max-w-[calc(100%-1rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-xl">
				<DialogHeader className="border-b px-5 py-4">
					<DialogTitle>{t("tag_library_manage_title")}</DialogTitle>
					<DialogDescription>{t("tag_library_manage_desc")}</DialogDescription>
				</DialogHeader>

				<div className="flex min-h-0 flex-1 flex-col gap-4 p-5">
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
							/>
						</div>
					</div>

					<div className="min-h-48 flex-1 space-y-1.5 overflow-y-auto pr-1">
						{loading ? (
							<p className="rounded-lg border border-border/70 bg-muted/25 p-3 text-sm text-muted-foreground">
								{t("info_loading")}
							</p>
						) : tags.length > 0 ? (
							tags.map((tag) => {
								const editing = editingId === tag.id;
								const confirmingDelete = deleteTarget?.id === tag.id;
								const busy = busyId === tag.id;

								return (
									<div
										key={tag.id}
										className="rounded-lg border border-border/70 bg-card/60 p-2.5"
									>
										{editing ? (
											<div className="flex min-w-0 flex-col gap-2 sm:flex-row sm:items-center">
												<div className="relative min-w-0 flex-1">
													<span
														className="-translate-y-1/2 absolute top-1/2 left-2.5 size-2.5 rounded-full ring-1 ring-black/10"
														style={{
															backgroundColor: safeTagColor(tag.color),
														}}
														aria-hidden
													/>
													<Input
														value={editingName}
														onChange={(event) =>
															setEditingName(event.target.value)
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
										) : confirmingDelete ? (
											<div className="space-y-3">
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
											</div>
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
			</DialogContent>
		</Dialog>
	);
}
