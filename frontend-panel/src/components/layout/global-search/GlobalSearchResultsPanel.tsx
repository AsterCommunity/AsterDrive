import type { RefObject } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "@/components/ui/icon";
import type { SearchResults } from "@/types/api";
import { GlobalSearchResultRow } from "./GlobalSearchResultRow";
import type { SearchEntry } from "./types";

interface GlobalSearchResultsPanelProps {
	activeIndex: number;
	canLoadMore: boolean;
	error: string | null;
	loading: boolean;
	loadingMore: boolean;
	loadMoreRef: RefObject<HTMLDivElement | null>;
	onHoverResult: (index: number) => void;
	onSelectResult: (entry: SearchEntry) => void;
	openingKey: string | null;
	resultEntries: SearchEntry[];
	resultListRef: RefObject<HTMLDivElement | null>;
	results: SearchResults;
	searchActive: boolean;
}

export function GlobalSearchResultsPanel({
	activeIndex,
	canLoadMore,
	error,
	loading,
	loadingMore,
	loadMoreRef,
	onHoverResult,
	onSelectResult,
	openingKey,
	resultEntries,
	resultListRef,
	results,
	searchActive,
}: GlobalSearchResultsPanelProps) {
	const { t } = useTranslation(["files", "search"]);

	return (
		<div className="flex max-h-[min(68vh,560px)] min-h-80 flex-col">
			<div className="border-b bg-muted/20 px-4 py-2 text-xs text-muted-foreground">
				{searchActive
					? t("search:results", {
							files: results.total_files,
							folders: results.total_folders,
						})
					: t("search:start_typing")}
			</div>
			<div ref={resultListRef} className="flex-1 overflow-y-auto px-2 py-2">
				{!searchActive ? (
					<GlobalSearchEmptyPrompt />
				) : loading && resultEntries.length === 0 ? (
					<div className="flex h-full min-h-56 flex-col items-center justify-center gap-3 text-sm text-muted-foreground">
						<Icon name="Spinner" className="h-5 w-5 animate-spin" />
						<span>{t("search:searching")}</span>
					</div>
				) : error ? (
					<div className="flex h-full min-h-56 flex-col items-center justify-center gap-3 px-4 text-center">
						<div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-destructive/10 text-destructive">
							<Icon name="Warning" className="h-5 w-5" />
						</div>
						<div className="space-y-1">
							<p className="text-sm font-medium">{t("search:search_error")}</p>
							<p className="text-sm text-muted-foreground">{error}</p>
						</div>
					</div>
				) : resultEntries.length === 0 ? (
					<div className="flex h-full min-h-56 flex-col items-center justify-center gap-3 px-4 text-center">
						<div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-muted text-muted-foreground">
							<Icon name="MagnifyingGlass" className="h-5 w-5" />
						</div>
						<div className="space-y-1">
							<p className="text-sm font-medium">{t("search:no_results")}</p>
							<p className="text-sm text-muted-foreground">
								{t("search:no_results_desc")}
							</p>
						</div>
					</div>
				) : (
					<div className="space-y-4 px-1 py-1">
						{results.folders.length > 0 ? (
							<div className="space-y-1">
								<div className="px-2 text-xs font-medium uppercase tracking-[0.14em] text-muted-foreground">
									{t("files:folders_section")}
								</div>
								<div className="space-y-1">
									{results.folders.map((item, index) => (
										<GlobalSearchResultRow
											key={`folder-${item.id}`}
											active={activeIndex === index}
											entry={{
												key: `folder-${item.id}`,
												kind: "folder",
												item,
											}}
											index={index}
											opening={openingKey === `folder-${item.id}`}
											onHover={onHoverResult}
											onSelect={onSelectResult}
										/>
									))}
								</div>
							</div>
						) : null}
						{results.files.length > 0 ? (
							<div className="space-y-1">
								<div className="px-2 text-xs font-medium uppercase tracking-[0.14em] text-muted-foreground">
									{t("files:files_section")}
								</div>
								<div className="space-y-1">
									{results.files.map((item, index) => {
										const absoluteIndex = results.folders.length + index;
										return (
											<GlobalSearchResultRow
												key={`file-${item.id}`}
												active={activeIndex === absoluteIndex}
												entry={{
													key: `file-${item.id}`,
													kind: "file",
													item,
												}}
												index={absoluteIndex}
												opening={openingKey === `file-${item.id}`}
												onHover={onHoverResult}
												onSelect={onSelectResult}
											/>
										);
									})}
								</div>
							</div>
						) : null}
						{canLoadMore ? (
							<div
								ref={loadMoreRef}
								data-search-load-more="true"
								className="flex justify-center py-3"
							>
								{loadingMore ? (
									<div className="h-5 w-5 animate-spin rounded-full border-2 border-muted-foreground/30 border-t-muted-foreground" />
								) : (
									<div className="h-5 w-5" />
								)}
							</div>
						) : null}
					</div>
				)}
			</div>
			<div className="flex items-center justify-between gap-3 border-t bg-muted/20 px-4 py-2 text-xs text-muted-foreground">
				<span>{t("search:open_selected_desc")}</span>
				<div className="flex items-center gap-1.5">
					<kbd className="rounded-md border border-border/70 bg-background px-2 py-1 font-sans">
						Enter
					</kbd>
					<kbd className="rounded-md border border-border/70 bg-background px-2 py-1 font-sans">
						Esc
					</kbd>
				</div>
			</div>
		</div>
	);
}

function GlobalSearchEmptyPrompt() {
	const { t } = useTranslation("search");

	return (
		<div className="grid gap-3 px-2 py-3 sm:grid-cols-2">
			<div className="rounded-2xl border border-border/60 bg-muted/25 p-4">
				<div className="mb-3 flex h-10 w-10 items-center justify-center rounded-xl bg-background text-primary ring-1 ring-border/60">
					<Icon name="MagnifyingGlass" className="h-4 w-4" />
				</div>
				<div className="space-y-1">
					<p className="text-sm font-medium">{t("search:start_typing")}</p>
					<p className="text-sm text-muted-foreground">
						{t("search:start_typing_desc")}
					</p>
				</div>
			</div>
			<div className="rounded-2xl border border-border/60 bg-muted/25 p-4">
				<div className="mb-3 flex h-10 w-10 items-center justify-center rounded-xl bg-background text-primary ring-1 ring-border/60">
					<Icon name="ArrowDown" className="h-4 w-4" />
				</div>
				<div className="space-y-1">
					<p className="text-sm font-medium">{t("search:navigate_results")}</p>
					<p className="text-sm text-muted-foreground">
						{t("search:navigate_results_desc")}
					</p>
				</div>
			</div>
		</div>
	);
}
