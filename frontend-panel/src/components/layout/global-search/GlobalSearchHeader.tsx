import type { KeyboardEvent, Ref } from "react";
import { useTranslation } from "react-i18next";
import { safeTagColor } from "@/components/files/TagChips";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import type { TagInfo } from "@/types/api";
import type { SearchCategoryFilter, SearchFilter } from "./types";
import { SEARCH_CATEGORY_OPTIONS, SEARCH_FILTER_OPTIONS } from "./types";

type SearchTagMatch = "any" | "all";

interface GlobalSearchHeaderProps {
	categoryFilter: SearchCategoryFilter;
	filter: SearchFilter;
	inputRef: Ref<HTMLInputElement>;
	onCategoryFilterChange: (category: SearchCategoryFilter) => void;
	onClose: () => void;
	onFilterChange: (filter: SearchFilter) => void;
	onInputBlur: () => void;
	onInputCompositionEnd: (value: string) => void;
	onInputCompositionStart: () => void;
	onInputKeyDown: (event: KeyboardEvent<HTMLInputElement>) => void;
	onManageTagLibrary: () => void;
	onQueryChange: (value: string) => void;
	onTagMatchChange: (value: SearchTagMatch) => void;
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
	onClose,
	onFilterChange,
	onInputBlur,
	onInputCompositionEnd,
	onInputCompositionStart,
	onInputKeyDown,
	onManageTagLibrary,
	onQueryChange,
	onTagMatchChange,
	onTagToggle,
	query,
	selectedTagIds,
	tagLoading,
	tagMatch,
	tags,
}: GlobalSearchHeaderProps) {
	const { t } = useTranslation(["search", "files"]);
	const selectedTagSet = new Set(selectedTagIds);

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
		</div>
	);
}
