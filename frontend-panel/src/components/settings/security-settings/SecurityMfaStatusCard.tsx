import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { formatDateAbsolute } from "@/lib/format";
import type { MfaFactorInfo } from "@/services/authService";
import type { PendingAction } from "./mfaTypes";

interface SecurityMfaStatusCardProps {
	factor: MfaFactorInfo;
	recoveryCodesRemaining: number;
	onOpenAction: (action: Exclude<PendingAction, null>) => void;
}

function methodLabel(method: MfaFactorInfo["method"]) {
	return method === "totp" ? "TOTP" : method;
}

export function SecurityMfaStatusCard({
	factor,
	onOpenAction,
	recoveryCodesRemaining,
}: SecurityMfaStatusCardProps) {
	const { t } = useTranslation(["settings"]);

	return (
		<div className="rounded-lg border bg-muted/20 p-4">
			<div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
				<div className="min-w-0 space-y-1">
					<p className="text-sm font-medium">{factor.name}</p>
					<p className="text-xs text-muted-foreground">
						{methodLabel(factor.method)} ·{" "}
						{t("settings:settings_mfa_enabled_at")}{" "}
						{formatDateAbsolute(factor.enabled_at)}
					</p>
					<p className="text-xs text-muted-foreground">
						{t("settings:settings_mfa_recovery_remaining", {
							count: recoveryCodesRemaining,
						})}
					</p>
				</div>
				<div className="flex flex-wrap gap-2">
					<Button
						type="button"
						variant="outline"
						onClick={() => onOpenAction("regenerate")}
					>
						<Icon name="ArrowsClockwise" className="mr-2 size-4" />
						{t("settings:settings_mfa_regenerate_recovery")}
					</Button>
					<Button
						type="button"
						variant="destructive"
						onClick={() => onOpenAction("disable")}
					>
						<Icon name="Trash" className="mr-2 size-4" />
						{t("settings:settings_mfa_disable")}
					</Button>
				</div>
			</div>
		</div>
	);
}

interface SecurityMfaEmptyStateProps {
	onStartSetup: () => void;
}

export function SecurityMfaEmptyState({
	onStartSetup,
}: SecurityMfaEmptyStateProps) {
	const { t } = useTranslation(["settings"]);

	return (
		<div className="rounded-lg border border-dashed bg-muted/20 px-4 py-8 text-center">
			<p className="text-sm font-medium">{t("settings:settings_mfa_empty")}</p>
			<p className="mt-1 text-sm text-muted-foreground">
				{t("settings:settings_mfa_empty_desc")}
			</p>
			<Button type="button" className="mt-4" onClick={onStartSetup}>
				<Icon name="Shield" className="mr-2 size-4" />
				{t("settings:settings_mfa_start_setup")}
			</Button>
		</div>
	);
}
