import type { KeyboardEvent, Ref } from "react";
import { useTranslation } from "react-i18next";
import { safeTagColor } from "@/components/files/TagChips";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import type { FileCategory, TagInfo } from "@/types/api";
import type { SearchCategoryFilter, SearchFilter } from "./types";
import { SEARCH_CATEGORY_OPTIONS, SEARCH_FILTER_OPTIONS } from "./types";

type SearchTagMatch = "any" | "all";

interface GlobalSearchHeaderProps {
	categoryFilter: SearchCategoryFilter;
	filter: SearchFilter;
	inputRef: Ref<HTMLInputElement>;
	onCategoryFilterChange: (category: SearchCategoryFilter) => void;
	onCategoryFilterClear: () => void;
	onClose: () => void;
	onFilterClear: () => void;
	onFilterChange: (filter: SearchFilter) => void;
	onInputBlur: () => void;
	onInputCompositionEnd: (value: string) => void;
	onInputCompositionStart: () => void;
	onInputKeyDown: (event: KeyboardEvent<HTMLInputElement>) => void;
	onManageTagLibrary: () => void;
	onQueryClear: () => void;
	onQueryChange: (value: string) => void;
	onTagClear: (tagId: number) => void;
	onTagMatchChange: (value: SearchTagMatch) => void;
	onTagMatchClear: () => void;
	onTagToggle: (tagId: number) => void;
	query: string;
	selectedTagIds: number[];
	tagLoading: boolean;
	tagMatch: SearchTagMatch;
	tags: TagInfo[];
}

export function GlobalSearchHeader({
	categoryFilter,
	filter,
	inputRef,
	onCategoryFilterChange,
	onCategoryFilterClear,
	onClose,
	onFilterClear,
	onFilterChange,
	onInputBlur,
	onInputCompositionEnd,
	onInputCompositionStart,
	onInputKeyDown,
	onManageTagLibrary,
	onQueryClear,
	onQueryChange,
	onTagClear,
	onTagMatchChange,
	onTagMatchClear,
	onTagToggle,
	query,
	selectedTagIds,
	tagLoading,
	tagMatch,
	tags,
}: GlobalSearchHeaderProps) {
	const { t } = useTranslation(["search", "files"]);
	const selectedTagSet = new Set(selectedTagIds);
	const selectedTags = tags.filter((tag) => selectedTagSet.has(tag.id));

	return (
		<div className="border-b bg-background/95 px-4 py-3">
			<div className="flex items-center gap-3">
				<div className="relative min-w-0 flex-1">
					<Icon
						name="MagnifyingGlass"
						className="-translate-y-1/2 absolute top-1/2 left-3 size-4 text-foreground/55"
					/>
					<Input
						ref={inputRef}
						value={query}
						onChange={(event) => onQueryChange(event.target.value)}
						onCompositionStart={onInputCompositionStart}
						onCompositionEnd={(event) =>
							onInputCompositionEnd(event.currentTarget.value)
						}
						onBlur={onInputBlur}
						onKeyDown={onInputKeyDown}
						placeholder={t("search:placeholder")}
						autoComplete="off"
						spellCheck={false}
						className="h-11 rounded-xl border-border/70 bg-muted/35 pr-3 pl-9 text-base shadow-none focus-visible:bg-background"
					/>
				</div>
				<Button
					type="button"
					variant="ghost"
					size="sm"
					onClick={onClose}
					aria-label={t("search:close_search")}
					className="shrink-0"
				>
					<Icon name="X" className="size-4" />
				</Button>
			</div>
			<div className="mt-3 flex flex-wrap items-center gap-2">
				{SEARCH_FILTER_OPTIONS.map((option) => (
					<Button
						key={option.value}
						type="button"
						variant={filter === option.value ? "secondary" : "ghost"}
						size="sm"
						onClick={() => onFilterChange(option.value)}
						className="rounded-full"
					>
						{t(`search:${option.labelKey}`)}
					</Button>
				))}
			</div>
			{filter !== "folder" ? (
				<fieldset className="mt-2 flex gap-1.5 overflow-x-auto border-0 p-0 pb-1">
					<legend className="sr-only">{t("search:quick_categories")}</legend>
					{SEARCH_CATEGORY_OPTIONS.map((option) => {
						const active = categoryFilter === option.value;
						return (
							<Button
								key={option.value}
								type="button"
								variant={active ? "secondary" : "ghost"}
								size="sm"
								onClick={() => onCategoryFilterChange(option.value)}
								className="shrink-0 rounded-full"
								aria-pressed={active}
							>
								<Icon name={option.icon} className="size-4" />
								{t(`search:${option.labelKey}`)}
							</Button>
						);
					})}
				</fieldset>
			) : null}
			<fieldset className="mt-2 flex items-center gap-1.5 overflow-x-auto border-0 p-0 pb-1">
				<legend className="sr-only">{t("search:tag_filters")}</legend>
				<span className="sticky left-0 shrink-0 bg-background/95 pr-1 text-xs font-medium text-muted-foreground">
					{t("search:tag_filters")}
				</span>
				{tagLoading ? (
					<span className="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-full border border-border/60 px-2.5 text-xs text-muted-foreground">
						<Icon name="Spinner" className="size-3.5 animate-spin" />
						{t("search:tag_loading")}
					</span>
				) : tags.length === 0 ? (
					<span className="inline-flex h-8 shrink-0 items-center rounded-full border border-dashed border-border/70 px-2.5 text-xs text-muted-foreground">
						{t("search:no_tags")}
					</span>
				) : (
					tags.map((tag) => {
						const active = selectedTagSet.has(tag.id);
						return (
							<Button
								key={tag.id}
								type="button"
								variant={active ? "secondary" : "ghost"}
								size="sm"
								onClick={() => onTagToggle(tag.id)}
								className="max-w-36 shrink-0 rounded-full"
								aria-pressed={active}
								title={tag.name}
							>
								<span
									className="size-2 shrink-0 rounded-full ring-1 ring-black/10"
									style={{ backgroundColor: safeTagColor(tag.color) }}
									aria-hidden
								/>
								<span className="truncate">{tag.name}</span>
							</Button>
						);
					})
				)}
				{selectedTagIds.length > 1 ? (
					<Button
						type="button"
						variant="outline"
						size="sm"
						onClick={() => onTagMatchChange(tagMatch === "any" ? "all" : "any")}
						aria-pressed={tagMatch === "any"}
						className="shrink-0 rounded-full"
					>
						{tagMatch === "any"
							? t("search:tag_match_any")
							: t("search:tag_match_all")}
					</Button>
				) : null}
				<span className="h-5 w-px shrink-0 bg-border/70" aria-hidden />
				<Button
					type="button"
					variant="outline"
					size="sm"
					onClick={onManageTagLibrary}
					className="shrink-0 rounded-full"
				>
					<Icon name="Tag" className="size-3.5" />
					{t("files:tag_library_manage")}
				</Button>
			</fieldset>
			<SearchFilterStrip
				categoryFilter={categoryFilter}
				filter={filter}
				onCategoryFilterClear={onCategoryFilterClear}
				onFilterClear={onFilterClear}
				onQueryClear={onQueryClear}
				onTagClear={onTagClear}
				onTagMatchClear={onTagMatchClear}
				query={query}
				selectedTags={selectedTags}
				tagMatch={tagMatch}
			/>
		</div>
	);
}

