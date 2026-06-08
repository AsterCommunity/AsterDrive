import { type DragEvent, Fragment, useEffect, useReducer, useRef } from "react";
import { useTranslation } from "react-i18next";
import { SortMenu } from "@/components/common/SortMenu";
import { ToolbarBar } from "@/components/common/ToolbarBar";
import { ViewToggle } from "@/components/common/ViewToggle";
import {
	Breadcrumb,
	BreadcrumbEllipsis,
	BreadcrumbItem,
	BreadcrumbLink,
	BreadcrumbList,
	BreadcrumbPage,
	BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Icon } from "@/components/ui/icon";
import { cn } from "@/lib/utils";
import { CurrentFolderDropdownMenuContent } from "@/pages/file-browser/CurrentFolderActionsMenu";
import type { FileBrowserSelectionToolbarState } from "@/pages/file-browser/types";
import type { SortBy, SortOrder } from "@/stores/fileStore/types";

type SelectionToolbarPhase = "hidden" | "entering" | "visible" | "exiting";

const SELECTION_TOOLBAR_ENTER_MS = 160;
const SELECTION_TOOLBAR_EXIT_DELAY_MS = 40;
const SELECTION_TOOLBAR_EXIT_MS = 120;

function scheduleSelectionToolbarTimer(callback: () => void, delay: number) {
	return setTimeout(callback, delay);
}

type VisibleBreadcrumbEntry =
	| {
			type: "item";
			item: {
				id: number | null;
				name: string;
			};
			sourceIndex: number;
	  }
	| {
			type: "ellipsis";
			key: string;
			items: Array<{
				id: number | null;
				name: string;
			}>;
	  };

interface FileBrowserToolbarProps {
	breadcrumb: Array<{
		id: number | null;
		name: string;
	}>;
	dragOverBreadcrumbIndex: number | null;
	isCompactBreadcrumb: boolean;
	isRootFolder: boolean;
	isSearching: boolean;
	searchQuery: string | null;
	selectionToolbar: FileBrowserSelectionToolbarState | null;
	sortBy: SortBy;
	sortOrder: SortOrder;
	uploadReady: boolean;
	viewMode: "grid" | "list";
	onBreadcrumbDragLeave: (event: DragEvent) => void;
	onBreadcrumbDragOver: (event: DragEvent, index: number) => void;
	onBreadcrumbDrop: (
		event: DragEvent,
		index: number,
		targetFolderId: number | null,
	) => Promise<void>;
	onCreateFile: () => void;
	onCreateFolder: () => void;
	onManageTagLibrary: () => void;
	onNavigateToFolder: (folderId: number | null, folderName: string) => void;
	onOfflineDownload: () => void;
	onRefresh: () => void | Promise<void>;
	onSetSortBy: (value: SortBy) => void;
	onSetSortOrder: (value: SortOrder) => void;
	onSetViewMode: (value: "grid" | "list") => void;
	onTriggerFileUpload: () => void;
	onTriggerFolderUpload: () => void;
}

