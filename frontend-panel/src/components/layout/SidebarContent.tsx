import { FolderTree } from "@/components/folders/FolderTree";
import { WorkspaceSwitcher } from "@/components/layout/WorkspaceSwitcher";
import { SidebarNavigation } from "./SidebarNavigation";
import { SidebarQuickCategories } from "./SidebarQuickCategories";
import { SidebarStorageUsage } from "./SidebarStorageUsage";
import type { SidebarContentProps } from "./sidebarTypes";
import { WorkspaceSidebarLayout } from "./WorkspaceSidebarLayout";

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
		<WorkspaceSidebarLayout
			header={<WorkspaceSwitcher variant="sidebar" />}
			tree={<FolderTree onMoveToFolder={onMoveToFolder} />}
			scrollProps={{
				ref: scrollViewportRef,
				"data-testid": "user-sidebar-scroll",
				viewportProps: { onScroll: onScrollViewport },
			}}
			navigation={
				<>
					<SidebarQuickCategories
						onMobileClose={onMobileClose}
						workspace={workspace}
					/>
					<SidebarNavigation
						items={navLinks.map((link) => ({
							...link,
							key: link.to,
							active: locationPathname === link.to,
							onClick: onMobileClose,
							dragOver: link.to === trashPath && trashDragOver,
							onDragOver: link.to === trashPath ? onTrashDragOver : undefined,
							onDragLeave: link.to === trashPath ? onTrashDragLeave : undefined,
							onDrop: link.to === trashPath ? onTrashDropEvent : undefined,
						}))}
					/>
				</>
			}
			footer={
				<SidebarStorageUsage
					activeTeam={activeTeam}
					storageQuota={storageQuota}
					storageUsed={storageUsed}
					user={user}
					workspace={workspace}
				/>
			}
		/>
	);
}
