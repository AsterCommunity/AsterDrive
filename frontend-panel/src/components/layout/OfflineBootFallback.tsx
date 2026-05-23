import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";

export function OfflineBootFallback() {
	const { t } = useTranslation("offline");

	return (
		<div className="flex min-h-screen bg-background text-foreground">
			<div className="m-auto flex w-full max-w-xl flex-col items-center gap-6 px-6 text-center">
				<div className="flex size-16 items-center justify-center rounded-2xl border bg-muted/40">
					<Icon name="WifiX" className="size-8 text-muted-foreground" />
				</div>
				<div className="space-y-2">
					<h1 className="text-2xl font-semibold tracking-tight">
						{t("offline_boot_title")}
					</h1>
					<p className="text-sm leading-6 text-muted-foreground sm:text-base">
						{t("offline_boot_description")}
					</p>
				</div>
				<div className="flex flex-col gap-3 sm:flex-row">
					<Button onClick={() => window.location.reload()}>
						<Icon name="ArrowClockwise" className="mr-2 size-4" />
						{t("offline_boot_retry")}
					</Button>
					<Button
						variant="outline"
						onClick={() => window.location.assign("/login")}
					>
						<Icon name="SignIn" className="mr-2 size-4" />
						{t("offline_boot_login")}
					</Button>
				</div>
				<p className="text-xs text-muted-foreground">
					{t("offline_boot_hint")}
				</p>
			</div>
		</div>
	);
}