function useSelectionToolbarMotion(
	selectionToolbar: FileBrowserSelectionToolbarState | null,
) {
	const [state, dispatch] = useReducer(
		(
			current: {
				hasSelection: boolean;
				phase: SelectionToolbarPhase;
			},
			action:
				| {
						type: "set";
						hasSelection: boolean;
				  }
				| {
						type: "enter";
				  }
				| {
						type: "visible";
				  }
				| {
						type: "exiting";
				  }
				| {
						type: "hide";
				  },
		): {
			hasSelection: boolean;
			phase: SelectionToolbarPhase;
		} => {
			switch (action.type) {
				case "set":
					return {
						hasSelection: action.hasSelection,
						phase: action.hasSelection
							? "entering"
							: current.hasSelection
								? "visible"
								: "hidden",
					};
				case "enter":
					return { ...current, phase: "entering" };
				case "visible":
					return { ...current, phase: "visible" };
				case "exiting":
					return { ...current, phase: "exiting" };
				case "hide":
					return { hasSelection: false, phase: "hidden" };
			}
		},
		{
			hasSelection: selectionToolbar !== null,
			phase: selectionToolbar ? "entering" : "hidden",
		},
	);
	const retainedSelectionToolbarRef =
		useRef<FileBrowserSelectionToolbarState | null>(selectionToolbar);
	const enterTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
	const exitTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
	const restoreTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
	const hasSelection = selectionToolbar !== null;
	const hasSelectionRef = useRef(state.hasSelection);
	hasSelectionRef.current = hasSelection;

	if (selectionToolbar) {
		retainedSelectionToolbarRef.current = selectionToolbar;
	}

	useEffect(() => {
		const clearTimers = () => {
			if (enterTimerRef.current) {
				clearTimeout(enterTimerRef.current);
				enterTimerRef.current = null;
			}
			if (exitTimerRef.current) {
				clearTimeout(exitTimerRef.current);
				exitTimerRef.current = null;
			}
			if (restoreTimerRef.current) {
				clearTimeout(restoreTimerRef.current);
				restoreTimerRef.current = null;
			}
		};

		clearTimers();

		if (hasSelection) {
			dispatch({ type: "set", hasSelection: true });
			enterTimerRef.current = scheduleSelectionToolbarTimer(() => {
				dispatch({ type: "visible" });
				enterTimerRef.current = null;
			}, SELECTION_TOOLBAR_ENTER_MS);
		} else if (retainedSelectionToolbarRef.current) {
			dispatch({ type: "set", hasSelection: false });
			exitTimerRef.current = scheduleSelectionToolbarTimer(() => {
				if (hasSelectionRef.current) {
					exitTimerRef.current = null;
					return;
				}
				dispatch({ type: "exiting" });
				exitTimerRef.current = null;
				restoreTimerRef.current = scheduleSelectionToolbarTimer(() => {
					if (hasSelectionRef.current) {
						restoreTimerRef.current = null;
						return;
					}
					retainedSelectionToolbarRef.current = null;
					dispatch({ type: "hide" });
					restoreTimerRef.current = null;
				}, SELECTION_TOOLBAR_EXIT_MS);
			}, SELECTION_TOOLBAR_EXIT_DELAY_MS);
		} else {
			dispatch({ type: "set", hasSelection: false });
		}

		return () => {
			if (enterTimerRef.current) {
				clearTimeout(enterTimerRef.current);
				enterTimerRef.current = null;
			}
			if (exitTimerRef.current) {
				clearTimeout(exitTimerRef.current);
				exitTimerRef.current = null;
			}
			if (restoreTimerRef.current) {
				clearTimeout(restoreTimerRef.current);
				restoreTimerRef.current = null;
			}
		};
	}, [hasSelection]);

	const renderedSelectionToolbar =
		selectionToolbar ?? retainedSelectionToolbarRef.current;
	const renderedPhase: SelectionToolbarPhase = selectionToolbar
		? state.phase === "hidden" || state.phase === "exiting"
			? "entering"
			: state.phase
		: renderedSelectionToolbar
			? state.phase === "exiting"
				? "exiting"
				: "visible"
			: "hidden";

	return {
		phase: renderedPhase,
		selectionToolbar: renderedSelectionToolbar,
	};
}

function selectionToolbarMotionClass(phase: SelectionToolbarPhase) {
	return cn(
		"will-change-[opacity] motion-reduce:animate-none",
		phase === "entering" && "animate-in fade-in duration-[120ms] ease-out",
		phase === "visible" && "opacity-100",
		phase === "exiting" &&
			"pointer-events-none animate-out fade-out duration-[120ms] ease-in",
	);
}

