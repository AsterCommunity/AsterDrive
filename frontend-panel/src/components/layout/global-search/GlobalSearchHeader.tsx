import type { KeyboardEvent, Ref } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import type { SearchCategoryFilter, SearchFilter } from "./types";
import { SEARCH_CATEGORY_OPTIONS, SEARCH_FILTER_OPTIONS } from "./types";

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
	onQueryChange: (value: string) => void;
	query: string;
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
	onQueryChange,
	query,
}: GlobalSearchHeaderProps) {
	const { t } = useTranslation(["search"]);

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
		</div>
	);
}
