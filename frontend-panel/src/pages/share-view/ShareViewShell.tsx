import { type ReactNode, useCallback, useState } from "react";
import { UserAvatarImage } from "@/components/common/UserAvatarImage";
import { ShareTopBar } from "@/components/layout/ShareTopBar";
import { Icon } from "@/components/ui/icon";
import type { FolderContents, SharePublicInfo } from "@/types/api";
import { ShareFolderSidebar } from "./ShareFolderSidebar";
import type { ShareBreadcrumbItem } from "./types";

export function SharePageShell({ children }: { children: ReactNode }) {
	return (
		<div className="flex h-dvh flex-col bg-background text-foreground">
			<ShareTopBar />
			{children}
		</div>
	);
}

export function ShareFolderPageShell({
	breadcrumb,
	children,
	folderContents,
	info,
	shareOwnerText,
	token,
	onNavigate,
}: {
	breadcrumb: ShareBreadcrumbItem[];
	children: ReactNode;
	folderContents: FolderContents | null;
	info: SharePublicInfo;
	shareOwnerText: string;
	token: string;
	onNavigate: (folderId: number | null, folderName?: string) => void;
}) {
	const [mobileOpen, setMobileOpen] = useState(false);
	const handleMobileToggle = useCallback(() => {
		setMobileOpen((current) => !current);
	}, []);
	const handleMobileClose = useCallback(() => setMobileOpen(false), []);

	return (
		<div className="flex h-dvh flex-col bg-background text-foreground">
			<ShareTopBar
				mobileOpen={mobileOpen}
				onSidebarToggle={handleMobileToggle}
			/>
			<div className="flex min-h-0 flex-1 overflow-hidden">
				<ShareFolderSidebar
					breadcrumb={breadcrumb}
					folderContents={folderContents}
					info={info}
					mobileOpen={mobileOpen}
					shareOwnerText={shareOwnerText}
					token={token}
					onMobileClose={handleMobileClose}
					onNavigate={onNavigate}
				/>
				<div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
					{children}
				</div>
			</div>
		</div>
	);
}

export function ShareOwnerBanner({
	owner,
	text,
}: {
	owner: SharePublicInfo["shared_by"];
	text: string;
}) {
	return (
		// D9：分享者信息条去框，头像 + 文字裸排
		<div className="flex max-w-full items-center gap-3 px-1 py-2">
			<UserAvatarImage
				avatar={owner.avatar}
				name={owner.name}
				size="sm"
				className="rounded-lg"
			/>
			<div className="min-w-0">
				<div className="truncate text-sm font-medium text-foreground">
					{text}
				</div>
			</div>
		</div>
	);
}

export function ShareCenteredPanel({
	children,
	description,
	icon,
	title,
}: {
	children?: ReactNode;
	description: string;
	icon: "Lock" | "Warning";
	title: string;
}) {
	return (
		<SharePageShell>
			<main className="flex min-h-0 flex-1 items-center justify-center overflow-auto p-4 sm:p-6">
				{/* D9：居中面板用微弱色阶代替边框投影，保持视觉锚点 */}
				<section className="w-full max-w-md rounded-xl bg-muted/25 p-6 dark:bg-muted/15">
					<div className="text-center">
						<div className="mx-auto flex size-14 items-center justify-center rounded-lg bg-muted/45 text-muted-foreground">
							<Icon
								name={icon}
								className={
									icon === "Warning" ? "size-7 text-destructive" : "size-7"
								}
							/>
						</div>
						<h1 className="mt-4 text-lg font-semibold leading-snug">{title}</h1>
						<p className="mt-2 text-sm leading-6 text-muted-foreground">
							{description}
						</p>
					</div>
					{children ? <div className="mt-5">{children}</div> : null}
				</section>
			</main>
		</SharePageShell>
	);
}
