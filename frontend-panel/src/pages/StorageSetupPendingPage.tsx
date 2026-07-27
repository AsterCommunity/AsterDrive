import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { useAuthStore } from "@/stores/authStore";

export default function StorageSetupPendingPage() {
	const { t } = useTranslation("auth");
	const logout = useAuthStore((state) => state.logout);

	return (
		<main className="flex min-h-screen items-center justify-center bg-[radial-gradient(circle_at_top_left,color-mix(in_oklab,var(--primary)_12%,transparent),transparent_38%),var(--background)] px-6 py-12">
			<section className="w-full max-w-lg rounded-3xl border border-border/70 bg-card/95 p-8 shadow-2xl shadow-black/5 backdrop-blur">
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
				<div className="mt-7 flex items-center gap-3 rounded-2xl border border-border/70 bg-muted/25 p-4 text-sm text-muted-foreground">
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
			</section>
		</main>
	);
}
