import type {
	DragEvent,
	KeyboardEvent as ReactKeyboardEvent,
	PointerEvent as ReactPointerEvent,
} from "react";
import type { IconName } from "@/components/ui/icon";
import type { InternalDragData } from "@/lib/dragDrop";
import type { Workspace } from "@/lib/workspace";
import type { FileCategory, TeamInfo, UserInfo } from "@/types/api";

export type SidebarResizeHandleElement = HTMLDivElement;

export interface SidebarProps {
	mobileOpen: boolean;
	onMobileClose: () => void;
	onTrashDrop?: (data: InternalDragData) => void | Promise<void>;
	onMoveToFolder?: (
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number | null,
	) => Promise<void> | void;
	onSearchCategoryOpen?: (category: FileCategory) => void;
}

export interface SidebarNavLink {
	icon: IconName;
	label: string;
	to: string;
}

export interface SidebarContentProps
	extends Pick<
		SidebarProps,
		"onMobileClose" | "onMoveToFolder" | "onSearchCategoryOpen"
	> {
	activeTeam: TeamInfo | null;
	locationPathname: string;
	navLinks: SidebarNavLink[];
	storageQuota: number;
	storageUsed: number;
	trashDragOver: boolean;
	trashPath: string;
	user: UserInfo | null;
	workspace: Workspace;
	onTrashDragLeave: (event: DragEvent<HTMLAnchorElement>) => void;
	onTrashDragOver: (event: DragEvent<HTMLAnchorElement>) => void;
	onTrashDropEvent: (event: DragEvent<HTMLAnchorElement>) => void;
}

export interface SidebarResizeHandleProps {
	resizing: boolean;
	width: number;
	onKeyDown: (event: ReactKeyboardEvent<SidebarResizeHandleElement>) => void;
	onPointerDown: (event: ReactPointerEvent<SidebarResizeHandleElement>) => void;
}
