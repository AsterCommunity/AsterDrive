import { FolderTree } from "@/components/folders/FolderTree";
import { WorkspaceSwitcher } from "@/components/layout/WorkspaceSwitcher";
import { ScrollArea } from "@/components/ui/scroll-area";
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
	onScrollViewport,
	onTrashDragLeave,
	onTrashDragOver,
	onTrashDropEvent,
	scrollViewportRef,
	storageQuota,
	storageUsed,
	trashDragOver,
	trashPath,
	user,
	workspace,
}: SidebarContentProps) {
	return (
		<div className="flex h-full min-h-0 flex-col overflow-hidden overscroll-contain">
			<div className="shrink-0 px-3 py-2 sm:py-2.5">
				<WorkspaceSwitcher variant="sidebar" />
			</div>

			<ScrollArea
				ref={scrollViewportRef}
				data-testid="user-sidebar-scroll"
				className="min-h-0 flex-1"
				viewportProps={{ onScroll: onScrollViewport }}
			>
				<div className="flex min-h-full flex-col">
					<FolderTree onMoveToFolder={onMoveToFolder} />

					<div className="mt-auto space-y-2 pt-2">
						<SidebarQuickCategories
							onMobileClose={onMobileClose}
							workspace={workspace}
						/>
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
