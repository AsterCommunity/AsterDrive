import type { FormEvent } from "react";
import type { z } from "zod/v4";
import { AsterDriveWordmark } from "@/components/common/AsterDriveWordmark";
import { cn } from "@/lib/utils";
import type { MfaMethod } from "@/services/authService";
import type { ExternalAuthPublicProvider } from "@/types/api";
import { AnimateSwap } from "./authAnimations";
import { ExternalAuthRecoveryPanel } from "./ExternalAuthRecoveryPanel";
import { LoginAuthForm } from "./LoginAuthForm";
import { LoginBrandPanel } from "./LoginBrandPanel";
import { LoginHeader } from "./LoginHeader";
import type {
	ExternalAuthRecoveryState,
	MfaPanelState,
	PasswordResetPanelState,
} from "./loginPageState";
import { MfaChallengePanel } from "./MfaChallengePanel";
import { PasswordResetRequestPanel } from "./PasswordResetRequestPanel";
import {
	PendingActivationPanel,
	type PendingActivationState,
} from "./PendingActivationPanel";
import type { AuthMode } from "./types";

type Translate = (key: string, options?: Record<string, unknown>) => string;

interface LoginPageViewProps {
	checking: boolean;
	description: string;
	errors: Record<string, string>;
	exiting: boolean;
	externalAuthBusyProvider: string | null;
	externalAuthLoading: boolean;
	externalAuthProviders: ExternalAuthPublicProvider[];
	externalAuthRecovery: ExternalAuthRecoveryState | null;
	extraField: string;
	extraLabel: string;
	extraPlaceholder: string;
	identifier: string;
	identifierLabel: string;
	identifierPlaceholder: string;
	isSubmitDisabled: boolean;
	passkeyLoginEnabled: boolean;
	mfaPanel: MfaPanelState | null;
	mode: AuthMode;
	modeActionText: string;
	passkeySubmitting: boolean;
	passkeySupported: boolean;
	password: string;
	passwordResetPanel: PasswordResetPanelState | null;
	pendingActivation: PendingActivationState | null;
	registrationClosed: boolean;
	resendingActivation: boolean;
	showPassword: boolean;
	submitLabel: string;
	submitting: boolean;
	t: Translate;
	title: string;
	emailSchema: z.ZodType;
	onExternalAuthEmailChange: (value: string, error: string) => void;
	onExternalAuthIdentifierChange: (value: string) => void;
	onExternalAuthLogin: (provider: ExternalAuthPublicProvider) => void;
	onExternalAuthModeChange: (mode: "password" | "email") => void;
	onExternalAuthPasswordChange: (value: string) => void;
	onExternalAuthRecoveryBack: () => void;
	onExtraFieldChange: (value: string) => void;
	onForgotPassword: () => void;
	onIdentifierChange: (value: string) => void;
	onMfaBack: () => void;
	onMfaCodeChange: (value: string) => void;
	onMfaEmailCodeSend: () => void;
	onMfaMethodChange: (method: MfaMethod) => void;
	onPasskeyLogin: () => void;
	onPasswordChange: (value: string) => void;
	onPasswordResetBack: () => void;
	onPasswordResetEmailChange: (value: string, error: string) => void;
	onPasswordResetSubmit: () => void;
	onPendingActivationReset: () => void;
	onResendActivation: () => void;
	onShowPasswordChange: (show: boolean) => void;
	onSubmit: (event: FormEvent) => void;
	onSwitchAuthMode: (mode: Extract<AuthMode, "login" | "register">) => void;
}

