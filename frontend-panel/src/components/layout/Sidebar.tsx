import {
	type CSSProperties,
	type DragEvent,
	type KeyboardEvent as ReactKeyboardEvent,
	type PointerEvent as ReactPointerEvent,
	useCallback,
	useEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { Link, useLocation } from "react-router-dom";
import { FolderTree } from "@/components/folders/FolderTree";
import { WorkspaceSwitcher } from "@/components/layout/WorkspaceSwitcher";
import { Icon, type IconName } from "@/components/ui/icon";
import { Progress } from "@/components/ui/progress";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { STORAGE_KEYS } from "@/config/app";
import {
	USER_SIDEBAR_DEFAULT_WIDTH_PX,
	USER_SIDEBAR_MAX_WIDTH_PX,
	USER_SIDEBAR_MIN_WIDTH_PX,
	USER_SIDEBAR_WIDTH_CLASS,
	USER_TOPBAR_OFFSET_CLASS,
} from "@/lib/constants";
import {
	hasInternalDragData,
	type InternalDragData,
	readInternalDragData,
} from "@/lib/dragDrop";
import { formatBytes } from "@/lib/format";
import { cn, sidebarNavItemClass } from "@/lib/utils";
import {
	isTeamWorkspace,
	workspaceSharesPath,
	workspaceTasksPath,
	workspaceTrashPath,
	workspaceWebdavPath,
} from "@/lib/workspace";
import { useAuthStore } from "@/stores/authStore";
import { useTeamStore } from "@/stores/teamStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import type { FileCategory } from "@/types/api";

const SIDEBAR_KEYBOARD_RESIZE_STEP_PX = 16;
type SidebarResizeHandleElement = HTMLHRElement;

const QUICK_CATEGORY_LINKS: Array<{
	category: FileCategory;
	icon: IconName;
	labelKey: string;
}> = [
	{ category: "image", icon: "FileImage", labelKey: "category_image" },
	{ category: "video", icon: "FileVideo", labelKey: "category_video" },
	{ category: "audio", icon: "FileAudio", labelKey: "category_audio" },
	{ category: "document", icon: "FileText", labelKey: "category_document" },
];

interface SidebarProps {
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

function clampSidebarWidth(width: number) {
	return Math.min(
		USER_SIDEBAR_MAX_WIDTH_PX,
		Math.max(USER_SIDEBAR_MIN_WIDTH_PX, Math.round(width)),
	);
}

function readStoredSidebarWidth() {
	if (typeof localStorage === "undefined") {
		return USER_SIDEBAR_DEFAULT_WIDTH_PX;
	}

	const raw = localStorage.getItem(STORAGE_KEYS.userSidebarWidth);
	if (raw == null) {
		return USER_SIDEBAR_DEFAULT_WIDTH_PX;
	}

	const parsed = Number(raw);
	if (!Number.isFinite(parsed)) {
		return USER_SIDEBAR_DEFAULT_WIDTH_PX;
	}

	return clampSidebarWidth(parsed);
}

function storeSidebarWidth(width: number) {
	if (typeof localStorage === "undefined") {
		return;
	}

	try {
		localStorage.setItem(
			STORAGE_KEYS.userSidebarWidth,
			String(clampSidebarWidth(width)),
		);
	} catch {
		// localStorage can be unavailable or full; resizing should still work.
	}
}

export function Sidebar({
	mobileOpen,
	onMobileClose,
	onTrashDrop,
	onMoveToFolder,
	onSearchCategoryOpen,
}: SidebarProps) {
	const { t } = useTranslation();
	const location = useLocation();
	const user = useAuthStore((s) => s.user);
	const workspace = useWorkspaceStore((s) => s.workspace);
	const teams = useTeamStore((s) => s.teams);
	const resizeStateRef = useRef<{
		startWidth: number;
		startX: number;
		width: number;
	} | null>(null);
	const [trashDragOver, setTrashDragOver] = useState(false);
	const [sidebarWidth, setSidebarWidth] = useState(readStoredSidebarWidth);
	const [sidebarResizing, setSidebarResizing] = useState(false);
	const activeTeam = isTeamWorkspace(workspace)
		? (teams.find((team) => team.id === workspace.teamId) ?? null)
		: null;

	useEffect(() => {
		if (!sidebarResizing) {
			return;
		}

		const body = typeof document !== "undefined" ? document.body : null;
		const previousCursor = body?.style.cursor;
		const previousUserSelect = body?.style.userSelect;

		if (body) {
			body.style.cursor = "col-resize";
			body.style.userSelect = "none";
		}

		function handlePointerMove(event: PointerEvent) {
			const state = resizeStateRef.current;
			if (!state) {
				return;
			}

			const nextWidth = clampSidebarWidth(
				state.startWidth + event.clientX - state.startX,
			);
			state.width = nextWidth;
			setSidebarWidth(nextWidth);
		}

		function handlePointerEnd() {
			const width = resizeStateRef.current?.width;
			resizeStateRef.current = null;
			setSidebarResizing(false);
			if (width != null) {
				storeSidebarWidth(width);
			}
		}

		window.addEventListener("pointermove", handlePointerMove);
		window.addEventListener("pointerup", handlePointerEnd);
		window.addEventListener("pointercancel", handlePointerEnd);

		return () => {
			window.removeEventListener("pointermove", handlePointerMove);
			window.removeEventListener("pointerup", handlePointerEnd);
			window.removeEventListener("pointercancel", handlePointerEnd);

			if (body) {
				body.style.cursor = previousCursor ?? "";
				body.style.userSelect = previousUserSelect ?? "";
			}
		};
	}, [sidebarResizing]);

	const sidebarStyle: CSSProperties & { "--user-sidebar-width": string } = {
		"--user-sidebar-width": `${sidebarWidth}px`,
	};

	const commitSidebarWidth = useCallback((width: number) => {
		const nextWidth = clampSidebarWidth(width);
		setSidebarWidth(nextWidth);
		storeSidebarWidth(nextWidth);
	}, []);

	const handleSidebarResizePointerDown = useCallback(
		(event: ReactPointerEvent<SidebarResizeHandleElement>) => {
			if (event.button !== 0) {
				return;
			}

			event.preventDefault();
			resizeStateRef.current = {
				startWidth: sidebarWidth,
				startX: event.clientX,
				width: sidebarWidth,
			};
			setSidebarResizing(true);
		},
		[sidebarWidth],
	);

	const handleSidebarResizeKeyDown = useCallback(
		(event: ReactKeyboardEvent<SidebarResizeHandleElement>) => {
			let nextWidth: number | null = null;

			if (event.key === "ArrowLeft") {
				nextWidth = sidebarWidth - SIDEBAR_KEYBOARD_RESIZE_STEP_PX;
			} else if (event.key === "ArrowRight") {
				nextWidth = sidebarWidth + SIDEBAR_KEYBOARD_RESIZE_STEP_PX;
			} else if (event.key === "Home") {
				nextWidth = USER_SIDEBAR_MIN_WIDTH_PX;
			} else if (event.key === "End") {
				nextWidth = USER_SIDEBAR_MAX_WIDTH_PX;
			}

			if (nextWidth == null) {
				return;
			}

			event.preventDefault();
			commitSidebarWidth(nextWidth);
		},
		[commitSidebarWidth, sidebarWidth],
	);

	const navLinks: { to: string; icon: IconName; label: string }[] =
		useMemo(() => {
			const links: { to: string; icon: IconName; label: string }[] = [
				{
					to: workspaceTrashPath(workspace),
					icon: "Trash",
					label: t("trash"),
				},
				{
					to: workspaceSharesPath(workspace),
					icon: "Link",
					label: t("share:my_shares_title"),
				},
				{
					to: workspaceTasksPath(workspace),
					icon: "Clock",
					label: t("tasks:title"),
				},
			];

			if (!isTeamWorkspace(workspace)) {
				links.push({
					to: workspaceWebdavPath(),
					icon: "HardDrive",
					label: t("webdav"),
				});
			}

			return links;
		}, [t, workspace]);

	const storageUsed = activeTeam
		? activeTeam.storage_used
		: !isTeamWorkspace(workspace)
			? (user?.storage_used ?? 0)
			: 0;
	const storageQuota = activeTeam
		? activeTeam.storage_quota
		: !isTeamWorkspace(workspace)
			? (user?.storage_quota ?? 0)
			: 0;
	const trashPath = workspaceTrashPath(workspace);

	const handleTrashDragOver = (e: DragEvent<HTMLAnchorElement>) => {
		if (!onTrashDrop || !hasInternalDragData(e.dataTransfer)) return;
		e.preventDefault();
		e.dataTransfer.dropEffect = "move";
		setTrashDragOver(true);
	};

	const handleTrashDragLeave = (e: DragEvent<HTMLAnchorElement>) => {
		const nextTarget = e.relatedTarget;
		if (nextTarget instanceof Node && e.currentTarget.contains(nextTarget)) {
			return;
		}
		setTrashDragOver(false);
	};

	const handleTrashDrop = (e: DragEvent<HTMLAnchorElement>) => {
		setTrashDragOver(false);
		if (!onTrashDrop) return;
		e.preventDefault();
		const data = readInternalDragData(e.dataTransfer);
		if (!data) return;
		void onTrashDrop(data);
	};

	const sidebarContent = (
		<div className="flex h-full min-h-0 flex-col overflow-hidden overscroll-contain">
			<div className="shrink-0 border-b border-sidebar-border bg-sidebar px-3 py-2 sm:py-2.5">
				<WorkspaceSwitcher variant="sidebar" />
			</div>

			{/* FolderTree scrolling is handled by ScrollArea. */}
			<ScrollArea className="min-h-32 flex-1">
				<FolderTree onMoveToFolder={onMoveToFolder} />
			</ScrollArea>

			<Separator />

			{/* Quick category search */}
			<div className="shrink-0 p-2 space-y-1">
				<p className="px-3 py-1 text-xs font-medium text-muted-foreground">
					{t("search:quick_categories")}
				</p>
				{QUICK_CATEGORY_LINKS.map((link) => (
					<button
						key={link.category}
						type="button"
						onClick={() => {
							onSearchCategoryOpen?.(link.category);
							onMobileClose();
						}}
						className={sidebarNavItemClass(false, "w-full text-left")}
					>
						<Icon name={link.icon} className="h-4 w-4 shrink-0" />
						{t(`search:${link.labelKey}`)}
					</button>
				))}
			</div>

			<Separator />

			{/* Navigation links */}
			<div className="shrink-0 p-2 space-y-1">
				{navLinks.map((link) => (
					<Link
						key={link.to}
						to={link.to}
						onClick={onMobileClose}
						onDragOver={link.to === trashPath ? handleTrashDragOver : undefined}
						onDragLeave={
							link.to === trashPath ? handleTrashDragLeave : undefined
						}
						onDrop={link.to === trashPath ? handleTrashDrop : undefined}
						className={sidebarNavItemClass(
							location.pathname === link.to,
							link.to === trashPath &&
								trashDragOver &&
								"bg-destructive/10 text-destructive ring-1 ring-destructive/30",
						)}
					>
						<Icon name={link.icon} className="h-4 w-4 shrink-0" />
						{link.label}
					</Link>
				))}
			</div>

			{/* Storage usage */}
			{user && (!isTeamWorkspace(workspace) || activeTeam) && (
				<>
					<Separator />
					<div className="shrink-0 p-3 space-y-1.5">
						<p className="text-xs font-medium text-muted-foreground">
							{activeTeam ? activeTeam.name : t("files:storage_space")}
						</p>
						<Progress
							value={
								storageQuota > 0
									? Math.min((storageUsed / storageQuota) * 100, 100)
									: 0
							}
							className="h-1.5"
						/>
						<p className="text-xs text-muted-foreground">
							{storageQuota > 0
								? t("files:storage_quota", {
										used: formatBytes(storageUsed),
										quota: formatBytes(storageQuota),
									})
								: t("files:storage_used", {
										used: formatBytes(storageUsed),
									})}
						</p>
					</div>
				</>
			)}
		</div>
	);

	return (
		<>
			{/* Mobile overlay backdrop */}
			<button
				type="button"
				className={cn(
					"fixed inset-x-0 z-40 cursor-default bg-black/50 transition-opacity duration-200 ease-out md:hidden motion-reduce:transition-none",
					USER_TOPBAR_OFFSET_CLASS,
					mobileOpen ? "opacity-100" : "pointer-events-none opacity-0",
				)}
				onClick={onMobileClose}
				aria-label={t("close_sidebar")}
				aria-hidden={!mobileOpen}
				tabIndex={mobileOpen ? 0 : -1}
			/>

			{/* Sidebar - desktop inline, mobile overlay */}
			<aside
				data-theme-surface="chrome"
				style={sidebarStyle}
				className={cn(
					"border-r border-sidebar-border bg-sidebar text-sidebar-foreground transition-transform duration-200 ease-out motion-reduce:transition-none",
					USER_SIDEBAR_WIDTH_CLASS,
					"fixed left-0 z-50 flex shrink-0 flex-col md:relative md:left-auto md:top-auto md:bottom-auto md:z-auto md:translate-x-0",
					USER_TOPBAR_OFFSET_CLASS,
					mobileOpen
						? "translate-x-0 shadow-lg dark:shadow-none md:shadow-none"
						: "-translate-x-full pointer-events-none shadow-none md:pointer-events-auto",
				)}
			>
				{sidebarContent}
				<hr
					aria-label={t("resize_sidebar")}
					aria-orientation="vertical"
					aria-valuemax={USER_SIDEBAR_MAX_WIDTH_PX}
					aria-valuemin={USER_SIDEBAR_MIN_WIDTH_PX}
					aria-valuenow={sidebarWidth}
					tabIndex={0}
					className={cn(
						"absolute inset-y-0 -right-1 z-20 hidden h-auto w-2 cursor-col-resize touch-none border-0 bg-transparent transition-colors md:block focus-visible:bg-primary/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring hover:bg-primary/15",
						sidebarResizing && "bg-primary/25",
					)}
					onPointerDown={handleSidebarResizePointerDown}
					onKeyDown={handleSidebarResizeKeyDown}
				/>
			</aside>
		</>
	);
}
