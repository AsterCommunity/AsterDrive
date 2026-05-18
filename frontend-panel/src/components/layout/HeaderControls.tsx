import type { ReactNode } from "react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { RoleBadge } from "@/components/common/RoleBadge";
import { UserAvatarImage } from "@/components/common/UserAvatarImage";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Icon } from "@/components/ui/icon";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { ensureI18nNamespaces } from "@/i18n";
import { MENU_SECTION_PADDING_CLASS } from "@/lib/constants";
import { queuePreferenceSync } from "@/lib/preferenceSync";
import { getNormalizedDisplayName, getUserDisplayName } from "@/lib/user";
import { useAuthStore } from "@/stores/authStore";
import { useMusicPlayerStore } from "@/stores/musicPlayerStore";
import { useThemeStore } from "@/stores/themeStore";

interface HeaderControlsProps {
	actions?: ReactNode;
	showHomeButton?: boolean;
	homeLabel?: string;
	showAdminEntry?: boolean;
}

export function HeaderControls({
	actions,
	showHomeButton = false,
	homeLabel,
	showAdminEntry = false,
}: HeaderControlsProps) {
	const { t, i18n } = useTranslation();
	const navigate = useNavigate();
	const user = useAuthStore((s) => s.user);
	const isAuthStale = useAuthStore((s) => s.isAuthStale);
	const logout = useAuthStore((s) => s.logout);
	const musicQueue = useMusicPlayerStore((s) => s.queue);
	const musicIsPlaying = useMusicPlayerStore((s) => s.isPlaying);
	const toggleMusicPanel = useMusicPlayerStore((s) => s.togglePanel);
	const mode = useThemeStore((s) => s.mode);
	const setMode = useThemeStore((s) => s.setMode);
	const [loggingOut, setLoggingOut] = useState(false);
	const userName = getUserDisplayName(user);
	const customDisplayName = getNormalizedDisplayName(
		user?.profile?.display_name,
	);
	const showUsernameSecondary =
		customDisplayName !== null && customDisplayName !== user?.username;
	const currentLanguage = i18n.language?.startsWith("zh") ? "zh" : "en";
	const triggerClassName =
		"h-10 gap-2 rounded-full border border-border/35 bg-background/65 px-1.5 transition-[background-color,border-color,color,box-shadow] hover:bg-muted/45 active:translate-y-0 aria-expanded:border-border/50 aria-expanded:bg-muted/45 sm:h-9 sm:px-2";
	const menuItemClass = `${MENU_SECTION_PADDING_CLASS} min-h-9 rounded-lg py-1.5`;
	const profileSectionClass = "rounded-xl bg-muted/10";
	const menuSectionClass = "rounded-xl";
	const dangerSectionClass = "rounded-xl";
	const preferenceRowClass = `${MENU_SECTION_PADDING_CLASS} flex items-center justify-between gap-3 rounded-lg py-1`;
	const segmentedControlClass =
		"flex items-center gap-1 rounded-lg bg-muted/35 p-1";
	const logoutButtonClass = `${MENU_SECTION_PADDING_CLASS} flex w-full cursor-default items-center gap-1.5 rounded-lg py-1.5 min-h-9 text-sm select-none text-muted-foreground transition-colors duration-150 hover:bg-destructive/8 hover:text-destructive disabled:pointer-events-none disabled:opacity-50`;

	const handleRouteNavigation = (
		to: string,
		{ viewTransition = false }: { viewTransition?: boolean } = {},
	) => {
		navigate(to, { viewTransition });
	};

	const handleSettingsNavigation = async () => {
		await Promise.all([
			import("@/pages/SettingsPage"),
			ensureI18nNamespaces(["settings"], i18n.language),
		]);
		navigate("/settings/profile", { viewTransition: false });
	};

	const handleLogout = async () => {
		try {
			setLoggingOut(true);
			await logout();
			navigate("/login", { replace: true });
		} finally {
			setLoggingOut(false);
		}
	};

	return (
		<div className="flex items-center gap-2 shrink-0">
			{actions}
			{isAuthStale && (
				<Tooltip>
					<TooltipTrigger
						render={
							<Badge
								variant="outline"
								className="hidden cursor-help items-center gap-1.5 border-amber-500/40 bg-amber-500/10 text-amber-700 md:inline-flex dark:text-amber-300"
							/>
						}
					>
						<Icon name="Warning" className="h-3.5 w-3.5" />
						<span>{t("offline:offline_status_short")}</span>
					</TooltipTrigger>
					<TooltipContent className="max-w-64 text-left leading-relaxed">
						<div>{t("offline:offline_mode")}</div>
						<div className="text-background/80">
							{t("offline:auth_stale_detail")}
						</div>
					</TooltipContent>
				</Tooltip>
			)}
			{showHomeButton ? (
				<Button
					variant="ghost"
					size="sm"
					className="hidden md:inline-flex"
					onClick={() => handleRouteNavigation("/")}
				>
					<Icon name="House" className="mr-1.5 h-4 w-4" />
					{homeLabel ?? t("back")}
				</Button>
			) : null}
			{musicQueue.length > 0 ? (
				<Tooltip>
					<TooltipTrigger
						render={
							<Button
								type="button"
								variant="ghost"
								size="icon-sm"
								className="rounded-full"
								onClick={toggleMusicPanel}
								aria-label={t("files:music_player_open")}
								data-music-player-trigger
							/>
						}
					>
						<Icon
							name={musicIsPlaying ? "MusicNotes" : "VinylRecord"}
							className={musicIsPlaying ? "h-4 w-4 text-primary" : "h-4 w-4"}
						/>
					</TooltipTrigger>
					<TooltipContent>{t("files:music_player_open")}</TooltipContent>
				</Tooltip>
			) : null}
			<DropdownMenu>
				<DropdownMenuTrigger
					render={
						<Button
							variant="ghost"
							size="sm"
							className={triggerClassName}
							aria-label={userName}
						/>
					}
				>
					<UserAvatarImage
						avatar={user?.profile.avatar ?? null}
						name={userName}
						size="sm"
						className="border border-border/60"
					/>
					<span className="sr-only">{userName}</span>
					<div className="hidden min-w-0 items-center gap-2 md:flex">
						<span className="max-w-28 truncate text-sm font-medium">
							{userName}
						</span>
						{user?.role === "admin" ? (
							<Badge
								variant="outline"
								className="border-border/45 bg-background/70 px-1.5 py-0 text-[11px] text-muted-foreground"
							>
								admin
							</Badge>
						) : null}
					</div>
					<Icon
						name="CaretDown"
						className="hidden h-3.5 w-3.5 text-muted-foreground md:block"
					/>
				</DropdownMenuTrigger>
				<DropdownMenuContent
					align="end"
					className="w-[18.5rem] min-w-[18.5rem] p-1.5"
				>
					<div className="flex flex-col">
						<div
							className={`${profileSectionClass} ${MENU_SECTION_PADDING_CLASS} flex items-center gap-2.5 py-2.5`}
						>
							<UserAvatarImage
								avatar={user?.profile.avatar ?? null}
								name={userName}
								size="lg"
								className="ring-1 ring-border/35"
							/>
							<div className="min-w-0 flex-1 space-y-1.5">
								<div className="flex flex-wrap items-center gap-x-2 gap-y-1">
									<p className="min-w-0 truncate text-sm font-semibold text-foreground">
										{userName}
									</p>
									{showUsernameSecondary ? (
										<p className="shrink-0 text-xs font-medium text-muted-foreground">
											@{user?.username}
										</p>
									) : null}
								</div>
								{user?.email ? (
									<p className="min-w-0 truncate text-xs text-muted-foreground">
										{user.email}
									</p>
								) : null}
								{user?.role ? (
									<div className="flex items-center">
										<RoleBadge role={user.role} label={user.role} />
									</div>
								) : null}
							</div>
						</div>

						<div className="mx-2 my-1 h-px bg-border/60" />

						<div className={`${menuSectionClass} p-1`}>
							<DropdownMenuItem
								onClick={() => {
									void handleSettingsNavigation();
								}}
								className={menuItemClass}
							>
								<Icon name="Gear" className="mr-2 h-4 w-4" />
								{t("settings")}
							</DropdownMenuItem>

							{showAdminEntry && user?.role === "admin" ? (
								<DropdownMenuItem
									onClick={() => handleRouteNavigation("/admin")}
									className={menuItemClass}
								>
									<Icon name="Shield" className="mr-2 h-4 w-4" />
									{t("admin_panel")}
								</DropdownMenuItem>
							) : null}

							{showHomeButton ? (
								<DropdownMenuItem
									onClick={() => handleRouteNavigation("/")}
									className={menuItemClass}
								>
									<Icon name="House" className="mr-2 h-4 w-4" />
									{homeLabel ?? t("back")}
								</DropdownMenuItem>
							) : null}
						</div>

						<div className="mx-2 my-1 h-px bg-border/50" />

						<div className={`${menuSectionClass} px-1 py-0`}>
							<div className={preferenceRowClass}>
								<span className="text-[11px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
									{t("theme")}
								</span>
								<div className={segmentedControlClass}>
									<Button
										variant={mode === "light" ? "secondary" : "ghost"}
										size="icon-sm"
										className="rounded-md"
										onClick={() => setMode("light")}
										aria-label={t("theme_light")}
									>
										<Icon name="Sun" />
									</Button>
									<Button
										variant={mode === "dark" ? "secondary" : "ghost"}
										size="icon-sm"
										className="rounded-md"
										onClick={() => setMode("dark")}
										aria-label={t("theme_dark")}
									>
										<Icon name="Moon" />
									</Button>
									<Button
										variant={mode === "system" ? "secondary" : "ghost"}
										size="icon-sm"
										className="rounded-md"
										onClick={() => setMode("system")}
										aria-label={t("theme_system")}
									>
										<Icon name="Monitor" />
									</Button>
								</div>
							</div>

							<div className="mx-2 my-0.5 h-px bg-border/70" />

							<div className={preferenceRowClass}>
								<span className="text-[11px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
									{t("language")}
								</span>
								<div className={segmentedControlClass}>
									<Button
										variant={currentLanguage === "en" ? "secondary" : "ghost"}
										size="xs"
										className="min-w-9 rounded-md"
										onClick={() => {
											void i18n.changeLanguage("en");
											queuePreferenceSync({ language: "en" });
										}}
										aria-label={t("language_en")}
									>
										EN
									</Button>
									<Button
										variant={currentLanguage === "zh" ? "secondary" : "ghost"}
										size="xs"
										className="min-w-9 rounded-md"
										onClick={() => {
											void i18n.changeLanguage("zh");
											queuePreferenceSync({ language: "zh" });
										}}
										aria-label={t("language_zh")}
									>
										{t("language_zh")}
									</Button>
								</div>
							</div>
						</div>

						<div className="mx-2 my-1 h-px bg-border/50" />

						<div className={`${dangerSectionClass} p-1`}>
							<button
								type="button"
								className={logoutButtonClass}
								onClick={() => void handleLogout()}
								disabled={loggingOut}
							>
								<Icon name="SignOut" className="mr-2 h-4 w-4 shrink-0" />
								{loggingOut ? t("loading") : t("logout")}
							</button>
						</div>
					</div>
				</DropdownMenuContent>
			</DropdownMenu>
		</div>
	);
}
