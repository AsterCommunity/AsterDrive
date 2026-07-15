import { useTranslation } from "react-i18next";
import { UserAvatarImage } from "@/components/common/UserAvatarImage";
import { Icon } from "@/components/ui/icon";
import { ScrollArea } from "@/components/ui/scroll-area";
import { USER_TOPBAR_OFFSET_CLASS } from "@/lib/constants";
import { cn } from "@/lib/utils";
import type { FolderContents, SharePublicInfo } from "@/types/api";
import { ShareFolderTree } from "./ShareFolderTree";
import {
	getAccessSummary,
	getDownloadSummary,
	getExpirySummary,
} from "./shareViewSummaries";
import type { ShareBreadcrumbItem } from "./types";

export function ShareFolderSidebar({
	breadcrumb,
	folderContents,
	info,
	mobileOpen,
	shareOwnerText,
	token,
	onMobileClose,
	onNavigate,
}: {
	breadcrumb: ShareBreadcrumbItem[];
	folderContents: FolderContents | null;
	info: SharePublicInfo;
	mobileOpen: boolean;
	shareOwnerText: string;
	token: string;
	onMobileClose: () => void;
	onNavigate: (folderId: number | null, folderName?: string) => void;
}) {
	const { t } = useTranslation(["core", "share"]);
	const downloadSummary = getDownloadSummary(info, t);
	const expirySummary = getExpirySummary(info, t);
	const accessSummary = getAccessSummary(info, t);
	const handleNavigate = (folderId: number | null, folderName?: string) => {
		onNavigate(folderId, folderName);
		onMobileClose();
	};

	return (
		<>
			<button
				type="button"
				className={cn(
					"fixed inset-x-0 z-(--z-fixed) cursor-default bg-black/50 transition-opacity duration-200 ease-out md:hidden motion-reduce:transition-none",
					USER_TOPBAR_OFFSET_CLASS,
					mobileOpen ? "opacity-100" : "pointer-events-none opacity-0",
				)}
				onClick={onMobileClose}
				aria-label={t("close_sidebar")}
				tabIndex={mobileOpen ? 0 : -1}
			/>
			<aside
				data-theme-surface="chrome"
				data-testid="share-folder-sidebar"
				className={cn(
					"w-60 shrink-0 border-r border-sidebar-border bg-sidebar text-sidebar-foreground transition-transform duration-200 ease-out motion-reduce:transition-none",
					"fixed left-0 z-(--z-fixed) flex flex-col md:relative md:left-auto md:top-auto md:bottom-auto md:z-auto md:translate-x-0",
					USER_TOPBAR_OFFSET_CLASS,
					mobileOpen
						? "translate-x-0 shadow-lg dark:shadow-none md:shadow-none"
						: "pointer-events-none -translate-x-full shadow-none md:pointer-events-auto",
				)}
			>
				<div className="shrink-0 border-b border-sidebar-border bg-sidebar px-3 py-2.5">
					<div className="flex min-w-0 items-center gap-2.5">
						<div className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-accent/55 text-accent-foreground">
							<Icon name="Link" className="size-4" />
						</div>
						<div className="min-w-0 flex-1">
							<p className="text-xs text-muted-foreground">
								{t("shared_folder")}
							</p>
							<p className="truncate text-sm font-medium">{info.name}</p>
						</div>
					</div>
				</div>

				<ScrollArea className="min-h-0 flex-1" data-testid="share-tree-scroll">
					<ShareFolderTree
						breadcrumb={breadcrumb}
						folderContents={folderContents}
						rootName={info.name}
						token={token}
						onNavigate={handleNavigate}
					/>
				</ScrollArea>

				<div className="shrink-0 border-t border-sidebar-border px-3 py-3 pb-[calc(0.75rem+env(safe-area-inset-bottom))] md:pb-3">
					<section
						className="flex min-w-0 items-center gap-2.5"
						aria-label={shareOwnerText}
					>
						<UserAvatarImage
							avatar={info.shared_by.avatar}
							name={info.shared_by.name}
							size="sm"
							className="rounded-lg"
						/>
						<div className="min-w-0 flex-1">
							<p className="truncate text-sm font-medium text-foreground">
								{info.shared_by.name}
							</p>
							<p className="text-xs text-muted-foreground">
								{t("share:share_owner")}
							</p>
						</div>
					</section>
					<div className="mt-3 grid gap-2 text-xs text-muted-foreground">
						<div className="flex min-w-0 items-center gap-2">
							<Icon name="Download" className="size-4 shrink-0" />
							<span className="min-w-0 leading-5">{downloadSummary}</span>
						</div>
						<div className="flex min-w-0 items-center gap-2">
							<Icon name="Clock" className="size-4 shrink-0" />
							<span className="min-w-0 leading-5">{expirySummary}</span>
						</div>
						<div className="flex min-w-0 items-center gap-2">
							<Icon
								name={info.has_password ? "Lock" : "Globe"}
								className="size-4 shrink-0"
							/>
							<span className="min-w-0 leading-5">{accessSummary}</span>
						</div>
					</div>
				</div>
			</aside>
		</>
	);
}
