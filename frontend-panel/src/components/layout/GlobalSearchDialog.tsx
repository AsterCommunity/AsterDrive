import {
	type KeyboardEvent,
	useEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { GlobalSearchHeader } from "@/components/layout/global-search/GlobalSearchHeader";
import { GlobalSearchResultsPanel } from "@/components/layout/global-search/GlobalSearchResultsPanel";
import {
	getSearchOffset,
	hasMoreSearchResults,
} from "@/components/layout/global-search/searchResultState";
import {
	EMPTY_RESULTS,
	type SearchCategoryFilter,
	type SearchEntry,
	type SearchFilter,
	type SearchPreviewLocationState,
} from "@/components/layout/global-search/types";
import {
	Dialog,
	DialogContent,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { handleApiError } from "@/hooks/useApiError";
import { isImeComposingKeyEvent } from "@/lib/keyboard";
import { workspaceFolderPath } from "@/lib/workspace";
import { fileService } from "@/services/fileService";
import { isRequestCanceled } from "@/services/http";
import { searchService } from "@/services/searchService";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import type { FileCategory, SearchParams, SearchResults } from "@/types/api";

const SEARCH_DEBOUNCE_MS = 180;
const SEARCH_RESULT_LIMIT = 10;

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
	const resultListRef = useRef<HTMLDivElement | null>(null);
	const loadMoreRef = useRef<HTMLDivElement | null>(null);
	const controllerRef = useRef<AbortController | null>(null);
	const requestIdRef = useRef(0);
	const [query, setQuery] = useState("");
	const [filter, setFilter] = useState<SearchFilter>("all");
	const [categoryFilter, setCategoryFilter] =
		useState<SearchCategoryFilter>(null);
	const [results, setResults] = useState<SearchResults>(EMPTY_RESULTS);
	const [loading, setLoading] = useState(false);
	const [loadingMore, setLoadingMore] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [activeIndex, setActiveIndex] = useState(0);
	const [openingKey, setOpeningKey] = useState<string | null>(null);

	const trimmedQuery = query.trim();
	const hasSearchCriteria = Boolean(trimmedQuery || categoryFilter);
	const searchType: SearchFilter = categoryFilter ? "file" : filter;

	const buildSearchParams = useMemo(
		() =>
			(offset?: number): SearchParams => ({
				...(trimmedQuery ? { q: trimmedQuery } : {}),
				type: searchType,
				...(categoryFilter ? { category: categoryFilter } : {}),
				limit: SEARCH_RESULT_LIMIT,
				...(offset == null ? {} : { offset }),
			}),
		[categoryFilter, searchType, trimmedQuery],
	);

	const resultEntries = useMemo<SearchEntry[]>(
		() => [
			...results.folders.map((item) => ({
				key: `folder-${item.id}`,
				kind: "folder" as const,
				item,
			})),
			...results.files.map((item) => ({
				key: `file-${item.id}`,
				kind: "file" as const,
				item,
			})),
		],
		[results],
	);
	const activeEntry = resultEntries[activeIndex] ?? null;
	const canLoadMore = hasMoreSearchResults(results);

	useEffect(() => {
		if (!open) {
			inputComposingRef.current = false;
			inputCompositionEndAtRef.current = 0;
			controllerRef.current?.abort();
			controllerRef.current = null;
			requestIdRef.current += 1;
			setQuery("");
			setFilter("all");
			setCategoryFilter(null);
			setResults(EMPTY_RESULTS);
			setLoading(false);
			setLoadingMore(false);
			setError(null);
			setActiveIndex(0);
			setOpeningKey(null);
			return;
		}

		const timer = window.setTimeout(() => {
			inputRef.current?.focus();
			inputRef.current?.select();
		}, 0);
		return () => window.clearTimeout(timer);
	}, [open]);

	useEffect(() => {
		if (!open || !initialCategory) {
			return;
		}

		setQuery("");
		setFilter("file");
		setCategoryFilter(initialCategory);
		setResults(EMPTY_RESULTS);
		setActiveIndex(0);
		setLoading(false);
		setLoadingMore(false);
		setError(null);
	}, [initialCategory, open]);

	useEffect(() => {
		if (!open) {
			return;
		}

		if (!hasSearchCriteria) {
			controllerRef.current?.abort();
			controllerRef.current = null;
			requestIdRef.current += 1;
			setResults(EMPTY_RESULTS);
			setLoading(false);
			setLoadingMore(false);
			setError(null);
			setActiveIndex(0);
			return;
		}

		const requestId = requestIdRef.current + 1;
		requestIdRef.current = requestId;
		controllerRef.current?.abort();
		const controller = new AbortController();
		controllerRef.current = controller;
		setLoading(true);
		setLoadingMore(false);
		setError(null);

		const timer = window.setTimeout(() => {
			void searchService
				.search(buildSearchParams(), { signal: controller.signal })
				.then((nextResults) => {
					if (requestIdRef.current !== requestId) {
						return;
					}
					setResults(nextResults);
					setLoading(false);
					setLoadingMore(false);
					setActiveIndex(0);
				})
				.catch((searchError) => {
					if (
						requestIdRef.current !== requestId ||
						isRequestCanceled(searchError)
					) {
						return;
					}
					setResults(EMPTY_RESULTS);
					setLoading(false);
					setLoadingMore(false);
					setError(
						searchError instanceof Error
							? searchError.message
							: t("search:search_error"),
					);
				});
		}, SEARCH_DEBOUNCE_MS);

		return () => {
			window.clearTimeout(timer);
			controller.abort();
			if (
				requestIdRef.current === requestId &&
				controllerRef.current === controller
			) {
				controllerRef.current = null;
			}
		};
	}, [buildSearchParams, hasSearchCriteria, open, t]);

	useEffect(() => {
		if (
			!open ||
			!hasSearchCriteria ||
			!canLoadMore ||
			loading ||
			loadingMore ||
			typeof window === "undefined" ||
			typeof window.IntersectionObserver === "undefined"
		) {
			return;
		}

		const root = resultListRef.current;
		const sentinel = loadMoreRef.current;
		if (!root || !sentinel) {
			return;
		}

		const observer = new window.IntersectionObserver(
			(entries) => {
				if (!entries.some((entry) => entry.isIntersecting)) {
					return;
				}

				const requestId = requestIdRef.current + 1;
				requestIdRef.current = requestId;
				controllerRef.current?.abort();
				const controller = new AbortController();
				controllerRef.current = controller;
				setLoadingMore(true);

				void searchService
					.search(buildSearchParams(getSearchOffset(results)), {
						signal: controller.signal,
					})
					.then((nextResults) => {
						if (requestIdRef.current !== requestId) {
							return;
						}

						setResults((current) => ({
							files: [...current.files, ...nextResults.files],
							folders: [...current.folders, ...nextResults.folders],
							total_files: nextResults.total_files,
							total_folders: nextResults.total_folders,
						}));
						setLoadingMore(false);
					})
					.catch((searchError) => {
						if (
							requestIdRef.current !== requestId ||
							isRequestCanceled(searchError)
						) {
							return;
						}

						setLoadingMore(false);
					})
					.finally(() => {
						if (
							requestIdRef.current === requestId &&
							controllerRef.current === controller
						) {
							controllerRef.current = null;
						}
					});
			},
			{ root, rootMargin: "160px" },
		);

		observer.observe(sentinel);
		return () => observer.disconnect();
	}, [
		buildSearchParams,
		canLoadMore,
		hasSearchCriteria,
		loading,
		loadingMore,
		open,
		results,
	]);

	useEffect(() => {
		if (activeIndex < resultEntries.length) {
			return;
		}
		setActiveIndex(Math.max(0, resultEntries.length - 1));
	}, [activeIndex, resultEntries.length]);

	useEffect(() => {
		if (!open || resultEntries.length === 0) {
			return;
		}
		const activeRow = resultListRef.current?.querySelector<HTMLElement>(
			`[data-search-result-index="${activeIndex}"]`,
		);
		activeRow?.scrollIntoView({ block: "nearest" });
	}, [activeIndex, open, resultEntries.length]);

	const handleSelect = async (entry: SearchEntry) => {
		if (openingKey !== null) {
			return;
		}

		if (entry.kind === "folder") {
			onOpenChange(false);
			navigate(workspaceFolderPath(workspace, entry.item.id, entry.item.name), {
				viewTransition: false,
			});
			return;
		}

		setOpeningKey(entry.key);
		try {
			const fileInfo = await fileService.getFile(entry.item.id);
			const state: SearchPreviewLocationState = {
				searchPreviewFile: entry.item,
			};
			onOpenChange(false);
			navigate(workspaceFolderPath(workspace, fileInfo.folder_id ?? null), {
				state,
				viewTransition: false,
			});
		} catch (selectError) {
			handleApiError(selectError);
		} finally {
			setOpeningKey(null);
		}
	};

	const handleInputKeyDown = async (event: KeyboardEvent<HTMLInputElement>) => {
		if (
			inputComposingRef.current ||
			isImeComposingKeyEvent(event, {
				lastCompositionEndAt: inputCompositionEndAtRef.current,
			})
		) {
			return;
		}

		if (event.key === "ArrowDown") {
			if (resultEntries.length === 0) {
				return;
			}
			event.preventDefault();
			setActiveIndex((current) => (current + 1) % resultEntries.length);
			return;
		}

		if (event.key === "ArrowUp") {
			if (resultEntries.length === 0) {
				return;
			}
			event.preventDefault();
			setActiveIndex((current) =>
				current === 0 ? resultEntries.length - 1 : current - 1,
			);
			return;
		}

		if (event.key === "Enter" && activeEntry) {
			event.preventDefault();
			await handleSelect(activeEntry);
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
					onClose={() => onOpenChange(false)}
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
					onInputKeyDown={(event) => {
						void handleInputKeyDown(event);
					}}
					onQueryChange={setQuery}
					query={query}
				/>
				<GlobalSearchResultsPanel
					activeIndex={activeIndex}
					canLoadMore={canLoadMore}
					error={error}
					loading={loading}
					loadingMore={loadingMore}
					loadMoreRef={loadMoreRef}
					openingKey={openingKey}
					onHoverResult={setActiveIndex}
					onSelectResult={(entry) => {
						void handleSelect(entry);
					}}
					resultEntries={resultEntries}
					resultListRef={resultListRef}
					results={results}
					searchActive={hasSearchCriteria}
				/>
			</DialogContent>
		</Dialog>
	);
}