function FileBrowserSelectionToolbar({
	renderedSelectionToolbar,
	selectionToolbarContentClass,
	selectionToolbarHiddenProps,
}: {
	renderedSelectionToolbar: FileBrowserSelectionToolbarState;
	selectionToolbarContentClass: string;
	selectionToolbarHiddenProps: {
		"aria-hidden": boolean;
		inert: true | undefined;
	};
}) {
	const { t } = useTranslation(["files", "tasks"]);
	const selectDisplayedLabel = renderedSelectionToolbar.allDisplayedSelected
		? t("selection_clear")
		: t("selection_select_all_visible");
	const selectionDownloadLabel =
		renderedSelectionToolbar.downloadAction?.kind === "file"
			? t("download")
			: t("tasks:archive_download_action");

	return (
		<ToolbarBar
			left={
				<div
					data-testid="file-browser-selection-toolbar"
					{...selectionToolbarHiddenProps}
					className={cn(
						"flex min-w-0 flex-1 items-center gap-1.5 sm:gap-2",
						selectionToolbarContentClass,
					)}
				>
					<Button
						type="button"
						variant="ghost"
						size="icon-sm"
						className="size-7 shrink-0 sm:h-8 sm:w-8"
						onClick={renderedSelectionToolbar.onClearSelection}
						aria-label={t("selection_clear")}
						title={t("selection_clear")}
					>
						<Icon name="X" className="size-4" />
					</Button>
					<div className="flex min-w-0 flex-1 items-center gap-2">
						<span className="truncate text-sm font-semibold text-foreground">
							{t("core:selected_count", {
								count: renderedSelectionToolbar.count,
							})}
						</span>
						<button
							type="button"
							className="hidden rounded-md px-2 py-1 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent/55 hover:text-foreground disabled:pointer-events-none disabled:opacity-45 sm:inline-flex"
							onClick={renderedSelectionToolbar.onToggleDisplayedSelection}
							disabled={!renderedSelectionToolbar.hasDisplayedItems}
						>
							{selectDisplayedLabel}
						</button>
					</div>
				</div>
			}
			right={
				<div
					{...selectionToolbarHiddenProps}
					className={cn(
						"flex items-center gap-1 sm:gap-2",
						selectionToolbarContentClass,
					)}
				>
					{renderedSelectionToolbar.downloadAction ? (
						<Button
							type="button"
							size="sm"
							variant="outline"
							className="hidden md:inline-flex"
							onClick={renderedSelectionToolbar.downloadAction.onClick}
						>
							<Icon name="Download" className="size-3.5" />
							<span>{selectionDownloadLabel}</span>
						</Button>
					) : null}
					<Button
						type="button"
						size="sm"
						variant="outline"
						className="hidden sm:inline-flex"
						onClick={renderedSelectionToolbar.onManageTags}
					>
						<Icon name="Tag" className="size-3.5" />
						<span>{t("tag_manage")}</span>
					</Button>
					<Button
						type="button"
						size="sm"
						variant="outline"
						onClick={renderedSelectionToolbar.onMove}
						aria-label={t("move_to")}
						title={t("move_to")}
					>
						<Icon name="ArrowsOutCardinal" className="size-3.5" />
						<span className="hidden min-[420px]:inline">{t("move_to")}</span>
					</Button>
					<Button
						type="button"
						size="sm"
						variant="outline"
						className="hidden sm:inline-flex"
						onClick={renderedSelectionToolbar.onCopy}
					>
						<Icon name="Copy" className="size-3.5" />
						<span>{t("copy_to")}</span>
					</Button>
					<DropdownMenu>
						<DropdownMenuTrigger
							render={
								<Button
									type="button"
									variant="ghost"
									size="icon-sm"
									aria-label={t("selection_more_actions")}
									title={t("selection_more_actions")}
								>
									<Icon name="DotsThree" className="size-4" />
								</Button>
							}
						/>
						<DropdownMenuContent align="end" className="w-auto min-w-44">
							<DropdownMenuItem
								className="sm:hidden"
								disabled={!renderedSelectionToolbar.hasDisplayedItems}
								onClick={renderedSelectionToolbar.onToggleDisplayedSelection}
							>
								<Icon name="Check" className="size-4 text-muted-foreground" />
								{selectDisplayedLabel}
							</DropdownMenuItem>
							{renderedSelectionToolbar.downloadAction ? (
								<DropdownMenuItem
									className="md:hidden"
									onClick={renderedSelectionToolbar.downloadAction.onClick}
								>
									<Icon
										name="Download"
										className="size-4 text-muted-foreground"
									/>
									{selectionDownloadLabel}
								</DropdownMenuItem>
							) : null}
							<DropdownMenuItem
								className="sm:hidden"
								onClick={renderedSelectionToolbar.onCopy}
							>
								<Icon name="Copy" className="size-4 text-muted-foreground" />
								{t("copy_to")}
							</DropdownMenuItem>
							<DropdownMenuItem
								className="sm:hidden"
								onClick={renderedSelectionToolbar.onManageTags}
							>
								<Icon name="Tag" className="size-4 text-muted-foreground" />
								{t("tag_manage")}
							</DropdownMenuItem>
							{renderedSelectionToolbar.onArchiveCompress ? (
								<DropdownMenuItem
									onClick={renderedSelectionToolbar.onArchiveCompress}
								>
									<Icon
										name="FileZip"
										className="size-4 text-muted-foreground"
									/>
									{t("tasks:archive_compress_action")}
								</DropdownMenuItem>
							) : null}
							<DropdownMenuSeparator />
							<DropdownMenuItem
								variant="destructive"
								onClick={renderedSelectionToolbar.onDelete}
							>
								<Icon name="Trash" className="size-4" />
								{t("core:delete")}
							</DropdownMenuItem>
						</DropdownMenuContent>
					</DropdownMenu>
				</div>
			}
		/>
	);
}

