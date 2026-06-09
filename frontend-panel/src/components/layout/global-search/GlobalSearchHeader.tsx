import { type KeyboardEvent, type Ref, useState } from "react";
import { useTranslation } from "react-i18next";
import { AnimatedCollapsible } from "@/components/common/AnimatedCollapsible";
import { safeTagColor } from "@/components/files/tagColors";
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
	onQueryClear: () => void;
	onQueryChange: (value: string) => void;
	onSubmitSearch: () => void;
	onTagClear: (tagId: number) => void;
	onTagMatchChange: (value: SearchTagMatch) => void;
	onTagMatchClear: () => void;
	onTagToggle: (tagId: number) => void;
	query: string;
	searchReady: boolean;
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
	onQueryClear,
	onQueryChange,
	onSubmitSearch,
	onTagClear,
	onTagMatchChange,
	onTagMatchClear,
	onTagToggle,
	query,
	searchReady,
	selectedTagIds,
	tagLoading,
	tagMatch,
	tags,
}: GlobalSearchHeaderProps) {
	const { t } = useTranslation("search");
	const [filtersOpen, setFiltersOpen] = useState(false);
	const [tagPickerOpen, setTagPickerOpen] = useState(false);
	const selectedTagSet = new Set(selectedTagIds);
	const selectedTags = tags.filter((tag) => selectedTagSet.has(tag.id));
	const activeFilterCount =
		(filter !== "all" ? 1 : 0) +
		(categoryFilter ? 1 : 0) +
		selectedTagIds.length;
	const hasActiveStructuredFilters = activeFilterCount > 0;

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
						className="h-11 rounded-xl border-border/70 bg-muted/35 pr-12 pl-9 text-base shadow-none focus-visible:bg-background"
					/>
					<Button
						type="button"
						variant={
							filtersOpen || hasActiveStructuredFilters ? "secondary" : "ghost"
						}
						size="icon"
						aria-label={
							filtersOpen ? t("search:hide_filters") : t("search:show_filters")
						}
						aria-expanded={filtersOpen}
						onClick={() => setFiltersOpen((open) => !open)}
						className="-translate-y-1/2 absolute top-1/2 right-1.5 z-10 size-8 rounded-lg active:-translate-y-1/2"
					>
						<Icon name="MagnifyingGlassPlus" className="size-4" />
						{hasActiveStructuredFilters ? (
							<span className="-top-1 -right-1 absolute min-w-4 rounded-full bg-primary px-1 text-[10px] font-medium leading-4 text-primary-foreground shadow-xs">
								{activeFilterCount}
							</span>
						) : null}
					</Button>
				</div>
				<Button
					type="button"
					onClick={onSubmitSearch}
					disabled={!searchReady}
					className="shrink-0"
				>
					<Icon name="MagnifyingGlass" className="size-4" />
					<span className="hidden sm:inline">{t("search:submit_search")}</span>
				</Button>
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
			<AnimatedCollapsible open={filtersOpen} contentClassName="pt-3">
				<div className="space-y-3 rounded-xl border border-border/70 bg-muted/20 p-3">
					<fieldset className="space-y-2 border-0 p-0">
						<legend className="text-xs font-medium text-muted-foreground">
							{t("search:result_type")}
						</legend>
						<div className="grid grid-cols-3 gap-1.5 rounded-lg bg-background/70 p-1 ring-1 ring-border/60">
							{SEARCH_FILTER_OPTIONS.map((option) => (
								<Button
									key={option.value}
									type="button"
									variant={filter === option.value ? "secondary" : "ghost"}
									size="sm"
									onClick={() => onFilterChange(option.value)}
									className="min-w-0 rounded-md px-2"
								>
									<span className="truncate">
										{t(`search:${option.labelKey}`)}
									</span>
								</Button>
							))}
						</div>
					</fieldset>
					{filter !== "folder" ? (
						<fieldset className="space-y-2 border-0 p-0">
							<legend className="text-xs font-medium text-muted-foreground">
								{t("search:quick_categories")}
							</legend>
							<div className="flex flex-wrap gap-1.5">
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
							</div>
						</fieldset>
					) : null}
					<fieldset className="space-y-2 border-0 p-0">
						<legend className="text-xs font-medium text-muted-foreground">
							{t("search:tag_filters")}
						</legend>
						<div className="flex flex-wrap items-center gap-2">
							<Button
								type="button"
								variant={
									tagPickerOpen || selectedTagIds.length > 0
										? "secondary"
										: "outline"
								}
								size="sm"
								className="shrink-0 rounded-full"
								aria-expanded={tagPickerOpen}
								onClick={() => setTagPickerOpen((open) => !open)}
							>
								<Icon name="Tag" className="size-3.5" />
								{t("search:select_tags")}
								{selectedTagIds.length > 0 ? (
									<span className="ml-1 rounded-full bg-background/90 px-1.5 py-0.5 text-[11px] font-medium text-foreground shadow-xs">
										{selectedTagIds.length}
									</span>
								) : null}
							</Button>
							<Button
								type="button"
								variant="outline"
								size="sm"
								onClick={() =>
									onTagMatchChange(tagMatch === "any" ? "all" : "any")
								}
								aria-pressed={tagMatch === "any"}
								className="shrink-0 rounded-full"
								hidden={selectedTagIds.length <= 1}
							>
								{tagMatch === "any"
									? t("search:tag_match_any")
									: t("search:tag_match_all")}
							</Button>
						</div>
						<AnimatedCollapsible open={tagPickerOpen} contentClassName="pt-1">
							<div className="max-h-48 overflow-y-auto rounded-lg border border-border/70 bg-background/75 p-2">
								<div className="mb-2 flex items-center">
									<span className="text-xs text-muted-foreground">
										{selectedTagIds.length > 0
											? t("selected_tags", {
													count: selectedTagIds.length,
												})
											: t("tag_filters")}
									</span>
								</div>
								{tagLoading ? (
									<span className="inline-flex h-8 items-center gap-1.5 rounded-full border border-border/60 px-2.5 text-xs text-muted-foreground">
										<Icon name="Spinner" className="size-3.5 animate-spin" />
										{t("search:tag_loading")}
									</span>
								) : tags.length === 0 ? (
									<span className="inline-flex h-8 items-center rounded-full border border-dashed border-border/70 px-2.5 text-xs text-muted-foreground">
										{t("search:no_tags")}
									</span>
								) : (
									<div className="flex flex-wrap gap-1.5">
										{tags.map((tag) => {
											const active = selectedTagSet.has(tag.id);
											return (
												<Button
													key={tag.id}
													type="button"
													variant={active ? "secondary" : "ghost"}
													size="sm"
													onClick={() => onTagToggle(tag.id)}
													className="max-w-full rounded-full sm:max-w-44"
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
										})}
									</div>
								)}
							</div>
						</AnimatedCollapsible>
					</fieldset>
				</div>
			</AnimatedCollapsible>
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