interface SearchFilterStripProps {
	categoryFilter: SearchCategoryFilter;
	filter: SearchFilter;
	onCategoryFilterClear: () => void;
	onFilterClear: () => void;
	onQueryClear: () => void;
	onTagClear: (tagId: number) => void;
	onTagMatchClear: () => void;
	query: string;
	selectedTags: TagInfo[];
	tagMatch: SearchTagMatch;
}

function SearchFilterStrip({
	categoryFilter,
	filter,
	onCategoryFilterClear,
	onFilterClear,
	onQueryClear,
	onTagClear,
	onTagMatchClear,
	query,
	selectedTags,
	tagMatch,
}: SearchFilterStripProps) {
	const { t } = useTranslation("search");
	const trimmedQuery = query.trim();
	const hasActiveFilters =
		Boolean(trimmedQuery) ||
		filter !== "all" ||
		Boolean(categoryFilter) ||
		selectedTags.length > 0;

	if (!hasActiveFilters) {
		return null;
	}

	return (
		<div className="mt-2 flex items-center gap-1.5 overflow-x-auto pb-1">
			<span className="shrink-0 text-xs font-medium text-muted-foreground">
				{t("active_filters")}
			</span>
			{trimmedQuery ? (
				<SearchFilterChip
					label={t("filter_keyword", { value: trimmedQuery })}
					onClear={onQueryClear}
				/>
			) : null}
			{filter !== "all" ? (
				<SearchFilterChip
					label={t("filter_type", {
						value: t(
							SEARCH_FILTER_OPTIONS.find((option) => option.value === filter)
								?.labelKey ?? "all",
						),
					})}
					onClear={onFilterClear}
				/>
			) : null}
			{categoryFilter ? (
				<SearchFilterChip
					label={t("filter_category", {
						value: getCategoryLabel(t, categoryFilter),
					})}
					onClear={onCategoryFilterClear}
				/>
			) : null}
			{selectedTags.map((tag) => (
				<SearchFilterChip
					key={tag.id}
					label={t("filter_tag", { value: tag.name })}
					onClear={() => onTagClear(tag.id)}
				/>
			))}
			{selectedTags.length > 1 ? (
				<SearchFilterChip
					label={t("filter_tag_match", {
						value: tagMatch === "any" ? t("tag_match_any") : t("tag_match_all"),
					})}
					onClear={onTagMatchClear}
				/>
			) : null}
		</div>
	);
}

function SearchFilterChip({
	label,
	onClear,
}: {
	label: string;
	onClear: () => void;
}) {
	const { t } = useTranslation("search");

	return (
		<span className="inline-flex h-7 shrink-0 items-center gap-1 rounded-full border border-border/70 bg-muted/30 pl-2.5 text-xs text-foreground">
			<span>{label}</span>
			<button
				type="button"
				onClick={onClear}
				aria-label={t("clear_filter")}
				className="flex size-6 items-center justify-center rounded-full text-muted-foreground hover:bg-background hover:text-foreground"
			>
				<Icon name="X" className="size-3.5" />
			</button>
		</span>
	);
}

function getCategoryLabel(
	t: (key: string, options?: Record<string, unknown>) => string,
	category: FileCategory,
) {
	return t(
		SEARCH_CATEGORY_OPTIONS.find((option) => option.value === category)
			?.labelKey ?? "category_other",
	);
}
