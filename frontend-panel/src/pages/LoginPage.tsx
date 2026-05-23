import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLocation, useNavigate } from "react-router-dom";
import { toast } from "sonner";
import type { z } from "zod/v4";
import { AsterDriveWordmark } from "@/components/common/AsterDriveWordmark";
import { handleApiError } from "@/hooks/useApiError";
import { usePageTitle } from "@/hooks/usePageTitle";
import {
	clearContactVerificationRedirectSearch,
	getContactVerificationRedirectState,
} from "@/lib/contactVerificationRedirect";
import { logger } from "@/lib/logger";
import {
	clearPasswordResetRedirectSearch,
	getPasswordResetRedirectState,
} from "@/lib/passwordResetRedirect";
import { cn } from "@/lib/utils";
import {
	emailSchema,
	existingPasswordSchema,
	passwordSchema,
	usernameSchema,
} from "@/lib/validation";
import {
	getPasskeyCredential,
	isConditionalPasskeyLoginAvailable,
	isWebAuthnSupported,
	WebAuthnCancelledError,
	WebAuthnUnsupportedError,
} from "@/lib/webauthn";
import { authService } from "@/services/authService";
import { ApiError } from "@/services/http";
import { useAuthStore } from "@/stores/authStore";
import type { ExternalAuthPublicProvider } from "@/types/api";
import { ErrorCode } from "@/types/api-helpers";
import { AnimateSwap } from "./login/authAnimations";
import { ExternalAuthRecoveryPanel } from "./login/ExternalAuthRecoveryPanel";
import { LoginAuthForm } from "./login/LoginAuthForm";
import { LoginBrandPanel } from "./login/LoginBrandPanel";
import { LoginHeader } from "./login/LoginHeader";
import { PasswordResetRequestPanel } from "./login/PasswordResetRequestPanel";
import {
	PendingActivationPanel,
	type PendingActivationState,
} from "./login/PendingActivationPanel";
import type { AuthMode } from "./login/types";

// ── Component ───────────────────────────────────────────────

