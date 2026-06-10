import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	externalAuthKindIconPath,
	normalizeExternalAuthIconUrl,
} from "@/lib/externalAuthProviders";
import { cn } from "@/lib/utils";
import type { ExternalAuthPublicProvider } from "@/types/api";
import {
	AnimateHeight,
	AnimateInlineSwap,
	AnimateText,
} from "./authAnimations";
import type { AuthMode } from "./types";

function normalizedUrl(value: string) {
	try {
		return new URL(value, document.baseURI).href;
	} catch {
		return value;
	}
}

function fallbackExternalAuthIcon(
	target: HTMLImageElement,
	configuredIcon: string,
	kindIcon: string,
) {
	if (
		configuredIcon &&
		kindIcon &&
		target.dataset.fallbackTried !== "1" &&
		normalizedUrl(target.src) !== normalizedUrl(kindIcon)
	) {
		target.dataset.fallbackTried = "1";
		target.src = kindIcon;
		return;
	}
	target.hidden = true;
}

function ExternalAuthProviderIcon({
	provider,
}: {
	provider: ExternalAuthPublicProvider;
}) {
	const configuredIcon = normalizeExternalAuthIconUrl(provider.icon_url);
	const kindIcon = externalAuthKindIconPath(provider.kind);
	const iconUrl = configuredIcon || kindIcon;

	if (iconUrl) {
		return (
			<img
				src={iconUrl}
				alt=""
				aria-hidden="true"
				className="mr-2 size-4 object-contain"
				onError={(event) => {
					fallbackExternalAuthIcon(
						event.currentTarget,
						configuredIcon,
						kindIcon,
					);
				}}
			/>
		);
	}

	return <Icon name="Globe" className="mr-2 size-4" />;
}

interface LoginAuthFormProps {
	checking: boolean;
	errors: Record<string, string>;
	extraField: string;
	extraLabel: string;
	extraPlaceholder: string;
	identifier: string;
	identifierLabel: string;
	identifierPlaceholder: string;
	isSubmitDisabled: boolean;
	mode: AuthMode;
	modeActionText: string;
	password: string;
	externalAuthBusyProvider: string | null;
	externalAuthLoading: boolean;
	externalAuthProviders: ExternalAuthPublicProvider[];
	passkeyLoginEnabled: boolean;
	passkeySubmitting: boolean;
	passkeySupported: boolean;
	registrationClosed: boolean;
	showPassword: boolean;
	submitLabel: string;
	submitting: boolean;
	onExtraFieldChange: (value: string) => void;
	onForgotPassword: () => void;
	onIdentifierChange: (value: string) => void;
	onPasswordChange: (value: string) => void;
	onExternalAuthLogin: (provider: ExternalAuthPublicProvider) => void;
	onPasskeyLogin: () => void;
	onShowPasswordChange: (show: boolean) => void;
	onSwitchAuthMode: (mode: Extract<AuthMode, "login" | "register">) => void;
}

