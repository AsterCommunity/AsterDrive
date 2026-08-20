import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}

/**
 * D5 侧栏分组标题：11px uppercase 三级文案，「快速查看」「功能入口」共用。
 * 目录树作为主内容区保持无标题，分组职责主要由间距承担。
 */
export const SIDEBAR_SECTION_TITLE_CLASS =
	"px-3 py-1 text-[11px] font-medium uppercase tracking-wider text-muted-foreground/80";

/**
 * D5 选中指示条：active 项左缘的 2px primary 竖条，叠加在 bg-accent 色块上，
 * 暗色下选中辨识度不再依赖弱对比色块。用户侧栏与管理后台导航统一启用。
 */
const SIDEBAR_ACTIVE_INDICATOR_CLASS =
	"relative before:absolute before:left-1 before:top-1/2 before:h-4 before:w-0.5 before:-translate-y-1/2 before:rounded-full before:bg-primary before:content-['']";

interface SidebarSelectionOpts {
	/** 选中态是否追加 D5 左侧指示条。 */
	indicator?: boolean;
}

export function sidebarNavItemClass(
	isActive: boolean,
	extra?: ClassValue,
	opts?: SidebarSelectionOpts,
) {
	return cn(
		"flex select-none items-center gap-2 rounded-lg px-3 py-2 text-sm transition-[background-color,color,box-shadow]",
		isActive
			? cn(
					"bg-accent text-accent-foreground font-medium",
					opts?.indicator && SIDEBAR_ACTIVE_INDICATOR_CLASS,
				)
			: "text-muted-foreground hover:bg-accent/45 hover:text-foreground",
		extra,
	);
}

export function folderTreeRowClass(
	isActive: boolean,
	extra?: ClassValue,
	opts?: SidebarSelectionOpts,
) {
	return cn(
		"flex w-full cursor-pointer items-center gap-1 rounded-lg px-2 py-1.5 text-left text-sm transition-[background-color,color,box-shadow] hover:bg-accent/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:ring-offset-2",
		isActive
			? cn(
					"bg-accent text-accent-foreground font-medium",
					opts?.indicator && SIDEBAR_ACTIVE_INDICATOR_CLASS,
				)
			: "text-foreground",
		extra,
	);
}
