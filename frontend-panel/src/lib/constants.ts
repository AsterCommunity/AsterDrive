/** 拖拽操作的自定义 MIME 类型 */
export const DRAG_MIME = "application/x-asterdrive-move";
export const DRAG_SOURCE_MIME = "application/x-asterdrive-drag-source";

/** 顶栏高度（Tailwind spacing token） */
export const USER_TOPBAR_HEIGHT_CLASS = "h-16";
export const USER_TOPBAR_OFFSET_CLASS =
	"top-16 h-[calc(100dvh-4rem)] md:h-auto";

export const ADMIN_TOPBAR_HEIGHT_CLASS = "h-16";
export const ADMIN_TOPBAR_OFFSET_CLASS =
	"top-16 h-[calc(100dvh-4rem)] md:h-auto";

/** 侧栏宽度 */
export const USER_SIDEBAR_WIDTH_CLASS = "w-60 md:w-[var(--user-sidebar-width)]";
export const USER_SIDEBAR_DEFAULT_WIDTH_PX = 240;
export const USER_SIDEBAR_MIN_WIDTH_PX = 220;
export const USER_SIDEBAR_MAX_WIDTH_PX = 420;
export const ADMIN_SIDEBAR_WIDTH_CLASS = "w-60";

/** Admin 页面密度 */
export const ADMIN_CONTROL_HEIGHT_CLASS = "h-8";
export const ADMIN_ICON_BUTTON_CLASS = "size-8";
export const ADMIN_TABLE_ACTIONS_WIDTH_CLASS = "w-24";

/** 侧栏 / 列表内边距 */
export const SIDEBAR_SECTION_PADDING_CLASS = "px-2";
export const PAGE_SECTION_PADDING_CLASS = "px-4 md:px-6";
export const BOTTOM_FLOATING_BAR_PADDING_CLASS =
	"pb-[calc(5.5rem+env(safe-area-inset-bottom))]";
export const BOTTOM_UPLOAD_PANEL_COLLAPSED_PADDING_CLASS =
	"pb-[calc(7rem+env(safe-area-inset-bottom))]";
export const BOTTOM_UPLOAD_PANEL_PADDING_CLASS =
	"pb-[calc(18rem+env(safe-area-inset-bottom))]";
export const MENU_SECTION_PADDING_CLASS = "px-3";

export type BottomOverlayOffset =
	| "none"
	| "selection-compact"
	| "upload-compact"
	| "expanded";

export function getBottomOverlayPaddingClass(offset: BottomOverlayOffset) {
	switch (offset) {
		case "selection-compact":
			return BOTTOM_FLOATING_BAR_PADDING_CLASS;
		case "upload-compact":
			return BOTTOM_UPLOAD_PANEL_COLLAPSED_PADDING_CLASS;
		case "expanded":
			return BOTTOM_UPLOAD_PANEL_PADDING_CLASS;
		case "none":
			return undefined;
	}
}
export const SETTINGS_PAGE_CONTENT_PADDING_CLASS =
	"px-4 pt-4 pb-8 md:px-6 md:pt-6 md:pb-10";
export const ADMIN_SETTINGS_CONTENT_BASE_BOTTOM_PADDING_MOBILE_PX = 32;
export const ADMIN_SETTINGS_CONTENT_BASE_BOTTOM_PADDING_DESKTOP_PX = 48;
export const ADMIN_SETTINGS_SAVE_BAR_MIN_RESERVED_HEIGHT_MOBILE_PX = 152;
export const ADMIN_SETTINGS_SAVE_BAR_MIN_RESERVED_HEIGHT_DESKTOP_PX = 112;

/** FolderTree 视觉节奏 */
export const FOLDER_TREE_INDENT_PX = 16;
export const FOLDER_TREE_ROW_OFFSET_PX = 4;
export const FOLDER_TREE_SKELETON_OFFSET_PX = 8;
export const FOLDER_TREE_DRAG_EXPAND_DELAY_MS = 600;

/** 文件浏览页的局部反馈应当足够快，避免拖沓 */
export const FILE_BROWSER_FEEDBACK_DURATION_MS = 150;

/** 分页默认限制 */
export const FILE_PAGE_SIZE = 100;
export const FOLDER_LIMIT = 1000;
