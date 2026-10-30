import type { ReactNode } from "react";
import { useCallback, useEffect, useState } from "react";
import { GlobalSearchDialog } from "@/components/layout/GlobalSearchDialog";
import { Sidebar } from "@/components/layout/Sidebar";
import { TopBar } from "@/components/layout/TopBar";
import { shouldIgnoreKeyboardTarget } from "@/hooks/useSelectionShortcuts";
import type { InternalDragData } from "@/lib/dragDrop";
import { isImeComposingKeyEvent } from "@/lib/keyboard";
import { useAuthStore } from "@/stores/authStore";
import { useTeamStore } from "@/stores/teamStore";

interface AppLayoutProps {
	children: ReactNode;
	sidebarContent?: ReactNode;
	onTrashDrop?: (data: InternalDragData) => void | Promise<void>;
	onMoveToFolder?: (
		fileIds: number[],
		folderIds: number[],
		targetFolderId: number | null,
	) => Promise<void> | void;
}

export function AppLayout({
	children,
	sidebarContent,
	onTrashDrop,
	onMoveToFolder,
}: AppLayoutProps) {
	const userId = useAuthStore((state) => state.user?.id ?? null);
	const ensureTeamsLoaded = useTeamStore((state) => state.ensureLoaded);
	const [mobileOpen, setMobileOpen] = useState(false);
	const [searchOpen, setSearchOpen] = useState(false);

	const handleMobileToggle = useCallback(() => {
		setMobileOpen((prev) => !prev);
	}, []);

	const handleMobileClose = useCallback(() => {
		setMobileOpen(false);
	}, []);

	const handleSearchOpen = useCallback(() => {
		setSearchOpen(true);
	}, []);

	useEffect(() => {
		if (sidebarContent != null) return;
		void ensureTeamsLoaded(userId).catch(() => undefined);
	}, [ensureTeamsLoaded, userId, sidebarContent]);

	useEffect(() => {
		if (sidebarContent != null) return;
		function handleKeyDown(event: KeyboardEvent) {
			if (
				shouldIgnoreKeyboardTarget(event.target) ||
				isImeComposingKeyEvent(event)
			) {
				return;
			}

			const mod = event.metaKey || event.ctrlKey;
			if (event.key === "/" || (mod && event.key.toLowerCase() === "k")) {
				event.preventDefault();
				setSearchOpen(true);
			}
		}

		document.addEventListener("keydown", handleKeyDown);
		return () => document.removeEventListener("keydown", handleKeyDown);
	}, [sidebarContent]);

	return (
		<div className="flex h-dvh flex-col">
			<TopBar
				onSidebarToggle={handleMobileToggle}
				mobileOpen={mobileOpen}
				onSearchOpen={sidebarContent == null ? handleSearchOpen : undefined}
			/>
			<div className="flex flex-1 overflow-hidden">
				<Sidebar
					content={sidebarContent}
					mobileOpen={mobileOpen}
					onMobileClose={handleMobileClose}
					onTrashDrop={onTrashDrop}
					onMoveToFolder={onMoveToFolder}
				/>
				<main className="min-h-0 min-w-0 flex-1 flex flex-col overflow-hidden">
					{children}
				</main>
			</div>
			{sidebarContent == null ? (
				<GlobalSearchDialog open={searchOpen} onOpenChange={setSearchOpen} />
			) : null}
		</div>
	);
}
