import type { FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { SecurityFormErrors } from "./types";

interface SecurityPasswordSectionProps {
	canSubmitPassword: boolean;
	confirmPassword: string;
	currentPassword: string;
	errors: SecurityFormErrors;
	newPassword: string;
	passwordBusy: boolean;
	onConfirmPasswordChange: (value: string) => void;
	onCurrentPasswordChange: (value: string) => void;
	onNewPasswordChange: (value: string) => void;
	onSubmit: (event: FormEvent<HTMLFormElement>) => void;
}

export function SecurityPasswordSection({
	canSubmitPassword,
	confirmPassword,
	currentPassword,
	errors,
	newPassword,
	onConfirmPasswordChange,
	onCurrentPasswordChange,
	onNewPasswordChange,
	onSubmit,
	passwordBusy,
}: SecurityPasswordSectionProps) {
	const { t } = useTranslation(["auth", "core", "settings"]);

	return (
		<form className="space-y-4" onSubmit={onSubmit}>
			<div className="space-y-1">
				<h3 className="text-sm font-semibold">
					{t("settings:settings_password_section")}
				</h3>
				<p className="text-sm text-muted-foreground">
					{t("settings:settings_password_section_desc")}
				</p>
			</div>

			<div className="space-y-2">
				<Label htmlFor="current-password">
					{t("settings:settings_password_current")}
				</Label>
				<Input
					id="current-password"
					type="password"
					value={currentPassword}
					disabled={passwordBusy}
					aria-invalid={errors.currentPassword ? true : undefined}
					aria-label={t("settings:settings_password_current")}
					autoComplete="current-password"
					onChange={(event) => onCurrentPasswordChange(event.target.value)}
				/>
				{errors.currentPassword ? (
					<p className="text-xs text-destructive">{errors.currentPassword}</p>
				) : null}
			</div>

			<div className="space-y-2">
				<Label htmlFor="new-password">
					{t("settings:settings_password_new")}
				</Label>
				<Input
					id="new-password"
					type="password"
					value={newPassword}
					disabled={passwordBusy}
					aria-invalid={errors.newPassword ? true : undefined}
					aria-label={t("settings:settings_password_new")}
					autoComplete="new-password"
					onChange={(event) => onNewPasswordChange(event.target.value)}
				/>
				<p className="text-xs text-muted-foreground">
					{t("settings:settings_password_hint")}
				</p>
				{errors.newPassword ? (
					<p className="text-xs text-destructive">{errors.newPassword}</p>
				) : null}
			</div>

			<div className="space-y-2">
				<Label htmlFor="confirm-password">
					{t("settings:settings_password_confirm")}
				</Label>
				<Input
					id="confirm-password"
					type="password"
					value={confirmPassword}
					disabled={passwordBusy}
					aria-invalid={errors.confirmPassword ? true : undefined}
					aria-label={t("settings:settings_password_confirm")}
					autoComplete="new-password"
					onChange={(event) => onConfirmPasswordChange(event.target.value)}
				/>
				{errors.confirmPassword ? (
					<p className="text-xs text-destructive">{errors.confirmPassword}</p>
				) : null}
			</div>

			<div className="flex justify-end border-t pt-4">
				<Button
					type="submit"
					className="min-w-28"
					disabled={!canSubmitPassword}
				>
					{t("core:save")}
				</Button>
			</div>
		</form>
	);
}
