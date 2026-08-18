import type { StateCreator } from "zustand";
import type {
	BatchResult,
	FileListItem,
	FolderContents,
	FolderListItem,
} from "@/types/api";
import type { SelectionItemKey } from "./selectionRange";

export interface BreadcrumbItem {
	id: number | null;
	name: string;
}

export interface Clipboard {
	fileIds: number[];
	folderIds: number[];
	mode: "copy" | "cut";
}

export const VIEW_MODES = ["grid", "list"] as const;
export const BROWSER_OPEN_MODES = ["single_click", "double_click"] as const;
export const SORT_BY_VALUES = [
	"name",
	"size",
	"created_at",
	"updated_at",
	"type",
] as const;
export const SORT_ORDER_VALUES = ["asc", "desc"] as const;

export type ViewMode = (typeof VIEW_MODES)[number];
export type BrowserOpenMode = (typeof BROWSER_OPEN_MODES)[number];
export type SortBy = (typeof SORT_BY_VALUES)[number];
export type SortOrder = (typeof SORT_ORDER_VALUES)[number];

function isStringUnionValue<T extends string>(
	value: unknown,
	values: readonly T[],
): value is T {
	return typeof value === "string" && values.includes(value as T);
}

export function isViewMode(value: unknown): value is ViewMode {
	return isStringUnionValue(value, VIEW_MODES);
}

export function isBrowserOpenMode(value: unknown): value is BrowserOpenMode {
	return isStringUnionValue(value, BROWSER_OPEN_MODES);
}

export function isSortBy(value: unknown): value is SortBy {
	return isStringUnionValue(value, SORT_BY_VALUES);
}

export function isSortOrder(value: unknown): value is SortOrder {
	return isStringUnionValue(value, SORT_ORDER_VALUES);
}

export function normalizeViewMode(
	value: unknown,
	fallback: ViewMode,
): ViewMode {
	return isViewMode(value) ? value : fallback;
}

export function normalizeBrowserOpenMode(
	value: unknown,
	fallback: BrowserOpenMode,
): BrowserOpenMode {
	return isBrowserOpenMode(value) ? value : fallback;
}

export function normalizeSortBy(value: unknown, fallback: SortBy): SortBy {
	return isSortBy(value) ? value : fallback;
}

export function normalizeSortOrder(
	value: unknown,
	fallback: SortOrder,
): SortOrder {
	return isSortOrder(value) ? value : fallback;
}

export interface RequestSlice {
	resetWorkspaceState: () => void;
	lastFolderContents: {
		folderId: number | null;
		folders: FolderListItem[];
		sortBy: SortBy;
		sortOrder: SortOrder;
		workspaceRevision: number;
	} | null;
	workspaceRequestRevision: number;
	_workspaceRequestId: number;
	_workspaceRequestController: AbortController | null;
}

export interface NavigationSlice {
	currentFolderId: number | null;
	breadcrumb: BreadcrumbItem[];
	folders: FolderListItem[];
	files: FileListItem[];
	loading: boolean;
	error: string | null;
	filesTotalCount: number;
	foldersTotalCount: number;
	loadingMore: boolean;
	nextFileCursor: FolderContents["next_file_cursor"];
	navigateTo: (
		folderId: number | null,
		folderName?: string,
		breadcrumbPath?: BreadcrumbItem[],
	) => Promise<void>;
	refresh: () => Promise<void>;
	loadMoreFiles: () => Promise<void>;
	hasMoreFiles: () => boolean;
}

export interface PreferencesSlice {
	viewMode: ViewMode;
	browserOpenMode: BrowserOpenMode;
	sortBy: SortBy;
	sortOrder: SortOrder;
	setViewMode: (mode: ViewMode) => void;
	setBrowserOpenMode: (mode: BrowserOpenMode) => void;
	setSortBy: (sortBy: SortBy) => void;
	setSortOrder: (sortOrder: SortOrder) => void;
	_applyFromServer: (prefs: {
		viewMode?: unknown;
		browserOpenMode?: unknown;
		sortBy?: unknown;
		sortOrder?: unknown;
	}) => void;
}

export interface SelectionSlice {
	selectedFileIds: Set<number>;
	selectedFolderIds: Set<number>;
	/** 范围选择的锚点（普通点击 / Cmd+点击设定）；条目消失时惰性失效 */
	selectionAnchor: SelectionItemKey | null;
	/** 键盘导航的当前焦点项 */
	selectionFocus: SelectionItemKey | null;
	/** 上一次 Shift+点击并入选择的暂存项，下次 Shift+点击前先撤出 */
	shiftRangeFileIds: Set<number>;
	shiftRangeFolderIds: Set<number>;
	toggleFileSelection: (id: number) => void;
	toggleFolderSelection: (id: number) => void;
	selectOnlyFile: (id: number) => void;
	selectOnlyFolder: (id: number) => void;
	selectItems: (fileIds: number[], folderIds: number[]) => void;
	/** Shift+点击：撤出上次暂存范围后，从锚点并入到目标的新范围（Finder 连续重选终点语义） */
	selectRangeTo: (type: "file" | "folder", id: number) => void;
	/** 方向键：移动焦点并单选，锚点跟随 */
	moveSelectionBy: (delta: number) => void;
	/** Shift+方向键：锚点固定，扩展选择到移动后的焦点 */
	extendSelectionBy: (delta: number) => void;
	selectAll: () => void;
	clearSelection: () => void;
	selectionCount: () => number;
}

export interface ClipboardSlice {
	clipboard: Clipboard | null;
	clipboardCopy: () => number;
	clipboardCut: () => number;
	clipboardPaste: () => Promise<{ mode: "copy" | "cut"; result: BatchResult }>;
	clearClipboard: () => void;
}

export interface CrudSlice {
	createFile: (name: string) => Promise<void>;
	createFolder: (name: string) => Promise<void>;
	deleteFile: (id: number) => Promise<void>;
	deleteFolder: (id: number) => Promise<void>;
	moveToFolder: (
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number | null,
	) => Promise<BatchResult>;
}

export type FileState = RequestSlice &
	NavigationSlice &
	PreferencesSlice &
	SelectionSlice &
	ClipboardSlice &
	CrudSlice;

export type FileStoreSlice<T> = StateCreator<FileState, [], [], T>;
export type FileStoreGet = () => FileState;
export type FileStoreSet = (
	partial: Partial<FileState> | ((state: FileState) => Partial<FileState>),
) => void;

export function createRootBreadcrumb(): BreadcrumbItem[] {
	return [{ id: null, name: "Root" }];
}

export function createWorkspaceContentReset() {
	return {
		folders: [] as FolderListItem[],
		files: [] as FileListItem[],
		filesTotalCount: 0,
		foldersTotalCount: 0,
		loadingMore: false,
		nextFileCursor: null as FolderContents["next_file_cursor"],
	};
}

export function createSelectionReset() {
	return {
		selectedFileIds: new Set<number>(),
		selectedFolderIds: new Set<number>(),
	};
}

export function createWorkspaceResetState() {
	return {
		currentFolderId: null as number | null,
		breadcrumb: createRootBreadcrumb(),
		loading: false,
		error: null as string | null,
		clipboard: null as Clipboard | null,
		...createWorkspaceContentReset(),
		...createSelectionReset(),
	};
}
