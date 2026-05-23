import type { ReactNode } from "react";
import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { NavLink } from "react-router-dom";
import { AdminTopBar } from "@/components/layout/AdminTopBar";
import { Icon, type IconName } from "@/components/ui/icon";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
	ADMIN_SIDEBAR_WIDTH_CLASS,
	ADMIN_TOPBAR_OFFSET_CLASS,
	SIDEBAR_SECTION_PADDING_CLASS,
} from "@/lib/constants";
import { cn, sidebarNavItemClass } from "@/lib/utils";

export function AdminLayout({ children }: { children: ReactNode }) {
	const { t } = useTranslation("admin");
	const [mobileOpen, setMobileOpen] = useState(false);

	const handleMobileToggle = useCallback(() => {
		setMobileOpen((prev) => !prev);
	}, []);

	const handleMobileClose = useCallback(() => {
		setMobileOpen(false);
	}, []);

	const primaryNavItems: { to: string; label: string; icon: IconName }[] = [
		{ to: "/admin/overview", label: t("overview"), icon: "Presentation" },
		{ to: "/admin/users", label: t("users"), icon: "Shield" },
		{ to: "/admin/teams", label: t("teams"), icon: "Cloud" },
		{ to: "/admin/policies", label: t("policies"), icon: "HardDrive" },
		{ to: "/admin/remote-nodes", label: t("remote_nodes"), icon: "Globe" },
		{ to: "/admin/external-auth", label: t("external_auth"), icon: "SignIn" },
		{
			to: "/admin/policy-groups",
			label: t("policy_groups"),
			icon: "ListBullets",
		},
		{ to: "/admin/shares", label: t("shares"), icon: "Link" },
		{ to: "/admin/tasks", label: t("tasks"), icon: "Clock" },
		{ to: "/admin/locks", label: t("locks"), icon: "Lock" },
		{ to: "/admin/settings", label: t("system_settings"), icon: "Gear" },
		{
			to: "/admin/audit",
			label: t("audit_log"),
			icon: "ClipboardText",
		},
	];
	const secondaryNavItems: { to: string; label: string; icon: IconName }[] = [
		{ to: "/admin/about", label: t("about"), icon: "Info" },
	];

	const sidebarContent = (
		<div className="flex h-full flex-col bg-sidebar text-sidebar-foreground">
			<ScrollArea className="min-h-0 flex-1 pt-2">
				<nav className={cn("space-y-1 py-2", SIDEBAR_SECTION_PADDING_CLASS)}>
					{primaryNavItems.map((item) => (
						<NavLink
							key={item.to}
							to={item.to}
							onClick={handleMobileClose}
							className={({ isActive }) => sidebarNavItemClass(isActive)}
						>
							<Icon name={item.icon} className="size-4 shrink-0" />
							{item.label}
						</NavLink>
					))}
				</nav>
			</ScrollArea>
			<div className={cn("border-t py-2", SIDEBAR_SECTION_PADDING_CLASS)}>
				<nav className="space-y-1">
					{secondaryNavItems.map((item) => (
						<NavLink
							key={item.to}
							to={item.to}
							onClick={handleMobileClose}
							className={({ isActive }) => sidebarNavItemClass(isActive)}
						>
							<Icon name={item.icon} className="size-4 shrink-0" />
							{item.label}
						</NavLink>
					))}
				</nav>
			</div>
		</div>
	);

	return (
		<div className="flex h-screen flex-col bg-background">
			<AdminTopBar
				onSidebarToggle={handleMobileToggle}
				mobileOpen={mobileOpen}
			/>
			<div className="flex min-h-0 flex-1 overflow-hidden">
				<button
					type="button"
					className={cn(
						"fixed inset-x-0 z-40 bg-black/50 transition-opacity duration-200 ease-out md:hidden motion-reduce:transition-none",
						ADMIN_TOPBAR_OFFSET_CLASS,
						mobileOpen ? "opacity-100" : "pointer-events-none opacity-0",
					)}
					onClick={handleMobileClose}
					aria-label={t("core:close_admin_sidebar")}
					tabIndex={mobileOpen ? 0 : -1}
				/>
				<aside
					className={cn(
						"border-r border-sidebar-border bg-sidebar text-sidebar-foreground transition-transform duration-200 ease-out motion-reduce:transition-none",
						ADMIN_SIDEBAR_WIDTH_CLASS,
						"fixed left-0 z-50 flex flex-col md:relative md:left-auto md:top-auto md:bottom-auto md:z-auto md:translate-x-0",
						ADMIN_TOPBAR_OFFSET_CLASS,
						mobileOpen
							? "translate-x-0 shadow-lg dark:shadow-none md:shadow-none"
							: "-translate-x-full pointer-events-none shadow-none md:pointer-events-auto",
					)}
				>
					{sidebarContent}
				</aside>
				<main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
					{children}
				</main>
			</div>
		</div>
	);
}