export function LoginAuthForm({
	checking,
	errors,
	extraField,
	extraLabel,
	extraPlaceholder,
	identifier,
	identifierLabel,
	identifierPlaceholder,
	isSubmitDisabled,
	mode,
	modeActionText,
	onExtraFieldChange,
	onForgotPassword,
	onIdentifierChange,
	onPasswordChange,
	onExternalAuthLogin,
	onPasskeyLogin,
	onShowPasswordChange,
	onSwitchAuthMode,
	password,
	externalAuthBusyProvider,
	externalAuthLoading,
	externalAuthProviders,
	passkeyLoginEnabled,
	passkeySubmitting,
	passkeySupported,
	registrationClosed,
	showPassword,
	submitLabel,
	submitting,
}: LoginAuthFormProps) {
	const { t } = useTranslation(["login", "core"]);
	const requiresExtraField = mode === "register" || mode === "setup";
	const authMethodBusy =
		submitting || passkeySubmitting || externalAuthBusyProvider !== null;

	return (
		<>
			<div className="space-y-1.5">
				<div className="flex items-center justify-between">
					<Label htmlFor="identifier" className="text-sm">
						<AnimateText
							text={
								requiresExtraField ? identifierLabel : t("email_or_username")
							}
						/>
					</Label>
					<div className="flex min-h-4 items-center justify-end gap-2">
						<AnimateInlineSwap activeKey={`auth-mode:${mode}`}>
							{mode !== "idle" ? (
								<span
									className={cn(
										"text-xs text-muted-foreground/70 transition-opacity duration-150",
										checking && "opacity-0",
									)}
								>
									{modeActionText}
								</span>
							) : (
								<span className="w-0" />
							)}
						</AnimateInlineSwap>
						<AnimateInlineSwap
							activeKey={checking ? "auth-checking" : "auth-ready"}
						>
							{checking ? (
								<Icon
									name="Spinner"
									className="size-3 animate-spin text-muted-foreground"
								/>
							) : (
								<span className="w-0" />
							)}
						</AnimateInlineSwap>
					</div>
				</div>
				<Input
					id="identifier"
					placeholder={identifierPlaceholder}
					value={identifier}
					onChange={(event) => onIdentifierChange(event.target.value)}
					required
					autoFocus
					autoComplete={
						mode === "login" && passkeyLoginEnabled
							? "username webauthn"
							: "username"
					}
					className={cn(
						"h-10",
						errors.identifier &&
							"border-destructive focus-visible:ring-destructive",
					)}
				/>
				{errors.identifier ? (
					<p className="text-xs text-destructive">{errors.identifier}</p>
				) : null}
			</div>

			<AnimateHeight show={requiresExtraField}>
				<div className="mt-4 space-y-1.5">
					<Label htmlFor="extra" className="text-sm">
						<AnimateText text={extraLabel} />
					</Label>
					<Input
						id="extra"
						placeholder={extraPlaceholder}
						value={extraField}
						onChange={(event) => onExtraFieldChange(event.target.value)}
						required={requiresExtraField}
						autoComplete={identifier.includes("@") ? "off" : "email"}
						className={cn(
							"h-10",
							errors.extra &&
								"border-destructive focus-visible:ring-destructive",
						)}
					/>
					{errors.extra ? (
						<p className="text-xs text-destructive">{errors.extra}</p>
					) : null}
				</div>
			</AnimateHeight>

			<div className="mt-4 space-y-1.5">
				<Label htmlFor="password" className="text-sm">
					{t("core:password")}
				</Label>
				<div className="relative">
					<Input
						id="password"
						type={showPassword ? "text" : "password"}
						placeholder={t("core:password")}
						value={password}
						onChange={(event) => onPasswordChange(event.target.value)}
						required
						autoComplete={
							mode === "login" ? "current-password" : "new-password"
						}
						className={cn(
							"h-10 pr-10",
							errors.password &&
								"border-destructive focus-visible:ring-destructive",
						)}
					/>
					<button
						type="button"
						className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground transition-colors hover:text-foreground"
						onClick={() => onShowPasswordChange(!showPassword)}
						tabIndex={-1}
						aria-label={
							showPassword ? t("core:hide_password") : t("core:show_password")
						}
					>
						{showPassword ? (
							<Icon name="EyeSlash" className="size-4" />
						) : (
							<Icon name="Eye" className="size-4" />
						)}
					</button>
				</div>
				{errors.password ? (
					<p className="text-xs text-destructive">{errors.password}</p>
				) : null}
			</div>

			<div className="mt-3 flex justify-end">
				<button
					type="button"
					className="text-sm text-muted-foreground transition-colors hover:text-foreground"
					onClick={onForgotPassword}
				>
					{t("forgot_password")}
				</button>
			</div>

			<Button
				type="submit"
				className="mt-4 h-10 w-full"
				disabled={isSubmitDisabled}
			>
				{submitting ? (
					<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
				) : null}
				{submitLabel}
			</Button>

			{mode === "login" ? (
				<div className="mt-3 space-y-2">
					{passkeyLoginEnabled ? (
						<>
							<Button
								type="button"
								variant="outline"
								className="h-10 w-full"
								disabled={checking || authMethodBusy || !passkeySupported}
								onClick={onPasskeyLogin}
							>
								{passkeySubmitting ? (
									<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
								) : (
									<Icon name="Shield" className="mr-2 size-4" />
								)}
								{passkeySubmitting
									? t("passkey_signing_in")
									: t("passkey_sign_in")}
							</Button>
							{passkeySupported ? null : (
								<p className="text-center text-xs text-muted-foreground">
									{t("passkey_unsupported")}
								</p>
							)}
						</>
					) : null}
					{externalAuthLoading ? (
						<div className="flex h-10 items-center justify-center rounded-md border border-dashed text-sm text-muted-foreground">
							<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
							{t("external_auth_loading_providers")}
						</div>
					) : null}
					{externalAuthProviders.map((provider) => {
						const busy = externalAuthBusyProvider === provider.key;
						return (
							<Button
								key={`${provider.kind}:${provider.key}`}
								type="button"
								variant="outline"
								className="h-10 w-full"
								disabled={checking || authMethodBusy}
								onClick={() => onExternalAuthLogin(provider)}
							>
								{busy ? (
									<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
								) : (
									<ExternalAuthProviderIcon provider={provider} />
								)}
								<span className="truncate">
									{busy
										? t("external_auth_redirecting")
										: t("external_auth_sign_in_with", {
												provider: provider.display_name,
											})}
								</span>
							</Button>
						);
					})}
				</div>
			) : null}

			{mode !== "setup" && !checking && !registrationClosed ? (
				<p className="mt-6 text-center text-sm text-muted-foreground">
					{mode === "register"
						? t("already_have_account")
						: t("dont_have_account")}{" "}
					<button
						type="button"
						className="font-medium text-foreground transition-colors hover:text-primary"
						onClick={() =>
							onSwitchAuthMode(mode === "register" ? "login" : "register")
						}
					>
						{mode === "register" ? t("sign_in") : t("sign_up")}
					</button>
				</p>
			) : null}

			<p className="mt-8 text-center text-xs text-muted-foreground/50">
				Self-hosted cloud storage
			</p>
		</>
	);
}
