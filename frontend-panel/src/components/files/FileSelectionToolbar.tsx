import { type ReactNode, useEffect, useReducer, useRef } from "react";
import { useTranslation } from "react-i18next";
import { ToolbarBar } from "@/components/common/ToolbarBar";
import type { FileBrowserSelectionDownloadAction } from "@/components/files/FileBrowserContext";
import {
	BUILTIN_FILE_SELECTION_ACTION_DESCRIPTORS,
	type FileActionId,
	type ResolvedFileAction,
	resolveFileActions,
} from "@/components/files/fileActionRegistry";
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

export interface FileSelectionToolbarState {
	count: number;
	allDisplayedSelected: boolean;
	downloadAction?: FileBrowserSelectionDownloadAction;
	hasDisplayedItems: boolean;
	onArchiveCompress?: () => void;
	onClearSelection: () => void;
	onCopy?: () => void;
	onDelete?: () => void;
	onManageTags?: () => void;
	onMove?: () => void;
	onToggleDisplayedSelection: () => void;
}

type SelectionToolbarPhase = "hidden" | "entering" | "visible" | "exiting";

const SELECTION_TOOLBAR_ENTER_MS = 160;
const SELECTION_TOOLBAR_EXIT_DELAY_MS = 40;
const SELECTION_TOOLBAR_EXIT_MS = 120;

function scheduleSelectionToolbarTimer(callback: () => void, delay: number) {
	return setTimeout(callback, delay);
}

