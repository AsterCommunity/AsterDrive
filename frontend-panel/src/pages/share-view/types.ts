import type { FolderIcon } from "@/types/api";

export interface ShareBreadcrumbItem {
	id: number | null;
	name: string;
	icon?: FolderIcon;
}
