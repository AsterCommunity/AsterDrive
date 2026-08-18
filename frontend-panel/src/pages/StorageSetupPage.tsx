import { useState } from "react";
import { useTranslation } from "react-i18next";
import { AuthPageShell } from "@/components/auth/AuthPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { usePageTitle } from "@/hooks/usePageTitle";
import { usePendingAction } from "@/hooks/usePendingAction";
import AdminPoliciesPage from "@/pages/admin/AdminPoliciesPage";
import { useAuthStore } from "@/stores/authStore";

export default function StorageSetupPage() {
	const { t } = useTranslation(["auth", "core"]);
	const logout = useAuthStore((state) => state.logout);
	const [started, setStarted] = useState(false);
	const { pending: signingOut, runWithPending: runLogout } = usePendingAction();

	usePageTitle(t("storage_setup_page_title"));

	if (started) {
		return <AdminPoliciesPage variant="setup" />;
	}

	const handleLogout = async () => {
		await runLogout(async () => {
			try {
				await logout();
			} catch (error) {
				handleApiError(error);
			}
		});
	};

	return (
		<AuthPageShell contentClassName="max-w-md">
			<section aria-labelledby="storage-setup-title">
				<div className="space-y-3">
					<p className="text-xs font-semibold tracking-[0.18em] text-primary uppercase">
						{t("storage_setup_eyebrow")}
					</p>
					<h1
						id="storage-setup-title"
						className="font-heading text-2xl font-semibold tracking-tight sm:text-3xl"
					>
						{t("storage_setup_page_title")}
					</h1>
					<p className="text-sm leading-6 text-muted-foreground sm:text-base">
						{t("storage_setup_page_desc")}
					</p>
				</div>

				<div className="mt-8 flex flex-col gap-3 sm:flex-row sm:items-center">
					<Button
						type="button"
						className="h-10 sm:min-w-48"
						onClick={() => setStarted(true)}
					>
						<Icon name="HardDrive" className="mr-1 size-4" />
						{t("storage_setup_start")}
					</Button>
					<Button
						type="button"
						variant="outline"
						className="h-10 sm:ml-auto"
						disabled={signingOut}
						onClick={() => void handleLogout()}
					>
						<Icon
							name={signingOut ? "Spinner" : "SignOut"}
							className={`mr-1 size-4 ${signingOut ? "animate-spin" : ""}`}
						/>
						{t("core:logout")}
					</Button>
				</div>
			</section>
		</AuthPageShell>
	);
}