function useSelectionToolbarMotion(
	selectionToolbar: FileSelectionToolbarState | null,
) {
	const [state, dispatch] = useReducer(
		(
			current: {
				hasSelection: boolean;
				phase: SelectionToolbarPhase;
			},
			action:
				| { type: "set"; hasSelection: boolean }
				| { type: "visible" }
				| { type: "exiting" }
				| { type: "hide" },
		) => {
			switch (action.type) {
				case "set":
					return {
						hasSelection: action.hasSelection,
						phase: action.hasSelection
							? ("entering" as const)
							: current.hasSelection
								? ("visible" as const)
								: ("hidden" as const),
					};
				case "visible":
					return { ...current, phase: "visible" as const };
				case "exiting":
					return { ...current, phase: "exiting" as const };
				case "hide":
					return { hasSelection: false, phase: "hidden" as const };
			}
		},
		{
			hasSelection: selectionToolbar !== null,
			phase: selectionToolbar ? "entering" : "hidden",
		},
	);
	const retainedSelectionToolbarRef = useRef<FileSelectionToolbarState | null>(
		selectionToolbar,
	);
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

		return clearTimers;
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

function resolveSelectionToolbarActions(
	selectionToolbar: FileSelectionToolbarState,
) {
	return resolveFileActions(BUILTIN_FILE_SELECTION_ACTION_DESCRIPTORS, {
		downloadAction: selectionToolbar.downloadAction,
		handlers: {
			archive_compress: selectionToolbar.onArchiveCompress,
			copy: selectionToolbar.onCopy,
			delete: selectionToolbar.onDelete,
			manage_tags: selectionToolbar.onManageTags,
			move: selectionToolbar.onMove,
		} satisfies Partial<Record<FileActionId, () => void>>,
		isFolder: false,
		isLocked: false,
		selectionCount: selectionToolbar.count,
	});
}

function SelectionActionsMenu({
	actions,
	selectionToolbar,
	selectDisplayedLabel,
}: {
	actions: ResolvedFileAction[];
	selectionToolbar: FileSelectionToolbarState;
	selectDisplayedLabel: string;
}) {
	const { t } = useTranslation(["files", "tasks"]);
	const primaryActions = actions.filter(
		(action) => action.presentation.group !== "danger",
	);
	const dangerActions = actions.filter(
		(action) => action.presentation.group === "danger",
	);

	return (
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
					disabled={!selectionToolbar.hasDisplayedItems}
					onClick={selectionToolbar.onToggleDisplayedSelection}
				>
					<Icon name="Check" className="size-4 text-muted-foreground" />
					{selectDisplayedLabel}
				</DropdownMenuItem>
				{primaryActions.map((action) => (
					<DropdownMenuItem key={action.id} onClick={action.onClick}>
						<Icon name={action.icon} className="size-4 text-muted-foreground" />
						{t(action.labelKey)}
					</DropdownMenuItem>
				))}
				{dangerActions.length > 0 ? (
					<>
						<DropdownMenuSeparator />
						{dangerActions.map((action) => (
							<DropdownMenuItem
								key={action.id}
								variant="destructive"
								onClick={action.onClick}
							>
								<Icon name={action.icon} className="size-4" />
								{t(action.labelKey)}
							</DropdownMenuItem>
						))}
					</>
				) : null}
			</DropdownMenuContent>
		</DropdownMenu>
	);
}

function FileSelectionToolbarContent({
	selectionToolbar,
	motionClassName,
	hiddenProps,
}: {
	selectionToolbar: FileSelectionToolbarState;
	motionClassName: string;
	hiddenProps: {
		"aria-hidden": boolean;
		inert: true | undefined;
	};
}) {
	const { t } = useTranslation(["files", "tasks"]);
	const selectionActions = resolveSelectionToolbarActions(selectionToolbar);
	const selectionActionById = new Map(
		selectionActions.map((action) => [action.id, action] as const),
	);
	const downloadAction = selectionActionById.get("download");
	const manageTagsAction = selectionActionById.get("manage_tags");
	const moveAction = selectionActionById.get("move");
	const copyAction = selectionActionById.get("copy");
	const selectDisplayedLabel = selectionToolbar.allDisplayedSelected
		? t("selection_clear")
		: t("selection_select_all_visible");

	return (
		<>
			<div
				{...hiddenProps}
				className={cn(
					"absolute inset-x-0 top-0 z-10 hidden bg-card sm:block",
					motionClassName,
				)}
			>
				<ToolbarBar
					left={
						<div
							data-testid="file-browser-selection-toolbar"
							{...hiddenProps}
							className="flex min-w-0 flex-1 items-center gap-1.5 sm:gap-2"
						>
							<Button
								type="button"
								variant="ghost"
								size="icon-sm"
								className="size-7 shrink-0 sm:h-8 sm:w-8"
								onClick={selectionToolbar.onClearSelection}
								aria-label={t("selection_clear")}
								title={t("selection_clear")}
							>
								<Icon name="X" className="size-4" />
							</Button>
							<div className="flex min-w-0 flex-1 items-center gap-2">
								<span className="truncate text-sm font-semibold text-foreground">
									{t("core:selected_count", { count: selectionToolbar.count })}
								</span>
								<button
									type="button"
									className="hidden rounded-md px-2 py-1 text-xs font-medium text-muted-foreground transition-colors hover:bg-accent/55 hover:text-foreground disabled:pointer-events-none disabled:opacity-45 sm:inline-flex"
									onClick={selectionToolbar.onToggleDisplayedSelection}
									disabled={!selectionToolbar.hasDisplayedItems}
								>
									{selectDisplayedLabel}
								</button>
							</div>
						</div>
					}
					right={
						<div {...hiddenProps} className="flex items-center gap-1 sm:gap-2">
							{downloadAction ? (
								<Button
									type="button"
									size="sm"
									variant="outline"
									className="hidden md:inline-flex"
									onClick={downloadAction.onClick}
								>
									<Icon name={downloadAction.icon} className="size-3.5" />
									<span>{t(downloadAction.labelKey)}</span>
								</Button>
							) : null}
							{manageTagsAction ? (
								<Button
									type="button"
									size="sm"
									variant="outline"
									onClick={manageTagsAction.onClick}
								>
									<Icon name={manageTagsAction.icon} className="size-3.5" />
									<span>{t(manageTagsAction.labelKey)}</span>
								</Button>
							) : null}
							{moveAction ? (
								<Button
									type="button"
									size="sm"
									variant="outline"
									onClick={moveAction.onClick}
									aria-label={t(moveAction.labelKey)}
									title={t(moveAction.labelKey)}
								>
									<Icon name={moveAction.icon} className="size-3.5" />
									<span className="hidden min-[420px]:inline">
										{t(moveAction.labelKey)}
									</span>
								</Button>
							) : null}
							{copyAction ? (
								<Button
									type="button"
									size="sm"
									variant="outline"
									onClick={copyAction.onClick}
								>
									<Icon name={copyAction.icon} className="size-3.5" />
									<span>{t(copyAction.labelKey)}</span>
								</Button>
							) : null}
							<SelectionActionsMenu
								actions={selectionActions}
								selectionToolbar={selectionToolbar}
								selectDisplayedLabel={selectDisplayedLabel}
							/>
						</div>
					}
				/>
			</div>
			<div
				data-testid="file-browser-mobile-selection-toolbar"
				{...hiddenProps}
				className={cn(
					"fixed right-3 bottom-3 left-3 z-(--z-fixed) flex min-h-14 items-center gap-2 rounded-xl border border-border/70 bg-card/95 px-3 py-2 shadow-lg shadow-black/8 backdrop-blur supports-[backdrop-filter]:bg-card/85 dark:shadow-none sm:hidden",
					motionClassName,
				)}
			>
				<Button
					type="button"
					variant="ghost"
					size="icon-sm"
					className="size-10 shrink-0 rounded-lg"
					onClick={selectionToolbar.onClearSelection}
					aria-label={t("selection_clear")}
					title={t("selection_clear")}
				>
					<Icon name="X" className="size-4" />
				</Button>
				<div className="min-w-0 flex-1">
					<div className="truncate text-sm font-semibold text-foreground">
						{t("core:selected_count", { count: selectionToolbar.count })}
					</div>
					<button
						type="button"
						className="mt-0.5 truncate text-xs font-medium text-muted-foreground disabled:pointer-events-none disabled:opacity-45"
						onClick={selectionToolbar.onToggleDisplayedSelection}
						disabled={!selectionToolbar.hasDisplayedItems}
					>
						{selectDisplayedLabel}
					</button>
				</div>
				<div className="flex shrink-0 items-center gap-1">
					{downloadAction ? (
						<Button
							type="button"
							size="icon-sm"
							variant="outline"
							onClick={downloadAction.onClick}
							aria-label={t(downloadAction.labelKey)}
							title={t(downloadAction.labelKey)}
						>
							<Icon name={downloadAction.icon} className="size-3.5" />
						</Button>
					) : null}
					{moveAction ? (
						<Button
							type="button"
							size="icon-sm"
							variant="outline"
							onClick={moveAction.onClick}
							aria-label={t(moveAction.labelKey)}
							title={t(moveAction.labelKey)}
						>
							<Icon name={moveAction.icon} className="size-3.5" />
						</Button>
					) : null}
					<SelectionActionsMenu
						actions={selectionActions}
						selectionToolbar={selectionToolbar}
						selectDisplayedLabel={selectDisplayedLabel}
					/>
				</div>
			</div>
		</>
	);
}

export function FileSelectionToolbarTransition({
	defaultToolbar,
	selectionToolbar,
}: {
	defaultToolbar: ReactNode;
	selectionToolbar: FileSelectionToolbarState | null;
}) {
	const { phase, selectionToolbar: renderedSelectionToolbar } =
		useSelectionToolbarMotion(selectionToolbar);
	const motionClassName = selectionToolbarMotionClass(phase);
	const isSelectionToolbarExiting = phase === "exiting";
	const selectionToolbarHiddenProps = {
		"aria-hidden": isSelectionToolbarExiting,
		inert: isSelectionToolbarExiting ? (true as const) : undefined,
	};
	const shouldHideDefaultToolbar =
		renderedSelectionToolbar !== null && !isSelectionToolbarExiting;
	const defaultToolbarHiddenProps = {
		"aria-hidden": shouldHideDefaultToolbar,
		inert: shouldHideDefaultToolbar ? (true as const) : undefined,
	};

	return (
		<div className="relative">
			<div
				data-testid="file-browser-default-toolbar"
				{...defaultToolbarHiddenProps}
			>
				{defaultToolbar}
			</div>
			{renderedSelectionToolbar ? (
				<FileSelectionToolbarContent
					selectionToolbar={renderedSelectionToolbar}
					motionClassName={motionClassName}
					hiddenProps={selectionToolbarHiddenProps}
				/>
			) : null}
		</div>
	);
}
