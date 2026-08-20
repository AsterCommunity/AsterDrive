import type { ZodType } from "zod/v4";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { cn } from "@/lib/utils";
import { AnimateHeight, AnimateMeasuredHeight } from "./authAnimations";

type Translate = (key: string) => string;
type ExternalAuthRecoveryMode = "password" | "email";

interface ExternalAuthRecoveryPanelProps {
	email: string;
	emailError: string;
	emailSchema: ZodType;
	identifier: string;
	identifierError: string;
	mode: ExternalAuthRecoveryMode;
	passwordLoginEnabled: boolean;
	password: string;
	passwordError: string;
	sent: boolean;
	submittingEmail: boolean;
	submittingPassword: boolean;
	t: Translate;
	onBack: () => void;
	onEmailChange: (value: string, error: string) => void;
	onIdentifierChange: (value: string) => void;
	onModeChange: (mode: ExternalAuthRecoveryMode) => void;
	onPasswordChange: (value: string) => void;
}

export function ExternalAuthRecoveryPanel({
	email,
	emailError,
	emailSchema,
	identifier,
	identifierError,
	mode,
	passwordLoginEnabled,
	password,
	passwordError,
	sent,
	submittingEmail,
	submittingPassword,
	t,
	onBack,
	onEmailChange,
	onIdentifierChange,
	onModeChange,
	onPasswordChange,
}: ExternalAuthRecoveryPanelProps) {
	const busy = submittingEmail || submittingPassword;
	const tabPanelClassName =
		"space-y-4 animate-in fade-in slide-in-from-bottom-1 duration-200 motion-reduce:animate-none";

	return (
		<div className="space-y-4">
			<div className="flex items-start gap-3">
				<div className="rounded-xl bg-primary/10 p-2 text-primary">
					<Icon name={sent ? "Check" : "Link"} className="size-5" />
				</div>
				<div className="space-y-1">
					<p className="text-sm font-medium">
						{sent
							? t("external_auth_email_verification_sent_title")
							: t("external_auth_email_verification_panel_title")}
					</p>
					<p className="text-sm text-muted-foreground">
						{sent
							? t("external_auth_email_verification_sent_hint")
							: t("external_auth_email_verification_hint")}
					</p>
					{sent && email ? (
						<p className="text-xs text-muted-foreground">
							{t("core:email")}: {email}
						</p>
					) : null}
				</div>
			</div>

			<AnimateHeight show={!sent}>
				<Tabs
					value={mode}
					onValueChange={(value) =>
						onModeChange(value === "email" ? "email" : "password")
					}
				>
					<TabsList
						className={cn(
							"grid h-9 w-full",
							passwordLoginEnabled ? "grid-cols-2" : "grid-cols-1",
						)}
					>
						{passwordLoginEnabled ? (
							<TabsTrigger value="password">
								<Icon name="Lock" className="size-4" />
								{t("external_auth_password_link_tab")}
							</TabsTrigger>
						) : null}
						<TabsTrigger value="email">
							<Icon name="EnvelopeSimple" className="size-4" />
							{t("external_auth_email_verification_tab")}
						</TabsTrigger>
					</TabsList>

					<AnimateMeasuredHeight contentClassName="pt-4">
						{mode === "password" && passwordLoginEnabled ? (
							<div key="password" className={tabPanelClassName}>
								<div className="space-y-1.5">
									<Label
										htmlFor="external-auth-password-link-identifier"
										className="text-sm"
									>
										{t("email_or_username")}
									</Label>
									<Input
										id="external-auth-password-link-identifier"
										placeholder="you@example.com"
										value={identifier}
										onChange={(event) => onIdentifierChange(event.target.value)}
										autoFocus
										autoComplete="username"
										className={cn(
											"h-10",
											identifierError &&
												"border-destructive focus-visible:ring-destructive",
										)}
									/>
									{identifierError ? (
										<p className="text-xs text-destructive">
											{identifierError}
										</p>
									) : null}
								</div>

								<div className="space-y-1.5">
									<Label
										htmlFor="external-auth-password-link-password"
										className="text-sm"
									>
										{t("core:password")}
									</Label>
									<Input
										id="external-auth-password-link-password"
										type="password"
										placeholder={t("core:password")}
										value={password}
										onChange={(event) => onPasswordChange(event.target.value)}
										autoComplete="current-password"
										className={cn(
											"h-10",
											passwordError &&
												"border-destructive focus-visible:ring-destructive",
										)}
									/>
									{passwordError ? (
										<p className="text-xs text-destructive">{passwordError}</p>
									) : null}
								</div>

								<Button
									type="submit"
									className="h-10 w-full"
									disabled={
										busy ||
										identifier.trim().length === 0 ||
										password.length === 0
									}
								>
									{submittingPassword ? (
										<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
									) : (
										<Icon name="Link" className="mr-2 size-4" />
									)}
									{submittingPassword
										? t("external_auth_password_link_signing_in")
										: t("external_auth_password_link_submit")}
								</Button>
							</div>
						) : (
							<div key="email" className={tabPanelClassName}>
								<div className="space-y-1.5">
									<Label
										htmlFor="external-auth-recovery-email"
										className="text-sm"
									>
										{t("core:email")}
									</Label>
									<Input
										id="external-auth-recovery-email"
										placeholder="you@example.com"
										value={email}
										onChange={(event) => {
											const nextValue = event.target.value;
											const result = emailSchema.safeParse(nextValue);
											onEmailChange(
												nextValue,
												result.success
													? ""
													: (result.error.issues[0]?.message ?? ""),
											);
										}}
										autoComplete="email"
										className={cn(
											"h-10",
											emailError &&
												"border-destructive focus-visible:ring-destructive",
										)}
									/>
									{emailError ? (
										<p className="text-xs text-destructive">{emailError}</p>
									) : null}
								</div>

								<Button
									type="submit"
									className="h-10 w-full"
									disabled={busy || email.trim().length === 0 || !!emailError}
								>
									{submittingEmail ? (
										<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
									) : (
										<Icon name="EnvelopeSimple" className="mr-2 size-4" />
									)}
									{submittingEmail
										? t("external_auth_email_verification_sending")
										: t("external_auth_email_verification_send")}
								</Button>
							</div>
						)}
					</AnimateMeasuredHeight>
				</Tabs>
			</AnimateHeight>

			<Button
				type="button"
				variant="outline"
				className="h-10 w-full"
				onClick={onBack}
			>
				<Icon name="ArrowLeft" className="mr-2 size-4" />
				{t("back_to_sign_in")}
			</Button>
		</div>
	);
}
