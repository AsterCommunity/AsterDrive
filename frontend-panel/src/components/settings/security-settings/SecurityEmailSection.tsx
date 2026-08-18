import type { FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { MeResponse } from "@/types/api";

interface SecurityEmailSectionProps {
	canSubmitEmailChange: boolean;
	emailBusy: boolean;
	emailError?: string;
	newEmail: string;
	resendingEmailChange: boolean;
	user: MeResponse | null;
	onNewEmailChange: (value: string) => void;
	onResendEmailChange: () => void;
	onSubmit: (event: FormEvent<HTMLFormElement>) => void;
}

export function SecurityEmailSection({
	canSubmitEmailChange,
	emailBusy,
	emailError,
	newEmail,
	onNewEmailChange,
	onResendEmailChange,
	onSubmit,
	resendingEmailChange,
	user,
}: SecurityEmailSectionProps) {
	const { t } = useTranslation(["auth", "core", "settings"]);

	return (
		<form className="space-y-4" onSubmit={onSubmit}>
			<div className="space-y-1">
				<h3 className="text-sm font-semibold">
					{t("settings:settings_email_section")}
				</h3>
				<p className="text-sm text-muted-foreground">
					{t("settings:settings_email_section_desc")}
				</p>
			</div>

			<div className="grid gap-4 md:grid-cols-2">
				<div className="space-y-2">
					<Label htmlFor="current-email">
						{t("settings:settings_email_current")}
					</Label>
					<Input
						id="current-email"
						value={user?.email ?? ""}
						readOnly
						aria-label={t("settings:settings_email_current")}
					/>
				</div>

				<div className="space-y-2">
					<Label>{t("settings:settings_email_status")}</Label>
					<div className="flex min-h-10 items-center gap-2 rounded-lg border px-3 py-2">
						<Badge
							variant="outline"
							className={
								user?.email_verified
									? "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
									: "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300"
							}
						>
							{user?.email_verified
								? t("settings:settings_email_verified")
								: t("settings:settings_email_unverified")}
						</Badge>
						<span className="text-xs text-muted-foreground">
							{user?.email_verified
								? t("settings:settings_email_status_verified_desc")
								: t("settings:settings_email_status_unverified_desc")}
						</span>
					</div>
				</div>
			</div>

			{user?.pending_email ? (
				<div className="rounded-xl bg-muted/30 p-4">
					<div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
						<div className="space-y-1">
							<p className="text-sm font-medium">
								{t("settings:settings_email_pending")}
							</p>
							<p className="text-sm text-muted-foreground">
								{t("settings:settings_email_pending_desc", {
									email: user.pending_email,
								})}
							</p>
						</div>
						<Button
							type="button"
							variant="outline"
							disabled={resendingEmailChange}
							onClick={onResendEmailChange}
						>
							{resendingEmailChange ? (
								<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
							) : (
								<Icon name="ArrowClockwise" className="mr-2 size-4" />
							)}
							{resendingEmailChange
								? t("settings:settings_email_change_resending")
								: t("settings:settings_email_change_resend")}
						</Button>
					</div>
				</div>
			) : null}

			<div className="space-y-2">
				<Label htmlFor="new-email">{t("settings:settings_email_new")}</Label>
				<Input
					id="new-email"
					type="email"
					value={newEmail}
					disabled={emailBusy}
					aria-invalid={emailError ? true : undefined}
					aria-label={t("settings:settings_email_new")}
					autoComplete="email"
					placeholder="you@example.com"
					onChange={(event) => onNewEmailChange(event.target.value)}
				/>
				<p className="text-xs text-muted-foreground">
					{user?.email_verified
						? t("settings:settings_email_change_hint")
						: t("settings:settings_email_change_requires_verified")}
				</p>
				{emailError ? (
					<p className="text-xs text-destructive">{emailError}</p>
				) : null}
			</div>

			<div className="flex justify-end border-t pt-4">
				<Button
					type="submit"
					className="min-w-28"
					disabled={!canSubmitEmailChange}
				>
					{emailBusy ? (
						<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
					) : null}
					{emailBusy
						? t("settings:settings_email_change_requesting")
						: t("settings:settings_email_change_request")}
				</Button>
			</div>
		</form>
	);
}