export default function LoginPage() {
	const { t } = useTranslation(["auth", "core", "settings"]);
	const { hash, pathname, search } = useLocation();
	const navigate = useNavigate();
	const login = useAuthStore((s) => s.login);
	const refreshUser = useAuthStore((s) => s.refreshUser);
	const syncSession = useAuthStore((s) => s.syncSession);
	const conditionalPasskeyAbortRef = useRef<AbortController | null>(null);
	const conditionalPasskeySupportedRef = useRef(false);

	// The first field is always visible — it doubles as username or email
	const [identifier, setIdentifier] = useState("");
	// The extra field only shows for register/setup — it's whatever identifier is NOT
	const [extraField, setExtraField] = useState("");
	const [password, setPassword] = useState("");
	const [showPassword, setShowPassword] = useState(false);

	const [mode, setMode] = useState<AuthMode>("idle");
	const [checking, setChecking] = useState(true);
	const [submitting, setSubmitting] = useState(false);
	const [resendingActivation, setResendingActivation] = useState(false);
	const [requestingPasswordReset, setRequestingPasswordReset] = useState(false);
	const [passkeySubmitting, setPasskeySubmitting] = useState(false);
	const [externalAuthProviders, setExternalAuthProviders] = useState<
		ExternalAuthPublicProvider[]
	>([]);
	const [externalAuthLoading, setExternalAuthLoading] = useState(false);
	const [externalAuthBusyProvider, setExternalAuthBusyProvider] = useState<
		string | null
	>(null);
	const [passkeySupported, setPasskeySupported] = useState(false);
	const [registrationClosed, setRegistrationClosed] = useState(false);
	const [exiting, setExiting] = useState(false);
	const [errors, setErrors] = useState<Record<string, string>>({});
	const [pendingActivation, setPendingActivation] =
		useState<PendingActivationState | null>(null);
	const [showPasswordResetRequest, setShowPasswordResetRequest] =
		useState(false);
	const [passwordResetEmail, setPasswordResetEmail] = useState("");
	const [passwordResetError, setPasswordResetError] = useState("");
	const [externalAuthRecoveryFlow, setExternalAuthRecoveryFlow] = useState<
		string | null
	>(null);
	const [externalAuthRecoveryMode, setExternalAuthRecoveryMode] = useState<
		"password" | "email"
	>("password");
	const [
		externalAuthPasswordLinkIdentifier,
		setExternalAuthPasswordLinkIdentifier,
	] = useState("");
	const [
		externalAuthPasswordLinkPassword,
		setExternalAuthPasswordLinkPassword,
	] = useState("");
	const [
		externalAuthPasswordLinkIdentifierError,
		setExternalAuthPasswordLinkIdentifierError,
	] = useState("");
	const [
		externalAuthPasswordLinkPasswordError,
		setExternalAuthPasswordLinkPasswordError,
	] = useState("");
	const [
		externalAuthPasswordLinkSubmitting,
		setExternalAuthPasswordLinkSubmitting,
	] = useState(false);
	const [
		externalAuthEmailVerificationEmail,
		setExternalAuthEmailVerificationEmail,
	] = useState("");
	const [
		externalAuthEmailVerificationError,
		setExternalAuthEmailVerificationError,
	] = useState("");
	const [
		externalAuthEmailVerificationSubmitting,
		setExternalAuthEmailVerificationSubmitting,
	] = useState(false);
	const [
		externalAuthEmailVerificationSent,
		setExternalAuthEmailVerificationSent,
	] = useState(false);

	// Is the identifier an email address?
	const isEmail = identifier.includes("@");

	// In register/setup: identifier is one field, extraField is the other
	// If identifier is email → extraField is username (and vice versa)
	const identifierLabel = isEmail ? t("core:email") : t("core:username");
	const extraLabel = isEmail ? t("core:username") : t("core:email");
	const requiresExtraField = mode === "register" || mode === "setup";
	const identifierPlaceholder =
		requiresExtraField && !isEmail ? t("choose_username") : "you@example.com";
	const extraPlaceholder = isEmail ? t("choose_username") : "you@example.com";
	const passwordResetPrefill = isEmail
		? identifier.trim()
		: extraField.includes("@")
			? extraField.trim()
			: "";
	const loginSuccessMessage = t("login_success");
	const modeActionText = pendingActivation
		? t("activation_pending_title")
		: externalAuthRecoveryFlow
			? t("external_auth_email_verification_title")
			: showPasswordResetRequest
				? t("forgot_password_title")
				: mode === "login"
					? t("sign_in")
					: mode === "register"
						? t("sign_up")
						: mode === "setup"
							? t("create_admin")
							: "";
	usePageTitle(modeActionText || t("sign_in"));
	const isSubmitDisabled =
		submitting ||
		passkeySubmitting ||
		externalAuthBusyProvider !== null ||
		checking ||
		identifier.trim().length === 0 ||
		password.length === 0 ||
		(requiresExtraField && extraField.trim().length === 0);

	useEffect(() => {
		const searchParams = new URLSearchParams(search);
		const externalAuthStatus = searchParams.get("external_auth");
		const externalAuthMessage = searchParams.get("message");
		const externalAuthFlow = searchParams.get("flow");
		const verification = getContactVerificationRedirectState(search);
		const passwordReset = getPasswordResetRedirectState(search);
		if (!verification && !passwordReset && !externalAuthStatus) {
			return;
		}

		if (verification) {
			switch (verification.status) {
				case "email-changed":
					if (!verification.email) {
						return;
					}
					toast.success(
						t("settings:settings_email_change_confirmed_login_hint", {
							email: verification.email,
						}),
						{
							id: `contact-verification-email-changed-login:${verification.email}`,
						},
					);
					break;
				case "expired":
					toast.error(t("verify_contact_expired_title"), {
						description: t("verify_contact_expired_desc"),
						id: "contact-verification-expired-login",
					});
					break;
				case "invalid":
					toast.error(t("verify_contact_invalid_title"), {
						description: t("verify_contact_invalid_desc"),
						id: "contact-verification-invalid-login",
					});
					break;
				case "missing":
					toast.error(t("verify_contact_missing_token_title"), {
						description: t("verify_contact_missing_token_desc"),
						id: "contact-verification-missing-login",
					});
					break;
				case "register-activated":
					toast.success(t("activation_confirmed"), {
						id: "contact-verification-register-activated-login",
					});
					break;
			}
		}

		if (passwordReset?.status === "success") {
			toast.success(t("password_reset_success_login"), {
				id: "password-reset-success-login",
			});
		}

		if (externalAuthStatus === "email_required" && externalAuthFlow) {
			setExternalAuthRecoveryFlow(externalAuthFlow);
			setExternalAuthEmailVerificationEmail(passwordResetPrefill);
			setExternalAuthRecoveryMode("password");
			setExternalAuthPasswordLinkIdentifier(identifier.trim());
			setExternalAuthPasswordLinkPassword("");
			setExternalAuthPasswordLinkIdentifierError("");
			setExternalAuthPasswordLinkPasswordError("");
			setExternalAuthEmailVerificationError("");
			setExternalAuthEmailVerificationSent(false);
			setShowPasswordResetRequest(false);
			setPendingActivation(null);
			setMode("login");
		} else if (externalAuthStatus === "email_verification_missing") {
			toast.error(t("external_auth_email_verification_missing_token_title"), {
				description: t("external_auth_email_verification_missing_token_desc"),
				id: "external-auth-recovery-missing",
			});
		} else if (externalAuthStatus === "email_verification_invalid") {
			toast.error(t("external_auth_email_verification_invalid_title"), {
				description: t("external_auth_email_verification_invalid_desc"),
				id: "external-auth-recovery-invalid",
			});
		} else if (externalAuthStatus === "email_verification_expired") {
			toast.error(t("external_auth_email_verification_expired_title"), {
				description: t("external_auth_email_verification_expired_desc"),
				id: "external-auth-recovery-expired",
			});
		} else if (externalAuthStatus === "error") {
			toast.error(t("external_auth_login_failed"), {
				description:
					externalAuthMessage || t("external_auth_login_failed_desc"),
				id: "external-auth-login-error",
			});
		}

		searchParams.delete("external_auth");
		searchParams.delete("code");
		searchParams.delete("message");
		searchParams.delete("flow");
		searchParams.delete("return_path");
		const cleanedSearch = searchParams.toString();

		navigate(
			{
				hash,
				pathname,
				search: clearPasswordResetRedirectSearch(
					clearContactVerificationRedirectSearch(
						cleanedSearch ? `?${cleanedSearch}` : "",
					),
				),
			},
			{ replace: true },
		);
	}, [hash, pathname, search, navigate, identifier, passwordResetPrefill, t]);

	useEffect(() => {
		let cancelled = false;

		void authService
			.check()
			.then((result) => {
				if (cancelled) return;
				if (!result.has_users) {
					setRegistrationClosed(false);
					setMode("setup");
					return;
				}

				setRegistrationClosed(result.allow_user_registration === false);
				setMode("login");
			})
			.catch(() => {
				if (cancelled) return;
				setRegistrationClosed(false);
				setMode("login");
			})
			.finally(() => {
				if (!cancelled) {
					setChecking(false);
				}
			});

		return () => {
			cancelled = true;
		};
	}, []);

	useEffect(() => {
		setPasskeySupported(isWebAuthnSupported());
	}, []);

	useEffect(() => {
		let cancelled = false;

		setExternalAuthLoading(true);
		void authService
			.listExternalAuthProviders()
			.then((providers) => {
				if (!cancelled) {
					setExternalAuthProviders(providers);
				}
			})
			.catch((error) => {
				if (!cancelled) {
					logger.warn("failed to load external auth providers", error);
				}
			})
			.finally(() => {
				if (!cancelled) {
					setExternalAuthLoading(false);
				}
			});

		return () => {
			cancelled = true;
		};
	}, []);

	useEffect(() => {
		let cancelled = false;

		void isConditionalPasskeyLoginAvailable()
			.then((available) => {
				if (!cancelled) {
					conditionalPasskeySupportedRef.current = available;
				}
			})
			.catch((error) => {
				if (!cancelled) {
					conditionalPasskeySupportedRef.current = false;
				}
				logger.warn("conditional passkey support detection failed", error);
			});

		return () => {
			cancelled = true;
		};
	}, []);

	// ── Live validation ──

	const validateSingle = (field: string, value: string, schema: z.ZodType) => {
		const result = schema.safeParse(value);
		setErrors((prev) => {
			if (result.success) {
				const next = { ...prev };
				delete next[field];
				return next;
			}
			return { ...prev, [field]: result.error.issues[0]?.message ?? "" };
		});
	};

	// ── Submit validation ──

	const validate = (): boolean => {
		const errs: Record<string, string> = {};

		// Validate identifier as username or email
		const idSchema = isEmail ? emailSchema : usernameSchema;
		const idResult = idSchema.safeParse(identifier.trim());
		if (!idResult.success)
			errs.identifier = idResult.error.issues[0]?.message ?? "";

		// Validate extra field for register/setup
		if (mode === "register" || mode === "setup") {
			const extraSchema = isEmail ? usernameSchema : emailSchema;
			const extraResult = extraSchema.safeParse(extraField.trim());
			if (!extraResult.success)
				errs.extra = extraResult.error.issues[0]?.message ?? "";
		}

		const passwordValidationSchema =
			mode === "login" ? existingPasswordSchema : passwordSchema;
		const pwResult = passwordValidationSchema.safeParse(password);
		if (!pwResult.success)
			errs.password = pwResult.error.issues[0]?.message ?? "";

		setErrors(errs);
		return Object.keys(errs).length === 0;
	};

	// ── Exit animation → navigate ──

	const exitAndNavigate = useCallback(() => {
		setExiting(true);
		setTimeout(() => navigate("/", { replace: true }), 350);
	}, [navigate]);

	const resetPendingActivation = () => {
		setPendingActivation(null);
		setErrors({});
		setPassword("");
		setShowPassword(false);
	};

	const closePasswordResetRequest = () => {
		setShowPasswordResetRequest(false);
		setPasswordResetError("");
	};

	const closeExternalAuthRecovery = () => {
		setExternalAuthRecoveryFlow(null);
		setExternalAuthRecoveryMode("password");
		setExternalAuthPasswordLinkIdentifier("");
		setExternalAuthPasswordLinkPassword("");
		setExternalAuthPasswordLinkIdentifierError("");
		setExternalAuthPasswordLinkPasswordError("");
		setExternalAuthEmailVerificationEmail("");
		setExternalAuthEmailVerificationError("");
		setExternalAuthEmailVerificationSent(false);
	};

	const switchAuthMode = (
		nextMode: Extract<AuthMode, "login" | "register">,
	) => {
		setErrors({});
		setMode(nextMode);
	};

	const handleResendActivation = async () => {
		if (!pendingActivation) return;

		try {
			setResendingActivation(true);
			await authService.resendRegisterActivation(pendingActivation.identifier);
			toast.success(t("activation_resent"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setResendingActivation(false);
		}
	};

	const handlePasswordResetRequest = async () => {
		const email = passwordResetEmail.trim();
		const result = emailSchema.safeParse(email);
		if (!result.success) {
			setPasswordResetError(result.error.issues[0]?.message ?? "");
			return;
		}

		try {
			setRequestingPasswordReset(true);
			await authService.requestPasswordReset({ email });
			toast.success(t("password_reset_request_sent"));
			setIdentifier(email);
			setPasswordResetError("");
			setShowPasswordResetRequest(false);
		} catch (error) {
			handleApiError(error);
		} finally {
			setRequestingPasswordReset(false);
		}
	};

	const handleExternalAuthEmailVerificationRequest = async () => {
		if (!externalAuthRecoveryFlow) return;
		const email = externalAuthEmailVerificationEmail.trim();
		const result = emailSchema.safeParse(email);
		if (!result.success) {
			setExternalAuthEmailVerificationError(
				result.error.issues[0]?.message ?? "",
			);
			return;
		}

		try {
			setExternalAuthEmailVerificationSubmitting(true);
			await authService.startExternalAuthEmailVerification({
				flow_token: externalAuthRecoveryFlow,
				email,
			});
			setExternalAuthEmailVerificationError("");
			setExternalAuthEmailVerificationSent(true);
			toast.success(t("external_auth_email_verification_sent_toast"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setExternalAuthEmailVerificationSubmitting(false);
		}
	};

	const handleExternalAuthPasswordLink = async () => {
		if (!externalAuthRecoveryFlow) return;
		const id = externalAuthPasswordLinkIdentifier.trim();
		const pw = externalAuthPasswordLinkPassword;
		const errs: Record<string, string> = {};
		if (id.length === 0) {
			errs.identifier = t("email_or_username");
		}
		const pwResult = existingPasswordSchema.safeParse(pw);
		if (!pwResult.success) {
			errs.password = pwResult.error.issues[0]?.message ?? "";
		}
		setExternalAuthPasswordLinkIdentifierError(errs.identifier ?? "");
		setExternalAuthPasswordLinkPasswordError(errs.password ?? "");
		if (Object.keys(errs).length > 0) return;

		try {
			setExternalAuthPasswordLinkSubmitting(true);
			const session = await authService.linkExternalAuthWithPassword({
				flow_token: externalAuthRecoveryFlow,
				identifier: id,
				password: pw,
			});
			syncSession(session.expiresIn);
			await refreshUser();
			toast.success(t("external_auth_password_link_success"));
			exitAndNavigate();
		} catch (error) {
			handleApiError(error);
		} finally {
			setExternalAuthPasswordLinkSubmitting(false);
		}
	};

	const finishPasskeyLogin = useCallback(
		async (flowId: string, credential: unknown) => {
			const session = await authService.finishPasskeyLogin(flowId, credential);
			syncSession(session.expiresIn);
			await refreshUser();
			toast.success(loginSuccessMessage);
			exitAndNavigate();
		},
		[exitAndNavigate, loginSuccessMessage, refreshUser, syncSession],
	);

	const handlePasskeyLogin = async () => {
		if (!passkeySupported || mode !== "login") {
			toast.error(t("passkey_unsupported"));
			return;
		}

		try {
			conditionalPasskeyAbortRef.current?.abort();
			conditionalPasskeyAbortRef.current = null;
			setPasskeySubmitting(true);
			const trimmedIdentifier = identifier.trim();
			const start = await authService.startPasskeyLogin(
				trimmedIdentifier.length > 0 ? { identifier: trimmedIdentifier } : {},
			);
			const credential = await getPasskeyCredential(start.public_key);
			await finishPasskeyLogin(start.flow_id, credential);
		} catch (error) {
			if (error instanceof WebAuthnUnsupportedError) {
				toast.error(t("passkey_unsupported"));
				return;
			}
			if (error instanceof WebAuthnCancelledError) {
				toast.error(t("passkey_cancelled"));
				return;
			}
			handleApiError(error);
		} finally {
			setPasskeySubmitting(false);
		}
	};

	const handleExternalAuthLogin = async (
		provider: ExternalAuthPublicProvider,
	) => {
		if (mode !== "login") return;

		try {
			conditionalPasskeyAbortRef.current?.abort();
			conditionalPasskeyAbortRef.current = null;
			setExternalAuthBusyProvider(provider.key);
			const start = await authService.startExternalAuthLogin(provider, {
				return_path: "/?external_auth=success",
			});
			window.location.assign(start.authorization_url);
		} catch (error) {
			handleApiError(error);
			setExternalAuthBusyProvider(null);
		}
	};

	useEffect(() => {
		if (
			mode !== "login" ||
			checking ||
			showPasswordResetRequest ||
			externalAuthRecoveryFlow ||
			pendingActivation ||
			!conditionalPasskeySupportedRef.current
		) {
			return;
		}

		const controller = new AbortController();
		let completed = false;
		conditionalPasskeyAbortRef.current = controller;

		void (async () => {
			try {
				const start = await authService.startPasskeyLogin({
					conditional: true,
				});
				if (controller.signal.aborted) return;
				const credential = await getPasskeyCredential(
					start.public_key,
					"conditional",
					controller.signal,
				);
				if (controller.signal.aborted) return;
				completed = true;
				await finishPasskeyLogin(start.flow_id, credential);
			} catch (error) {
				if (controller.signal.aborted) return;
				if (
					error instanceof WebAuthnUnsupportedError ||
					error instanceof WebAuthnCancelledError
				) {
					return;
				}
				handleApiError(error);
			} finally {
				if (conditionalPasskeyAbortRef.current === controller) {
					conditionalPasskeyAbortRef.current = null;
				}
			}
		})();

		return () => {
			if (conditionalPasskeyAbortRef.current === controller) {
				conditionalPasskeyAbortRef.current = null;
			}
			if (!completed) {
				controller.abort();
			}
		};
	}, [
		checking,
		finishPasskeyLogin,
		mode,
		externalAuthRecoveryFlow,
		pendingActivation,
		showPasswordResetRequest,
	]);

	// ── Submit ──

	const handleSubmit = async (e: React.FormEvent) => {
		e.preventDefault();
		if (showPasswordResetRequest) {
			await handlePasswordResetRequest();
			return;
		}
		if (externalAuthRecoveryFlow) {
			if (externalAuthRecoveryMode === "email") {
				await handleExternalAuthEmailVerificationRequest();
			} else {
				await handleExternalAuthPasswordLink();
			}
			return;
		}
		if (!validate()) return;
		if (mode === "idle") return;

		setSubmitting(true);
		try {
			const id = identifier.trim();
			const extra = extraField.trim();

			if (mode === "login") {
				await login(id, password);
				toast.success(loginSuccessMessage);
				exitAndNavigate();
				return;
			}

			const un = isEmail ? extra : id;
			const em = isEmail ? id : extra;

			if (mode === "setup") {
				await authService.setup(un, em, password);
				toast.success(t("setup_complete"));
				await login(em, password);
				exitAndNavigate();
				return;
			}

			const registeredUser = await authService.register(un, em, password);
			setPassword("");
			setShowPassword(false);
			setErrors({});
			if (registeredUser.email_verified) {
				toast.success(t("register_success_direct"));
				setPendingActivation(null);
				setMode("login");
				setIdentifier(em);
				setExtraField("");
			} else {
				toast.success(t("register_success"));
				setPendingActivation({
					email: em,
					identifier: em,
					username: un,
				});
			}
		} catch (error) {
			if (
				error instanceof ApiError &&
				error.code === ErrorCode.PendingActivation
			) {
				setPendingActivation({
					email: isEmail ? identifier.trim() : undefined,
					identifier: identifier.trim(),
					username: isEmail ? undefined : identifier.trim(),
				});
				setPassword("");
				setShowPassword(false);
				setErrors({});
				return;
			}
			handleApiError(error);
		} finally {
			setSubmitting(false);
		}
	};

	// ── Labels ──

	const submitLabel = () => {
		if (submitting) {
			return mode === "login" ? t("signing_in") : t("creating_account");
		}
		if (mode === "setup") return t("create_admin");
		if (mode === "register") return t("sign_up");
		if (mode === "login") return t("sign_in");
		return t("continue");
	};

	const description = () => {
		if (pendingActivation) {
			return pendingActivation.email
				? t("activation_pending_desc_email", {
						email: pendingActivation.email,
					})
				: t("activation_pending_desc_identifier", {
						identifier: pendingActivation.identifier,
					});
		}
		if (externalAuthRecoveryFlow)
			return t("external_auth_account_recovery_desc");
		if (showPasswordResetRequest) return t("password_reset_request_desc");
		if (mode === "setup") return t("setup_desc");
		if (mode === "register") return t("create_new_account");
		if (mode === "login") return t("enter_password");
		return t("sign_in_to_account");
	};

	return (
		<div
			className={cn(
				"min-h-screen flex transition-all duration-300 ease-out",
				exiting && "opacity-0 scale-[1.02]",
			)}
		>
			<LoginBrandPanel />

			{/* Right — form */}
			<div className="flex-1 flex items-center justify-center bg-background p-6">
				<div className="w-full max-w-sm">
					{/* Mobile logo */}
					<div className="lg:hidden text-center mb-8">
						<AsterDriveWordmark
							alt="AsterDrive"
							className="mx-auto h-16 w-auto"
						/>
					</div>

					<LoginHeader
						title={
							pendingActivation
								? t("activation_pending_title")
								: externalAuthRecoveryFlow
									? t("external_auth_email_verification_title")
									: showPasswordResetRequest
										? t("forgot_password_title")
										: mode === "setup"
											? t("welcome_setup")
											: t("sign_in_to_account")
						}
						description={description()}
					/>

					<form onSubmit={handleSubmit}>
						<AnimateSwap
							activeKey={
								pendingActivation
									? "pending-activation"
									: externalAuthRecoveryFlow
										? "external-auth-recovery"
										: showPasswordResetRequest
											? "password-reset-request"
											: "auth-form"
							}
						>
							{pendingActivation ? (
								<PendingActivationPanel
									pendingActivation={pendingActivation}
									resendingActivation={resendingActivation}
									t={t}
									onResendActivation={() => void handleResendActivation()}
									onReset={resetPendingActivation}
								/>
							) : showPasswordResetRequest ? (
								<PasswordResetRequestPanel
									emailSchema={emailSchema}
									passwordResetEmail={passwordResetEmail}
									passwordResetError={passwordResetError}
									requestingPasswordReset={requestingPasswordReset}
									t={t}
									onBack={closePasswordResetRequest}
									onEmailChange={(value, error) => {
										setPasswordResetEmail(value);
										setPasswordResetError(error);
									}}
									onSubmit={() => void handlePasswordResetRequest()}
								/>
							) : externalAuthRecoveryFlow ? (
								<ExternalAuthRecoveryPanel
									email={externalAuthEmailVerificationEmail}
									emailError={externalAuthEmailVerificationError}
									emailSchema={emailSchema}
									identifier={externalAuthPasswordLinkIdentifier}
									identifierError={externalAuthPasswordLinkIdentifierError}
									mode={externalAuthRecoveryMode}
									password={externalAuthPasswordLinkPassword}
									passwordError={externalAuthPasswordLinkPasswordError}
									sent={externalAuthEmailVerificationSent}
									submittingEmail={externalAuthEmailVerificationSubmitting}
									submittingPassword={externalAuthPasswordLinkSubmitting}
									t={t}
									onBack={closeExternalAuthRecovery}
									onEmailChange={(value, error) => {
										setExternalAuthEmailVerificationEmail(value);
										setExternalAuthEmailVerificationError(error);
									}}
									onIdentifierChange={(value) => {
										setExternalAuthPasswordLinkIdentifier(value);
										if (value.trim().length > 0) {
											setExternalAuthPasswordLinkIdentifierError("");
										}
									}}
									onModeChange={setExternalAuthRecoveryMode}
									onPasswordChange={(value) => {
										setExternalAuthPasswordLinkPassword(value);
										if (externalAuthPasswordLinkPasswordError) {
											const result = existingPasswordSchema.safeParse(value);
											setExternalAuthPasswordLinkPasswordError(
												result.success
													? ""
													: (result.error.issues[0]?.message ?? ""),
											);
										}
									}}
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
									submitLabel={submitLabel()}
									submitting={submitting}
									onExtraFieldChange={(value) => {
										setExtraField(value);
										const schema = isEmail ? usernameSchema : emailSchema;
										validateSingle("extra", value, schema);
									}}
									onForgotPassword={() => {
										setShowPasswordResetRequest(true);
										setPasswordResetEmail(passwordResetPrefill);
										setPasswordResetError("");
									}}
									onIdentifierChange={(value) => {
										setIdentifier(value);
										if (value.length > 0 && !value.includes("@")) {
											validateSingle("identifier", value, usernameSchema);
										} else if (value.includes("@") && value.length > 3) {
											validateSingle("identifier", value, emailSchema);
										} else {
											setErrors((prev) => {
												const next = { ...prev };
												delete next.identifier;
												return next;
											});
										}
									}}
									onPasswordChange={(value) => {
										setPassword(value);
										if (mode !== "login" || errors.password) {
											validateSingle(
												"password",
												value,
												mode === "login"
													? existingPasswordSchema
													: passwordSchema,
											);
										}
									}}
									onPasskeyLogin={() => void handlePasskeyLogin()}
									onExternalAuthLogin={(provider) =>
										void handleExternalAuthLogin(provider)
									}
									onShowPasswordChange={setShowPassword}
									onSwitchAuthMode={switchAuthMode}
								/>
							)}
						</AnimateSwap>
					</form>
				</div>
			</div>
		</div>
	);
}
