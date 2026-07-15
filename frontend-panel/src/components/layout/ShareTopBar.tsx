import { useTranslation } from "react-i18next";
import { Link } from "react-router-dom";
import { HeaderControls } from "@/components/layout/HeaderControls";
import { MusicPlayerHeaderButton } from "@/components/layout/MusicPlayerHeaderButton";
import { TopBarBrand } from "@/components/layout/TopBarBrand";
import { TopBarShell } from "@/components/layout/TopBarShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Skeleton } from "@/components/ui/skeleton";
import { useAuthStore } from "@/stores/authStore";

export function ShareTopBar({
	mobileOpen = false,
	onSidebarToggle,
}: {
	mobileOpen?: boolean;
	onSidebarToggle?: () => void;
}) {
	const { t } = useTranslation(["core", "auth", "files"]);
	const isAuthenticated = useAuthStore((state) => state.isAuthenticated);
	const isChecking = useAuthStore((state) => state.isChecking);
	const user = useAuthStore((state) => state.user);
	const authenticated = isAuthenticated && user !== null;

	return (
		<TopBarShell
			onSidebarToggle={onSidebarToggle}
			sidebarOpen={mobileOpen}
			sidebarToggleLabels={{
				open: t("open_sidebar"),
				close: t("close_sidebar"),
			}}
			left={<TopBarBrand mobileVisible={!onSidebarToggle} />}
			right={
				<div className="flex items-center gap-2">
					{authenticated ? (
						<HeaderControls showHomeButton homeLabel={t("auth:go_home")} />
					) : (
						<>
							<MusicPlayerHeaderButton />
							{isChecking ? (
								<Skeleton className="h-9 w-24 rounded-full" />
							) : (
								<Button
									render={<Link to="/login" />}
									variant="outline"
									size="sm"
									className="rounded-full bg-background/65"
								>
									<Icon name="SignIn" className="mr-1.5 size-4" />
									{t("auth:go_to_login")}
								</Button>
							)}
						</>
					)}
					<span className="sr-only">{t("files:share")}</span>
				</div>
			}
		/>
	);
}
