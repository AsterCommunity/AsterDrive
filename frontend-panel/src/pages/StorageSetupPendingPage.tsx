import { useTranslation } from "react-i18next";
import { AuthPageShell } from "@/components/auth/AuthPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { useAuthStore } from "@/stores/authStore";

export default function StorageSetupPendingPage() {
	const { t } = useTranslation("auth");
	const logout = useAuthStore((state) => state.logout);

	return (
		<AuthPageShell>
			<div className="flex size-12 items-center justify-center rounded-2xl bg-primary/10 text-primary">
				<Icon name="HardDrive" className="size-6" />
			</div>
			<p className="mt-6 text-xs font-semibold tracking-[0.2em] text-primary uppercase">
				{t("storage_setup_pending_eyebrow")}
			</p>
			<h1 className="mt-2 font-heading text-2xl font-semibold tracking-tight">
				{t("storage_setup_pending_title")}
			</h1>
			<p className="mt-3 text-sm leading-7 text-muted-foreground">
				{t("storage_setup_pending_desc")}
			</p>
			<div className="mt-7 flex items-center gap-3 rounded-2xl bg-muted/30 p-4 text-sm text-muted-foreground">
				<Icon name="Spinner" className="size-4 shrink-0 animate-spin" />
				<span>{t("storage_setup_pending_refreshing")}</span>
			</div>
			<Button
				type="button"
				variant="outline"
				className="mt-6 w-full"
				onClick={() => void logout()}
			>
				{t("core:logout")}
			</Button>
		</AuthPageShell>
	);
}
