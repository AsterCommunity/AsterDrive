import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { ActionUiState } from "./mfaTypes";

interface SecurityMfaSensitiveActionFormProps {
	actionState: ActionUiState;
	onCancel: () => void;
	onCodeChange: (value: string) => void;
	onSubmit: () => void;
}

export function SecurityMfaSensitiveActionForm({
	actionState,
	onCancel,
	onCodeChange,
	onSubmit,
}: SecurityMfaSensitiveActionFormProps) {
	const { t } = useTranslation(["core", "settings"]);
	if (!actionState.kind) return null;

	return (
		<div className="space-y-4 rounded-lg border bg-muted/20 p-4 transition-[background-color,border-color,box-shadow] duration-200 ease-out">
			<div className="space-y-1">
				<h4 className="text-sm font-semibold">
					{actionState.kind === "disable"
						? t("settings:settings_mfa_disable")
						: t("settings:settings_mfa_regenerate_recovery")}
				</h4>
				<p className="text-sm text-muted-foreground">
					{actionState.kind === "disable"
						? t("settings:settings_mfa_disable_desc")
						: t("settings:settings_mfa_regenerate_desc")}{" "}
					{t("settings:settings_mfa_sensitive_action_desc")}
				</p>
			</div>
			<div className="max-w-sm space-y-2">
				<Label htmlFor="mfa-action-code">
					{t("settings:settings_mfa_code_or_recovery")}
				</Label>
				<Input
					id="mfa-action-code"
					value={actionState.code}
					autoComplete="one-time-code"
					onChange={(event) => onCodeChange(event.target.value)}
				/>
			</div>
			<div className="flex flex-wrap gap-2">
				<Button
					type="button"
					variant={actionState.kind === "disable" ? "destructive" : "default"}
					disabled={actionState.busy || actionState.code.trim().length === 0}
					onClick={onSubmit}
				>
					{actionState.busy ? (
						<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
					) : (
						<Icon name="Check" className="mr-2 size-4" />
					)}
					{actionState.kind === "disable"
						? t("settings:settings_mfa_disable")
						: t("settings:settings_mfa_regenerate_recovery")}
				</Button>
				<Button
					type="button"
					variant="outline"
					disabled={actionState.busy}
					onClick={onCancel}
				>
					{t("core:cancel")}
				</Button>
			</div>
		</div>
	);
}
