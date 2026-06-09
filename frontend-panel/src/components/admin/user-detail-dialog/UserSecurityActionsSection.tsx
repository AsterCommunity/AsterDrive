import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type { UserPasswordErrors } from "./types";

interface UserSecurityActionsSectionProps {
	confirmPasswordValue: string;
	mustChangePassword: boolean;
	onConfirmPasswordValueChange: (value: string) => void;
	onMfaReset: () => Promise<void>;
	onMustChangePasswordChange: (value: boolean) => void;
	onPasswordReset: () => Promise<void>;
	onPasswordValueChange: (value: string) => void;
	onSessionRevoke: () => Promise<void>;
	passwordErrors: UserPasswordErrors;
	passwordValue: string;
	resettingMfa: boolean;
	revokingSessions: boolean;
	savingPassword: boolean;
	savingProfile: boolean;
}

export function UserSecurityActionsSection({
	confirmPasswordValue,
	mustChangePassword,
	onConfirmPasswordValueChange,
	onMfaReset,
	onMustChangePasswordChange,
	onPasswordReset,
	onPasswordValueChange,
	onSessionRevoke,
	passwordErrors,
	passwordValue,
	resettingMfa,
	revokingSessions,
	savingPassword,
	savingProfile,
}: UserSecurityActionsSectionProps) {
	const { t } = useTranslation(["admin", "core"]);

	return (
		<section className="rounded-2xl border bg-background/60 p-6 max-lg:p-4">
			<div className="mb-4">
				<h4 className="text-base font-semibold text-foreground">
					{t("security_actions")}
				</h4>
				<p className="mt-1 text-sm text-muted-foreground">
					{t("security_actions_desc")}
				</p>
			</div>

			<div className="space-y-3">
				<div className="rounded-xl border bg-muted/10 p-5 max-lg:p-4">
					<div className="flex flex-col gap-4 md:flex-row md:items-start md:justify-between">
						<div className="max-w-2xl space-y-2">
							<div className="flex items-center gap-2">
								<h5 className="text-sm font-semibold text-foreground">
									{t("force_password_change")}
								</h5>
								<span className="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground">
									{mustChangePassword
										? t("force_password_change_enabled")
										: t("force_password_change_disabled")}
								</span>
							</div>
							<p className="text-sm text-muted-foreground">
								{t("force_password_change_desc")}
							</p>
						</div>
						<Switch
							aria-label={t("force_password_change")}
							checked={mustChangePassword}
							disabled={savingProfile}
							onCheckedChange={onMustChangePasswordChange}
							className="md:mt-1"
						/>
					</div>
				</div>

				<div className="rounded-xl border bg-muted/10 p-5 max-lg:p-4">
					<div className="mb-3">
						<h5 className="text-sm font-semibold text-foreground">
							{t("reset_password")}
						</h5>
						<p className="mt-1 text-sm text-muted-foreground">
							{t("admin_reset_password_desc")}
						</p>
					</div>
					<div className="grid gap-4 md:grid-cols-2">
						<div className="space-y-2">
							<Label htmlFor="user-reset-password">{t("core:password")}</Label>
							<Input
								id="user-reset-password"
								name="admin-reset-user-password"
								type="password"
								value={passwordValue}
								onChange={(event) => onPasswordValueChange(event.target.value)}
								autoComplete="new-password"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								disabled={savingPassword}
								aria-invalid={passwordErrors.password ? true : undefined}
							/>
							<p className="text-xs text-muted-foreground">
								{t("reset_password_hint")}
							</p>
							{passwordErrors.password ? (
								<p className="text-xs text-destructive">
									{passwordErrors.password}
								</p>
							) : null}
						</div>
						<div className="space-y-2">
							<Label htmlFor="user-reset-password-confirm">
								{t("confirm_password")}
							</Label>
							<Input
								id="user-reset-password-confirm"
								name="admin-reset-user-password-confirm"
								type="password"
								value={confirmPasswordValue}
								onChange={(event) =>
									onConfirmPasswordValueChange(event.target.value)
								}
								autoComplete="new-password"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								disabled={savingPassword}
								aria-invalid={passwordErrors.confirm ? true : undefined}
							/>
							{passwordErrors.confirm ? (
								<p className="text-xs text-destructive">
									{passwordErrors.confirm}
								</p>
							) : null}
						</div>
					</div>
					<div className="mt-4 flex justify-end">
						<Button
							type="button"
							onClick={() => void onPasswordReset()}
							disabled={
								savingPassword ||
								passwordValue.length === 0 ||
								confirmPasswordValue.length === 0
							}
						>
							{t("reset_password")}
						</Button>
					</div>
				</div>

				<div className="rounded-xl border border-destructive/30 bg-destructive/5 p-5 max-lg:p-4">
					<div className="flex flex-col gap-4 md:flex-row md:items-start md:justify-between">
						<div className="max-w-2xl space-y-2">
							<h5 className="text-sm font-semibold text-foreground">
								{t("reset_mfa")}
							</h5>
							<p className="text-sm text-muted-foreground">
								{t("reset_mfa_desc")}
							</p>
							<p className="text-xs text-muted-foreground">
								{t("reset_mfa_hint")}
							</p>
						</div>
						<Button
							type="button"
							variant="destructive"
							onClick={() => void onMfaReset()}
							disabled={resettingMfa}
							className="md:shrink-0"
						>
							{resettingMfa ? (
								<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
							) : (
								<Icon name="Shield" className="mr-1 size-4" />
							)}
							{t("reset_mfa")}
						</Button>
					</div>
				</div>

				<div className="rounded-xl border border-destructive/30 bg-destructive/5 p-5 max-lg:p-4">
					<div className="flex flex-col gap-4 md:flex-row md:items-start md:justify-between">
						<div className="max-w-2xl space-y-2">
							<h5 className="text-sm font-semibold text-foreground">
								{t("revoke_sessions")}
							</h5>
							<p className="text-sm text-muted-foreground">
								{t("revoke_sessions_desc")}
							</p>
							<p className="text-xs text-muted-foreground">
								{t("revoke_sessions_hint")}
							</p>
						</div>
						<Button
							type="button"
							variant="destructive"
							onClick={() => void onSessionRevoke()}
							disabled={revokingSessions}
							className="md:shrink-0"
						>
							{revokingSessions ? (
								<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
							) : (
								<Icon name="SignOut" className="mr-1 size-4" />
							)}
							{t("revoke_sessions")}
						</Button>
					</div>
				</div>
			</div>
		</section>
	);
}