export function FileBrowserToolbar({
	breadcrumb,
	dragOverBreadcrumbIndex,
	isCompactBreadcrumb,
	isRootFolder,
	isSearching,
	searchQuery,
	selectionToolbar,
	sortBy,
	sortOrder,
	uploadReady,
	viewMode,
	onBreadcrumbDragLeave,
	onBreadcrumbDragOver,
	onBreadcrumbDrop,
	onCreateFile,
	onCreateFolder,
	onManageTagLibrary,
	onNavigateToFolder,
	onOfflineDownload,
	onRefresh,
	onSetSortBy,
	onSetSortOrder,
	onSetViewMode,
	onTriggerFileUpload,
	onTriggerFolderUpload,
}: FileBrowserToolbarProps) {
	const { t } = useTranslation(["files", "tasks"]);
	const {
		phase: selectionToolbarPhase,
		selectionToolbar: renderedSelectionToolbar,
	} = useSelectionToolbarMotion(selectionToolbar);
	const visibleBreadcrumb: VisibleBreadcrumbEntry[] =
		isCompactBreadcrumb && breadcrumb.length > 2
			? [
					{ type: "item", item: breadcrumb[0], sourceIndex: 0 },
					{
						type: "ellipsis",
						key: "ellipsis",
						items: breadcrumb.slice(1, -1),
					},
					{
						type: "item",
						item: breadcrumb[breadcrumb.length - 1],
						sourceIndex: breadcrumb.length - 1,
					},
				]
			: breadcrumb.map((item, index) => ({
					type: "item" as const,
					item,
					sourceIndex: index,
				}));
	const selectionToolbarContentClass = selectionToolbarMotionClass(
		selectionToolbarPhase,
	);
	const isSelectionToolbarExiting = selectionToolbarPhase === "exiting";
	const selectionToolbarHiddenProps = {
		"aria-hidden": isSelectionToolbarExiting,
		inert: isSelectionToolbarExiting ? (true as const) : undefined,
	};
	const defaultLeft = (
		<>
			<span className="flex size-7 shrink-0 items-center justify-center rounded-lg bg-accent/55 text-accent-foreground sm:h-8 sm:w-8">
				<Icon name={isRootFolder ? "House" : "FolderOpen"} className="size-4" />
			</span>
			<div className="min-w-0 flex-1">
				{isSearching ? (
					<span className="block truncate text-xs text-muted-foreground sm:text-sm">
						{t("core:search")}: &quot;{searchQuery}&quot;
					</span>
				) : (
					<Breadcrumb className="min-w-0">
						<BreadcrumbList className="min-w-0 gap-1.5 text-xs sm:gap-2 sm:text-sm">
							{visibleBreadcrumb.map((entry, index) => (
								<Fragment
									key={
										entry.type === "ellipsis"
											? entry.key
											: `${entry.item.id ?? "root"}-${entry.sourceIndex}`
									}
								>
									{index > 0 && (
										<BreadcrumbSeparator className="mx-0.5 text-muted-foreground/45" />
									)}
									{entry.type === "ellipsis" ? (
										<BreadcrumbItem className="shrink-0">
											<DropdownMenu>
												<DropdownMenuTrigger
													render={
														<button
															type="button"
															className="flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent/70 hover:text-foreground sm:h-7 sm:w-7"
															aria-label={t("core:more")}
														>
															<BreadcrumbEllipsis />
														</button>
													}
												/>
												<DropdownMenuContent
													align="start"
													className="w-auto min-w-40"
												>
													{entry.items.map((hiddenItem) => (
														<DropdownMenuItem
															key={hiddenItem.id ?? "root"}
															onClick={() =>
																onNavigateToFolder(
																	hiddenItem.id,
																	hiddenItem.name,
																)
															}
														>
															<Icon
																name="FolderOpen"
																className="size-4 text-muted-foreground"
															/>
															<span className="truncate">
																{hiddenItem.name}
															</span>
														</DropdownMenuItem>
													))}
												</DropdownMenuContent>
											</DropdownMenu>
										</BreadcrumbItem>
									) : (
										<BreadcrumbItem
											className={
												entry.sourceIndex === breadcrumb.length - 1
													? "min-w-0 flex-1"
													: "shrink-0"
											}
										>
											{entry.sourceIndex < breadcrumb.length - 1 ? (
												<BreadcrumbLink
													className={[
														"cursor-pointer rounded-md px-1 py-0.5 text-[13px] text-muted-foreground transition-colors hover:bg-accent/45 hover:text-foreground sm:px-1.5 sm:text-sm",
														dragOverBreadcrumbIndex === entry.sourceIndex &&
															"ring-2 ring-primary bg-accent/30 text-foreground",
													]
														.filter(Boolean)
														.join(" ")}
													onDragOver={(event) =>
														onBreadcrumbDragOver(event, entry.sourceIndex)
													}
													onDragLeave={onBreadcrumbDragLeave}
													onDrop={(event) => {
														void onBreadcrumbDrop(
															event,
															entry.sourceIndex,
															entry.item.id,
														);
													}}
													onClick={() =>
														onNavigateToFolder(entry.item.id, entry.item.name)
													}
												>
													{entry.item.name}
												</BreadcrumbLink>
											) : (
												<BreadcrumbPage className="text-sm font-semibold text-foreground sm:text-[0.95rem]">
													{entry.item.name}
												</BreadcrumbPage>
											)}
										</BreadcrumbItem>
									)}
								</Fragment>
							))}
						</BreadcrumbList>
					</Breadcrumb>
				)}
			</div>
			<button
				type="button"
				className="flex size-7 shrink-0 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-accent/55 hover:text-accent-foreground sm:h-8 sm:w-8"
				onClick={() => void onRefresh()}
				aria-label={t("core:refresh")}
				title={t("core:refresh")}
			>
				<Icon name="ArrowsClockwise" className="size-4" />
			</button>
		</>
	);
	const defaultRight = (
		<>
			<DropdownMenu>
				<DropdownMenuTrigger
					render={
						<Button
							type="button"
							size="icon-sm"
							variant="ghost"
							className="sm:hidden"
							aria-label={t("folder_more_actions")}
							title={t("folder_more_actions")}
						>
							<Icon name="DotsThree" className="size-4" />
						</Button>
					}
				/>
				<CurrentFolderDropdownMenuContent
					uploadReady={uploadReady}
					onCreateFile={onCreateFile}
					onCreateFolder={onCreateFolder}
					onManageTagLibrary={onManageTagLibrary}
					onOfflineDownload={onOfflineDownload}
					onRefresh={onRefresh}
					onTriggerFileUpload={onTriggerFileUpload}
					onTriggerFolderUpload={onTriggerFolderUpload}
				/>
			</DropdownMenu>
			<Button
				type="button"
				size="sm"
				variant="outline"
				className="hidden md:inline-flex"
				onClick={onManageTagLibrary}
			>
				<Icon name="Tag" className="size-3.5" />
				<span>{t("tag_library_manage")}</span>
			</Button>
			<SortMenu
				sortBy={sortBy}
				sortOrder={sortOrder}
				onSortBy={onSetSortBy}
				onSortOrder={onSetSortOrder}
			/>
			<ViewToggle value={viewMode} onChange={onSetViewMode} />
		</>
	);

	if (!renderedSelectionToolbar) {
		return <ToolbarBar left={defaultLeft} right={defaultRight} />;
	}

	return (
		<FileBrowserSelectionToolbar
			renderedSelectionToolbar={renderedSelectionToolbar}
			selectionToolbarContentClass={selectionToolbarContentClass}
			selectionToolbarHiddenProps={selectionToolbarHiddenProps}
		/>
	);
}
