import { FolderTree } from "@/components/folders/FolderTree";
import { WorkspaceSwitcher } from "@/components/layout/WorkspaceSwitcher";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { SidebarNavigation } from "./SidebarNavigation";
import { SidebarQuickCategories } from "./SidebarQuickCategories";
import { SidebarStorageUsage } from "./SidebarStorageUsage";
import type { SidebarContentProps } from "./sidebarTypes";

export function SidebarContent({
	activeTeam,
	locationPathname,
	navLinks,
	onMobileClose,
	onMoveToFolder,
	onSearchCategoryOpen,
	onTrashDragLeave,
	onTrashDragOver,
	onTrashDropEvent,
	storageQuota,
	storageUsed,
	trashDragOver,
	trashPath,
	user,
	workspace,
}: SidebarContentProps) {
	return (
		<div className="flex h-full min-h-0 flex-col overflow-hidden overscroll-contain">
			<div className="shrink-0 border-b border-sidebar-border bg-sidebar px-3 py-2 sm:py-2.5">
				<WorkspaceSwitcher variant="sidebar" />
			</div>

			<ScrollArea data-testid="user-sidebar-scroll" className="min-h-0 flex-1">
				<div className="flex min-h-full flex-col">
					<FolderTree onMoveToFolder={onMoveToFolder} />

					<div className="mt-auto">
						<Separator />
						<SidebarQuickCategories
							onMobileClose={onMobileClose}
							onSearchCategoryOpen={onSearchCategoryOpen}
						/>
						<Separator />
						<SidebarNavigation
							locationPathname={locationPathname}
							navLinks={navLinks}
							onMobileClose={onMobileClose}
							onTrashDragLeave={onTrashDragLeave}
							onTrashDragOver={onTrashDragOver}
							onTrashDropEvent={onTrashDropEvent}
							trashDragOver={trashDragOver}
							trashPath={trashPath}
						/>
					</div>
				</div>
			</ScrollArea>

			<SidebarStorageUsage
				activeTeam={activeTeam}
				storageQuota={storageQuota}
				storageUsed={storageUsed}
				user={user}
				workspace={workspace}
			/>
		</div>
	);
}