export function LoginPageView({
	checking,
	description,
	emailSchema,
	errors,
	exiting,
	externalAuthBusyProvider,
	externalAuthLoading,
	externalAuthProviders,
	externalAuthRecovery,
	extraField,
	extraLabel,
	extraPlaceholder,
	identifier,
	identifierLabel,
	identifierPlaceholder,
	isSubmitDisabled,
	passkeyLoginEnabled,
	mfaPanel,
	mode,
	modeActionText,
	onExternalAuthEmailChange,
	onExternalAuthIdentifierChange,
	onExternalAuthLogin,
	onExternalAuthModeChange,
	onExternalAuthPasswordChange,
	onExternalAuthRecoveryBack,
	onExtraFieldChange,
	onForgotPassword,
	onIdentifierChange,
	onMfaBack,
	onMfaCodeChange,
	onMfaEmailCodeSend,
	onMfaMethodChange,
	onPasskeyLogin,
	onPasswordChange,
	onPasswordResetBack,
	onPasswordResetEmailChange,
	onPasswordResetSubmit,
	onPendingActivationReset,
	onResendActivation,
	onShowPasswordChange,
	onSubmit,
	onSwitchAuthMode,
	passkeySubmitting,
	passkeySupported,
	password,
	passwordResetPanel,
	pendingActivation,
	registrationClosed,
	resendingActivation,
	showPassword,
	submitLabel,
	submitting,
	t,
	title,
}: LoginPageViewProps) {
	const activeKey = pendingActivation
		? "pending-activation"
		: passwordResetPanel
			? "password-reset-request"
			: externalAuthRecovery
				? "external-auth-recovery"
				: mfaPanel
					? "mfa-challenge"
					: "auth-form";

	return (
		<div
			className={cn(
				"min-h-screen flex transition-all duration-300 ease-out",
				exiting && "opacity-0 scale-[1.02]",
			)}
		>
			<LoginBrandPanel />

			<div className="flex-1 flex items-center justify-center bg-background p-6">
				<div className="w-full max-w-sm">
					<div className="lg:hidden text-center mb-8">
						<AsterDriveWordmark
							alt="AsterDrive"
							className="mx-auto h-16 w-auto"
						/>
					</div>

					<LoginHeader title={title} description={description} />

					<form onSubmit={onSubmit}>
						<AnimateSwap activeKey={activeKey}>
							{pendingActivation ? (
								<PendingActivationPanel
									pendingActivation={pendingActivation}
									resendingActivation={resendingActivation}
									t={t}
									onResendActivation={onResendActivation}
									onReset={onPendingActivationReset}
								/>
							) : passwordResetPanel ? (
								<PasswordResetRequestPanel
									emailSchema={emailSchema}
									passwordResetEmail={passwordResetPanel.email}
									passwordResetError={passwordResetPanel.error}
									requestingPasswordReset={passwordResetPanel.requesting}
									t={t}
									onBack={onPasswordResetBack}
									onEmailChange={onPasswordResetEmailChange}
									onSubmit={onPasswordResetSubmit}
								/>
							) : externalAuthRecovery ? (
								<ExternalAuthRecoveryPanel
									email={externalAuthRecovery.email}
									emailError={externalAuthRecovery.emailError}
									emailSchema={emailSchema}
									identifier={externalAuthRecovery.passwordIdentifier}
									identifierError={externalAuthRecovery.passwordIdentifierError}
									mode={externalAuthRecovery.mode}
									password={externalAuthRecovery.password}
									passwordError={externalAuthRecovery.passwordError}
									sent={externalAuthRecovery.sent}
									submittingEmail={externalAuthRecovery.emailSubmitting}
									submittingPassword={externalAuthRecovery.passwordSubmitting}
									t={t}
									onBack={onExternalAuthRecoveryBack}
									onEmailChange={onExternalAuthEmailChange}
									onIdentifierChange={onExternalAuthIdentifierChange}
									onModeChange={onExternalAuthModeChange}
									onPasswordChange={onExternalAuthPasswordChange}
								/>
							) : mfaPanel ? (
								<MfaChallengePanel
									code={mfaPanel.code}
									emailCodeError={mfaPanel.emailCodeError}
									emailCodeExpiresAt={mfaPanel.emailCodeExpiresAt}
									emailCodeResendAt={mfaPanel.emailCodeResendAt}
									emailCodeSending={mfaPanel.emailCodeSending}
									emailCodeSent={mfaPanel.emailCodeSent}
									error={mfaPanel.error}
									expired={mfaPanel.challenge.expiresAt <= mfaPanel.now}
									methods={mfaPanel.challenge.methods}
									remainingSeconds={Math.max(
										0,
										Math.ceil(
											(mfaPanel.challenge.expiresAt - mfaPanel.now) / 1000,
										),
									)}
									selectedMethod={mfaPanel.selectedMethod}
									submitting={mfaPanel.submitting}
									t={t}
									onBack={onMfaBack}
									onCodeChange={onMfaCodeChange}
									onEmailCodeSend={onMfaEmailCodeSend}
									onMethodChange={onMfaMethodChange}
								/>
							) : (
								<LoginAuthForm
									checking={checking}
									errors={errors}
									extraField={extraField}
									extraLabel={extraLabel}
									extraPlaceholder={extraPlaceholder}
									identifier={identifier}
									identifierLabel={identifierLabel}
									identifierPlaceholder={identifierPlaceholder}
									isSubmitDisabled={isSubmitDisabled}
									passkeyLoginEnabled={passkeyLoginEnabled}
									mode={mode}
									modeActionText={modeActionText}
									password={password}
									passkeySubmitting={passkeySubmitting}
									passkeySupported={passkeySupported}
									externalAuthBusyProvider={externalAuthBusyProvider}
									externalAuthLoading={externalAuthLoading}
									externalAuthProviders={externalAuthProviders}
									registrationClosed={registrationClosed}
									showPassword={showPassword}
									submitLabel={submitLabel}
									submitting={submitting}
									onExtraFieldChange={onExtraFieldChange}
									onForgotPassword={onForgotPassword}
									onIdentifierChange={onIdentifierChange}
									onPasswordChange={onPasswordChange}
									onPasskeyLogin={onPasskeyLogin}
									onExternalAuthLogin={onExternalAuthLogin}
									onShowPasswordChange={onShowPasswordChange}
									onSwitchAuthMode={onSwitchAuthMode}
								/>
							)}
						</AnimateSwap>
					</form>
				</div>
			</div>
		</div>
	);
}
