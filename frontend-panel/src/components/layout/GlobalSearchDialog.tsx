import { type KeyboardEvent, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { GlobalSearchHeader } from "@/components/layout/global-search/GlobalSearchHeader";
import type {
	SearchCategoryFilter,
	SearchFilter,
} from "@/components/layout/global-search/types";
import {
	Dialog,
	DialogContent,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Icon } from "@/components/ui/icon";
import { isImeComposingKeyEvent } from "@/lib/keyboard";
import { workspaceSearchPath } from "@/lib/workspace";
import { isRequestCanceled } from "@/services/http";
import { createTagService } from "@/services/tagService";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import type { FileCategory, TagInfo } from "@/types/api";

const TAG_FILTER_LIMIT = 100;

type SearchTagMatch = "any" | "all";

interface GlobalSearchDialogProps {
	initialCategory?: FileCategory | null;
	open: boolean;
	onOpenChange: (open: boolean) => void;
}

export function GlobalSearchDialog({
	initialCategory = null,
	open,
	onOpenChange,
}: GlobalSearchDialogProps) {
	const { t } = useTranslation(["core", "files", "search"]);
	const navigate = useNavigate();
	const workspace = useWorkspaceStore((state) => state.workspace);
	const inputRef = useRef<HTMLInputElement | null>(null);
	const inputComposingRef = useRef(false);
	const inputCompositionEndAtRef = useRef(0);
	const [query, setQuery] = useState("");
	const [filter, setFilter] = useState<SearchFilter>("all");
	const [categoryFilter, setCategoryFilter] =
		useState<SearchCategoryFilter>(null);
	const [tagLoading, setTagLoading] = useState(false);
	const [tags, setTags] = useState<TagInfo[]>([]);
	const [selectedTagIds, setSelectedTagIds] = useState<number[]>([]);
	const [tagMatch, setTagMatch] = useState<SearchTagMatch>("any");

	const trimmedQuery = query.trim();
	const hasSearchCriteria = Boolean(
		trimmedQuery || categoryFilter || selectedTagIds.length > 0,
	);
	const searchType: SearchFilter = categoryFilter ? "file" : filter;
	const openSearchPage = () => {
		if (!hasSearchCriteria) {
			return;
		}
		onOpenChange(false);
		navigate(
			workspaceSearchPath(workspace, {
				...(trimmedQuery ? { q: trimmedQuery } : {}),
				type: searchType,
				...(categoryFilter ? { category: categoryFilter } : {}),
				...(selectedTagIds.length > 0
					? {
							tag_ids: selectedTagIds.join(","),
							tag_match: tagMatch,
						}
					: {}),
			}),
			{ viewTransition: false },
		);
	};

	useEffect(() => {
		if (!open) {
			inputComposingRef.current = false;
			inputCompositionEndAtRef.current = 0;
			setQuery("");
			setFilter("all");
			setCategoryFilter(null);
			setTags([]);
			setTagLoading(false);
			setSelectedTagIds([]);
			setTagMatch("any");
			return;
		}

		const timer = window.setTimeout(() => {
			inputRef.current?.focus();
			inputRef.current?.select();
		}, 0);
		return () => window.clearTimeout(timer);
	}, [open]);

	useEffect(() => {
		if (!open) {
			return;
		}

		let cancelled = false;
		const workspaceTagService = createTagService(workspace);
		setTagLoading(true);
		workspaceTagService
			.listTags({ params: { limit: TAG_FILTER_LIMIT, offset: 0 } })
			.then((tagPage) => {
				if (!cancelled) {
					setTags(tagPage.items);
				}
			})
			.catch((tagError) => {
				if (!cancelled && !isRequestCanceled(tagError)) {
					setTags([]);
				}
			})
			.finally(() => {
				if (!cancelled) {
					setTagLoading(false);
				}
			});

		return () => {
			cancelled = true;
		};
	}, [open, workspace]);

	useEffect(() => {
		if (!open || !initialCategory) {
			return;
		}

		setQuery("");
		setFilter("file");
		setCategoryFilter(initialCategory);
	}, [initialCategory, open]);

	const handleToggleTag = (tagId: number) => {
		setSelectedTagIds((current) =>
			current.includes(tagId)
				? current.filter((id) => id !== tagId)
				: [...current, tagId],
		);
	};

	const handleInputKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
		if (
			inputComposingRef.current ||
			isImeComposingKeyEvent(event, {
				lastCompositionEndAt: inputCompositionEndAtRef.current,
			})
		) {
			return;
		}

		if (event.key === "Enter" && hasSearchCriteria) {
			event.preventDefault();
			openSearchPage();
			return;
		}

		if (event.key === "Escape") {
			event.preventDefault();
			onOpenChange(false);
		}
	};

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent
				showCloseButton={false}
				className="max-w-[min(760px,calc(100vw-1rem))] gap-0 overflow-hidden p-0 sm:max-w-[min(760px,calc(100vw-2rem))]"
			>
				<DialogHeader className="sr-only">
					<DialogTitle>{t("search:dialog_title")}</DialogTitle>
				</DialogHeader>
				<GlobalSearchHeader
					categoryFilter={categoryFilter}
					filter={filter}
					inputRef={inputRef}
					onCategoryFilterChange={(nextCategory) => {
						setCategoryFilter((current) =>
							current === nextCategory ? null : nextCategory,
						);
						setFilter("file");
					}}
					onCategoryFilterClear={() => {
						setCategoryFilter(null);
					}}
					onClose={() => onOpenChange(false)}
					onFilterClear={() => {
						setFilter("all");
					}}
					onFilterChange={(nextFilter) => {
						setFilter(nextFilter);
						if (nextFilter !== "file") {
							setCategoryFilter(null);
						}
					}}
					onInputBlur={() => {
						inputComposingRef.current = false;
					}}
					onInputCompositionEnd={(value) => {
						inputComposingRef.current = false;
						inputCompositionEndAtRef.current = Date.now();
						setQuery(value);
					}}
					onInputCompositionStart={() => {
						inputComposingRef.current = true;
					}}
					onInputKeyDown={handleInputKeyDown}
					onQueryClear={() => {
						setQuery("");
					}}
					onQueryChange={setQuery}
					onTagClear={(tagId) => {
						setSelectedTagIds((current) =>
							current.filter((currentTagId) => currentTagId !== tagId),
						);
					}}
					onTagMatchChange={setTagMatch}
					onTagMatchClear={() => setTagMatch("any")}
					onTagToggle={handleToggleTag}
					onSubmitSearch={openSearchPage}
					query={query}
					searchReady={hasSearchCriteria}
					selectedTagIds={selectedTagIds}
					tagLoading={tagLoading}
					tagMatch={tagMatch}
					tags={tags}
				/>
				<div className="flex items-center justify-between gap-3 bg-muted/20 px-4 py-3 text-xs text-muted-foreground">
					<span className="inline-flex min-w-0 items-center gap-1.5">
						<Icon name="MagnifyingGlass" className="size-3.5 shrink-0" />
						<span className="truncate">
							{hasSearchCriteria
								? t("search:dialog_ready")
								: t("search:dialog_empty")}
						</span>
					</span>
					<kbd className="hidden rounded-md border border-border/70 bg-background px-2 py-1 font-sans sm:inline-flex">
						Enter
					</kbd>
				</div>
			</DialogContent>
		</Dialog>
	);
}
