import {
	type FormEvent,
	useCallback,
	useEffect,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { useLocation, useNavigate } from "react-router-dom";
import { toast } from "sonner";
import type { z } from "zod/v4";
import { handleApiError } from "@/hooks/useApiError";
import { usePageTitle } from "@/hooks/usePageTitle";
import {
	clearContactVerificationRedirectSearch,
	getContactVerificationRedirectState,
} from "@/lib/contactVerificationRedirect";
import { runWhenIdle } from "@/lib/idleTask";
import { logger } from "@/lib/logger";
import {
	clearPasswordResetRedirectSearch,
	getPasswordResetRedirectState,
} from "@/lib/passwordResetRedirect";
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
import {
	authService,
	type LoginResult,
	type MfaMethod,
} from "@/services/authService";
import { ApiError } from "@/services/http";
import { useAuthStore } from "@/stores/authStore";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import { useSystemSetupStore } from "@/stores/systemSetupStore";
import type { ExternalAuthPublicProvider, SystemSetupState } from "@/types/api";
import { ApiErrorCode } from "@/types/api-helpers";
import { createAuthCommandCoordinator } from "./login/authCommandCoordinator";
import {
	type AuthPolicySnapshot,
	createAuthRequestCoordinator,
	loadAuthBootstrap,
} from "./login/authPolicyCoordinator";
import {
	clearAuthFlowRedirectSearch,
	parseRecoverableAuthUrlFlow,
} from "./login/authUrlAdapter";
import { LoginPageView } from "./login/LoginPageView";
import {
	type authUiFlowReducer,
	initialAuthUiFlow,
} from "./login/loginPageState";
import type { AuthMode } from "./login/types";

function scheduleLoginSuccessPathWarmup() {
	return runWhenIdle(
		() => {
			void import("@/lib/pwaWarmup")
				.then(({ warmupLoginSuccessPath }) => {
					warmupLoginSuccessPath();
				})
				.catch(() => undefined);
		},
		{ fallbackDelayMs: 900, timeoutMs: 2_000 },
	);
}

function resolveMfaMethod(code: string, methods: MfaMethod[]): MfaMethod {
	const isTotp = /^\d{6}$/.test(normalizeTotpCode(code));
	if (isTotp && methods.includes("totp")) {
		return "totp";
	}
	if (/^\d{8}$/.test(code.trim()) && methods.includes("email_code")) {
		return "email_code";
	}
	if (methods.includes("recovery_code")) {
		return "recovery_code";
	}
	return methods[0] ?? "totp";
}

function normalizeTotpCode(code: string) {
	return code.trim().replace(/\s/g, "");
}

const LOGIN_UNAVAILABLE_ERROR_CODES = new Set<string>([
	ApiErrorCode.CredentialsFailed,
	ApiErrorCode.PendingActivation,
	ApiErrorCode.AuthAccountDisabled,
]);

function isLoginUnavailableError(error: unknown) {
	return (
		error instanceof ApiError && LOGIN_UNAVAILABLE_ERROR_CODES.has(error.code)
	);
}

function useLoginPageController() {
	const { t } = useTranslation(["login", "core"]);
	const { hash, pathname, search } = useLocation();
	const navigate = useNavigate();
	const refreshUser = useAuthStore((s) => s.refreshUser);
	const syncSession = useAuthStore((s) => s.syncSession);
	const invalidateSystemSetupState = useSystemSetupStore(
		(state) => state.invalidate,
	);
	const setSystemSetupState = useSystemSetupStore(
		(state) => state.setSetupState,
	);
	const publicPasskeyLoginEnabled = useFrontendConfigStore(
		(s) => s.passkeyLoginEnabled,
	);
	const publicPasswordLoginEnabled = useFrontendConfigStore(
		(s) => s.passwordLoginEnabled ?? true,
	);
	const conditionalPasskeyAbortRef = useRef<AbortController | null>(null);
	const conditionalPasskeySupportedRef = useRef(false);
	const authRequestCoordinatorRef = useRef(createAuthRequestCoordinator());
	const authCommandCoordinatorRef = useRef(
		createAuthCommandCoordinator(initialAuthUiFlow),
	);

	// The first field is always visible — it doubles as username or email
	const [identifier, setIdentifier] = useState("");
	// The extra field only shows for register/setup — it's whatever identifier is NOT
	const [extraField, setExtraField] = useState("");
	const [password, setPassword] = useState("");
	const [showPassword, setShowPassword] = useState(false);

	const [checking, setChecking] = useState(true);
	const [submitting, setSubmitting] = useState(false);
	const [resendingActivation, setResendingActivation] = useState(false);
	const [passkeySubmitting, setPasskeySubmitting] = useState(false);
	const [authPolicy, setAuthPolicy] = useState<AuthPolicySnapshot | null>(null);
	const [externalAuthBusyProvider, setExternalAuthBusyProvider] = useState<
		string | null
	>(null);
	const [passkeySupported] = useState(() => isWebAuthnSupported());
	const [registrationClosed, setRegistrationClosed] = useState(false);
	const [setupState, setSetupState] = useState<SystemSetupState | null>(null);
	const [exiting, setExiting] = useState(false);
	const [errors, setErrors] = useState<Record<string, string>>({});
	const [authFlow, setAuthFlow] = useState(initialAuthUiFlow);
	const dispatchAuthFlow = useCallback(
		(event: Parameters<typeof authUiFlowReducer>[1], serial?: number) => {
			setAuthFlow(authCommandCoordinatorRef.current.dispatch(event, serial));
		},
		[],
	);
	const mode: AuthMode =
		authFlow.kind === "bootstrapping"
			? "idle"
			: authFlow.kind === "setup" ||
					authFlow.kind === "register" ||
					authFlow.kind === "login"
				? authFlow.kind
				: "login";
	const pendingActivation =
		authFlow.kind === "pending-activation" ? authFlow.pendingActivation : null;
	const activationResendPanel =
		authFlow.kind === "activation-resend" ? authFlow.activationResend : null;
	const passwordResetPanel =
		authFlow.kind === "password-reset" ? authFlow.passwordReset : null;
	const externalAuthRecovery =
		authFlow.kind === "external-auth-recovery" ? authFlow.recovery : null;
	const mfaPanel = authFlow.kind === "mfa" ? authFlow : null;
	const mfaChallenge = mfaPanel?.challenge ?? null;
	const showPasswordResetRequest = passwordResetPanel !== null;
	const externalAuthRecoveryFlow = externalAuthRecovery?.flowToken ?? null;

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
	const activationResendPrefill = passwordResetPrefill;
	const loginSuccessMessage = t("login_success");
	const modeActionText = pendingActivation
		? t("activation_pending_title")
		: activationResendPanel
			? t("activation_resend_title")
			: externalAuthRecoveryFlow
				? t("external_auth_email_verification_title")
				: mfaChallenge
					? t("mfa_required_title")
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
	useEffect(() => scheduleLoginSuccessPathWarmup(), []);
	const passkeyLoginEnabled =
		authPolicy?.passkeyLoginEnabled ?? publicPasskeyLoginEnabled;
	const passwordLoginEnabled =
		authPolicy?.passwordLoginEnabled ?? publicPasswordLoginEnabled;
	const externalAuthProviders = authPolicy?.externalProviders ?? [];
	const externalAuthLoading = authPolicy === null;
	const externalAuthRecoveryMode =
		passwordLoginEnabled && externalAuthRecovery?.mode === "password"
			? "password"
			: "email";
	const canUsePasskeyLogin = passkeyLoginEnabled && passkeySupported;
	const isSubmitDisabled =
		submitting ||
		passkeySubmitting ||
		externalAuthBusyProvider !== null ||
		checking ||
		identifier.trim().length === 0 ||
		((mode !== "login" || passwordLoginEnabled) && password.length === 0) ||
		(requiresExtraField && extraField.trim().length === 0);

	useEffect(() => {
		const searchParams = new URLSearchParams(search);
		const mfaStatus = searchParams.get("mfa");
		const externalAuthStatus = searchParams.get("external_auth");
		const externalAuthMessage = searchParams.get("message");
		const recoverableFlow = parseRecoverableAuthUrlFlow(search);
		const invitationStatus = searchParams.get("invitation");
		const verification = getContactVerificationRedirectState(search);
		const passwordReset = getPasswordResetRedirectState(search);
		if (
			!verification &&
			!passwordReset &&
			!externalAuthStatus &&
			!mfaStatus &&
			!invitationStatus
		) {
			return;
		}

		if (verification) {
			switch (verification.status) {
				case "email-changed":
					if (!verification.email) {
						return;
					}
					toast.success(
						t("verify_contact_email_changed_login_hint", {
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

		if (invitationStatus === "accepted") {
			toast.success(t("invitation_accepted_login"), {
				id: "invitation-accepted-login",
			});
		}

		if (recoverableFlow?.kind === "mfa") {
			dispatchAuthFlow({
				type: "open_mfa",
				challenge: {
					expiresAt: recoverableFlow.expiresAt,
					flowToken: recoverableFlow.flowToken,
					methods: recoverableFlow.methods,
					returnPath: recoverableFlow.returnPath,
					successMessage: loginSuccessMessage,
				},
			});
		} else if (recoverableFlow?.kind === "external-auth-recovery") {
			dispatchAuthFlow({
				type: "open_external_auth_recovery",
				recovery: {
					email: passwordResetPrefill,
					emailError: "",
					emailSubmitting: false,
					flowToken: recoverableFlow.flowToken,
					mode: "password",
					password: "",
					passwordError: "",
					passwordIdentifier: identifier.trim(),
					passwordIdentifierError: "",
					passwordSubmitting: false,
					returnPath: recoverableFlow.returnPath,
					sent: false,
				},
			});
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

		navigate(
			{
				hash,
				pathname,
				search: clearPasswordResetRedirectSearch(
					clearContactVerificationRedirectSearch(
						clearAuthFlowRedirectSearch(search),
					),
				),
			},
			{ replace: true },
		);
	}, [
		hash,
		pathname,
		search,
		navigate,
		identifier,
		loginSuccessMessage,
		passwordResetPrefill,
		t,
		dispatchAuthFlow,
	]);

	useEffect(() => {
		if (!mfaChallenge) return;
		dispatchAuthFlow({ type: "set_mfa_now", now: Date.now() });
		const timer = window.setInterval(
			() => dispatchAuthFlow({ type: "set_mfa_now", now: Date.now() }),
			1000,
		);
		return () => window.clearInterval(timer);
	}, [mfaChallenge, dispatchAuthFlow]);

	useEffect(() => {
		const coordinator = authRequestCoordinatorRef.current;
		const revision = coordinator.begin();

		void loadAuthBootstrap(authService, {
			passkeyLoginEnabled: publicPasskeyLoginEnabled,
			passwordLoginEnabled: publicPasswordLoginEnabled,
		})
			.then((result) => {
				if (!coordinator.isCurrent(revision)) return;
				setAuthPolicy(result.policy);
				if (result.providersError) {
					logger.warn(
						"failed to load external auth providers",
						result.providersError,
					);
				}
				if (result.checkError) {
					logger.warn("failed to load auth check", result.checkError);
				}

				if (result.check) {
					setSystemSetupState(result.check.setup_state);
					setSetupState(result.check.setup_state);
					if (
						result.check.setup_state === "needs_admin" ||
						!result.check.has_users
					) {
						setRegistrationClosed(false);
						dispatchAuthFlow({ type: "bootstrap_resolved", flow: "setup" });
					} else {
						setRegistrationClosed(
							result.check.setup_state === "needs_storage" ||
								!result.policy.allowUserRegistration ||
								!result.policy.passwordLoginEnabled,
						);
						dispatchAuthFlow({ type: "bootstrap_resolved", flow: "login" });
					}
				} else {
					setRegistrationClosed(false);
					dispatchAuthFlow({ type: "bootstrap_resolved", flow: "login" });
				}
			})
			.catch((error) => {
				if (!coordinator.isCurrent(revision)) return;
				logger.warn("failed to load auth bootstrap", error);
			})
			.finally(() => {
				if (coordinator.isCurrent(revision)) {
					setChecking(false);
				}
			});

		return () => {
			coordinator.invalidate();
		};
	}, [
		publicPasskeyLoginEnabled,
		publicPasswordLoginEnabled,
		setSystemSetupState,
		dispatchAuthFlow,
	]);

	useEffect(() => {
		let cancelled = false;

		if (!passkeyLoginEnabled) {
			conditionalPasskeySupportedRef.current = false;
			return;
		}

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
	}, [passkeyLoginEnabled]);

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

		if (mode !== "login" || passwordLoginEnabled) {
			const passwordValidationSchema =
				mode === "login" ? existingPasswordSchema : passwordSchema;
			const pwResult = passwordValidationSchema.safeParse(password);
			if (!pwResult.success)
				errs.password = pwResult.error.issues[0]?.message ?? "";
		}

		setErrors(errs);
		return Object.keys(errs).length === 0;
	};

	// ── Exit animation → navigate ──

	const exitAndNavigateTo = useCallback(
		(target = "/") => {
			setExiting(true);
			setTimeout(() => navigate(target, { replace: true }), 350);
		},
		[navigate],
	);

	const finishAuthenticatedLogin = useCallback(
		async (
			session: { expiresIn: number },
			returnPath: string,
			successMessage: string,
		) => {
			syncSession(session.expiresIn);
			await refreshUser();
			toast.success(successMessage);
			exitAndNavigateTo(
				setupState === "needs_storage" ? "/setup/storage" : returnPath || "/",
			);
		},
		[exitAndNavigateTo, refreshUser, setupState, syncSession],
	);

	const finishPasswordChangeRequiredLogin = useCallback(
		async (session: { expiresIn: number }) => {
			syncSession(session.expiresIn);
			await refreshUser();
			exitAndNavigateTo("/force-password-change");
		},
		[exitAndNavigateTo, refreshUser, syncSession],
	);

	const handleLoginResult = useCallback(
		async (
			result: LoginResult,
			returnPath: string,
			successMessage: string,
			commandSerial?: number,
		) => {
			if (result.status === "authenticated") {
				await finishAuthenticatedLogin(result, returnPath, successMessage);
				return;
			}
			if (result.status === "password_change_required") {
				await finishPasswordChangeRequiredLogin(result);
				return;
			}
			const methods: MfaMethod[] =
				result.methods.length > 0 ? result.methods : ["totp"];
			dispatchAuthFlow(
				{
					type: "open_mfa",
					challenge: {
						expiresAt: Date.now() + result.expiresIn * 1000,
						flowToken: result.flowToken,
						methods,
						returnPath,
						successMessage,
					},
				},
				commandSerial,
			);
			setPassword("");
			setShowPassword(false);
		},
		[
			finishAuthenticatedLogin,
			finishPasswordChangeRequiredLogin,
			dispatchAuthFlow,
		],
	);

	const resetPendingActivation = () => {
		dispatchAuthFlow({ type: "open_auth" });
		setErrors({});
		setPassword("");
		setShowPassword(false);
	};

	const closePasswordResetRequest = () => {
		dispatchAuthFlow({ type: "close_password_reset" });
	};

	const closeActivationResendRequest = () => {
		dispatchAuthFlow({ type: "close_activation_resend" });
	};

	const closeExternalAuthRecovery = () => {
		dispatchAuthFlow({ type: "close_external_auth_recovery" });
	};

	const closeMfaChallenge = () => {
		dispatchAuthFlow({ type: "close_mfa" });
	};

	const switchAuthMode = (
		nextMode: Extract<AuthMode, "login" | "register">,
	) => {
		setErrors({});
		dispatchAuthFlow({ type: "switch_auth_mode", mode: nextMode });
	};

	const handleResendActivation = async () => {
		if (!pendingActivation || !passwordLoginEnabled) return;

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

	const handleActivationResendRequest = async () => {
		if (!activationResendPanel || !passwordLoginEnabled) return;
		const commandSerial = authCommandCoordinatorRef.current.begin();
		const email = activationResendPanel.email.trim();
		const result = emailSchema.safeParse(email);
		if (!result.success) {
			dispatchAuthFlow(
				{
					type: "set_activation_resend_error",
					error: result.error.issues[0]?.message ?? "",
				},
				commandSerial,
			);
			return;
		}

		try {
			dispatchAuthFlow(
				{
					type: "set_activation_resend_requesting",
					requesting: true,
				},
				commandSerial,
			);
			await authService.resendRegisterActivation(email);
			toast.success(t("activation_resend_request_sent"));
			setIdentifier(email);
			dispatchAuthFlow({ type: "close_activation_resend" }, commandSerial);
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchAuthFlow(
				{
					type: "set_activation_resend_requesting",
					requesting: false,
				},
				commandSerial,
			);
		}
	};

	const handlePasswordResetRequest = async () => {
		if (!passwordResetPanel || !passwordLoginEnabled) return;
		const commandSerial = authCommandCoordinatorRef.current.begin();
		const email = passwordResetPanel.email.trim();
		const result = emailSchema.safeParse(email);
		if (!result.success) {
			dispatchAuthFlow(
				{
					type: "set_password_reset_error",
					error: result.error.issues[0]?.message ?? "",
				},
				commandSerial,
			);
			return;
		}

		try {
			dispatchAuthFlow(
				{
					type: "set_password_reset_requesting",
					requesting: true,
				},
				commandSerial,
			);
			await authService.requestPasswordReset({ email });
			toast.success(t("password_reset_request_sent"));
			setIdentifier(email);
			dispatchAuthFlow({ type: "close_password_reset" }, commandSerial);
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchAuthFlow(
				{
					type: "set_password_reset_requesting",
					requesting: false,
				},
				commandSerial,
			);
		}
	};

	const handleExternalAuthEmailVerificationRequest = async () => {
		if (!externalAuthRecovery) return;
		const commandSerial = authCommandCoordinatorRef.current.begin();
		const email = externalAuthRecovery.email.trim();
		const result = emailSchema.safeParse(email);
		if (!result.success) {
			dispatchAuthFlow(
				{
					type: "set_external_email_error",
					error: result.error.issues[0]?.message ?? "",
				},
				commandSerial,
			);
			return;
		}

		try {
			dispatchAuthFlow(
				{
					type: "set_external_email_submitting",
					submitting: true,
				},
				commandSerial,
			);
			await authService.startExternalAuthEmailVerification({
				flow_token: externalAuthRecovery.flowToken,
				email,
			});
			dispatchAuthFlow({ type: "external_email_sent" }, commandSerial);
			toast.success(t("external_auth_email_verification_sent_toast"));
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchAuthFlow(
				{
					type: "set_external_email_submitting",
					submitting: false,
				},
				commandSerial,
			);
		}
	};

	const handleExternalAuthPasswordLink = async () => {
		if (!externalAuthRecovery || !passwordLoginEnabled) return;
		const commandSerial = authCommandCoordinatorRef.current.begin();
		const id = externalAuthRecovery.passwordIdentifier.trim();
		const pw = externalAuthRecovery.password;
		const errs: Record<string, string> = {};
		if (id.length === 0) {
			errs.identifier = t("email_or_username");
		}
		const pwResult = existingPasswordSchema.safeParse(pw);
		if (!pwResult.success) {
			errs.password = pwResult.error.issues[0]?.message ?? "";
		}
		dispatchAuthFlow(
			{
				type: "set_external_password_errors",
				identifier: errs.identifier ?? "",
				password: errs.password ?? "",
			},
			commandSerial,
		);
		if (Object.keys(errs).length > 0) return;

		try {
			dispatchAuthFlow(
				{
					type: "set_external_password_submitting",
					submitting: true,
				},
				commandSerial,
			);
			const result = await authService.linkExternalAuthWithPassword({
				flow_token: externalAuthRecovery.flowToken,
				identifier: id,
				password: pw,
			});
			await handleLoginResult(
				result,
				externalAuthRecovery.returnPath,
				t("external_auth_password_link_success"),
				commandSerial,
			);
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchAuthFlow(
				{
					type: "set_external_password_submitting",
					submitting: false,
				},
				commandSerial,
			);
		}
	};

	const finishPasskeyLogin = useCallback(
		async (flowId: string, credential: unknown) => {
			await handleLoginResult(
				await authService.finishPasskeyLogin(flowId, credential),
				"/",
				loginSuccessMessage,
			);
		},
		[handleLoginResult, loginSuccessMessage],
	);

	const handlePasskeyLogin = async () => {
		if (!canUsePasskeyLogin || mode !== "login") {
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
				return_path:
					setupState === "needs_storage"
						? "/setup/storage?external_auth=success"
						: "/?external_auth=success",
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
			mfaChallenge ||
			showPasswordResetRequest ||
			externalAuthRecoveryFlow ||
			activationResendPanel ||
			pendingActivation ||
			!passkeyLoginEnabled ||
			!conditionalPasskeySupportedRef.current
		) {
			return;
		}

		const controller = new AbortController();
		let completed = false;
		conditionalPasskeyAbortRef.current = controller;

		void (async () => {
			try {
				if (controller.signal.aborted) return;
				const start = await authService.startPasskeyLogin({
					conditional: true,
				});
				if (!controller.signal.aborted) {
					const credential = await getPasskeyCredential(
						start.public_key,
						"conditional",
						controller.signal,
					);
					if (!controller.signal.aborted) {
						completed = true;
						await finishPasskeyLogin(start.flow_id, credential);
					}
				}
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
		activationResendPanel,
		mfaChallenge,
		passkeyLoginEnabled,
		pendingActivation,
		showPasswordResetRequest,
	]);

	// ── Submit ──

	const handleMfaSubmit = async () => {
		if (!mfaPanel) return;
		const commandSerial = authCommandCoordinatorRef.current.begin();
		const { challenge } = mfaPanel;
		if (challenge.expiresAt <= Date.now()) {
			dispatchAuthFlow(
				{
					type: "set_mfa_error",
					error: t("mfa_flow_expired"),
				},
				commandSerial,
			);
			return;
		}
		const code = mfaPanel.code.trim();
		if (mfaPanel.selectedMethod === "email_code" && !mfaPanel.emailCodeSent) {
			dispatchAuthFlow(
				{
					type: "set_mfa_error",
					error: t("mfa_email_code_required_send"),
				},
				commandSerial,
			);
			return;
		}
		if (!code) {
			dispatchAuthFlow(
				{
					type: "set_mfa_error",
					error: t("mfa_code_required"),
				},
				commandSerial,
			);
			return;
		}

		try {
			const method = challenge.methods.includes(mfaPanel.selectedMethod)
				? mfaPanel.selectedMethod
				: resolveMfaMethod(code, challenge.methods);
			const normalizedCode =
				method === "totp" ? normalizeTotpCode(code) : code.trim();
			dispatchAuthFlow(
				{ type: "set_mfa_submitting", submitting: true },
				commandSerial,
			);
			await handleLoginResult(
				await authService.verifyMfaChallenge({
					flow_token: challenge.flowToken,
					method,
					code: normalizedCode,
				}),
				challenge.returnPath,
				challenge.successMessage,
				commandSerial,
			);
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchAuthFlow(
				{ type: "set_mfa_submitting", submitting: false },
				commandSerial,
			);
		}
	};

	const handleMfaEmailCodeSend = async () => {
		if (!mfaPanel) return;
		if (mfaPanel.selectedMethod !== "email_code") return;
		const commandSerial = authCommandCoordinatorRef.current.begin();
		if (mfaPanel.challenge.expiresAt <= Date.now()) {
			dispatchAuthFlow(
				{
					type: "set_mfa_error",
					error: t("mfa_flow_expired"),
				},
				commandSerial,
			);
			return;
		}
		if (mfaPanel.emailCodeResendAt > Date.now()) return;

		try {
			dispatchAuthFlow(
				{ type: "set_mfa_email_code_sending", sending: true },
				commandSerial,
			);
			const result = await authService.sendMfaEmailCode({
				flow_token: mfaPanel.challenge.flowToken,
			});
			dispatchAuthFlow(
				{
					type: "set_mfa_email_code_sent",
					expiresIn: result.expires_in,
					now: Date.now(),
					resendAfter: result.resend_after,
				},
				commandSerial,
			);
			toast.success(t("mfa_email_code_sent"));
		} catch (error) {
			dispatchAuthFlow(
				{
					type: "set_mfa_email_code_sending",
					sending: false,
				},
				commandSerial,
			);
			handleApiError(error);
		}
	};

	const handleSubmit = async (e: FormEvent) => {
		e.preventDefault();
		if (mfaChallenge) {
			await handleMfaSubmit();
			return;
		}
		if (showPasswordResetRequest) {
			await handlePasswordResetRequest();
			return;
		}
		if (activationResendPanel) {
			await handleActivationResendRequest();
			return;
		}
		if (externalAuthRecoveryFlow) {
			if (externalAuthRecoveryMode === "email" || !passwordLoginEnabled) {
				await handleExternalAuthEmailVerificationRequest();
			} else {
				await handleExternalAuthPasswordLink();
			}
			return;
		}
		if (mode === "login" && !passwordLoginEnabled) return;
		if (!validate()) return;
		if (mode === "idle") return;

		setSubmitting(true);
		const commandSerial = authCommandCoordinatorRef.current.begin();
		try {
			const id = identifier.trim();
			const extra = extraField.trim();

			if (mode === "login") {
				await handleLoginResult(
					await authService.login(id, password),
					"/",
					loginSuccessMessage,
					commandSerial,
				);
				return;
			}

			const un = isEmail ? extra : id;
			const em = isEmail ? id : extra;

			if (mode === "setup") {
				await authService.setup(un, em, password);

				// The administrator now exists. Clear the stale needs_admin snapshot
				// before any recoverable login or setup-state request can fail.
				invalidateSystemSetupState();
				setSetupState(null);
				setRegistrationClosed(true);
				dispatchAuthFlow({ type: "open_auth" }, commandSerial);
				setIdentifier(em);
				setExtraField("");
				setErrors({});
				setShowPassword(false);
				toast.success(t("setup_admin_created"));
				if (!passwordLoginEnabled) {
					setPassword("");
					return;
				}

				await handleLoginResult(
					await authService.login(em, password),
					"/",
					loginSuccessMessage,
					commandSerial,
				);
				return;
			}

			const registeredUser = await authService.register(un, em, password);
			setPassword("");
			setShowPassword(false);
			setErrors({});
			if (registeredUser.email_verified) {
				toast.success(t("register_success_direct"));
				dispatchAuthFlow({ type: "open_auth" }, commandSerial);
				setIdentifier(em);
				setExtraField("");
			} else {
				toast.success(t("register_success"));
				dispatchAuthFlow(
					{
						type: "set_pending_activation",
						pendingActivation: {
							email: em,
							identifier: em,
							username: un,
						},
					},
					commandSerial,
				);
			}
		} catch (error) {
			if (mode === "login" && isLoginUnavailableError(error)) {
				toast.error(t("login_failed_unavailable"));
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
		return t("core:continue");
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
		if (activationResendPanel) return t("activation_resend_desc");
		if (externalAuthRecoveryFlow)
			return t("external_auth_account_recovery_desc");
		if (mfaChallenge) return t("mfa_required_desc");
		if (showPasswordResetRequest) return t("password_reset_request_desc");
		if (mode === "setup") return t("setup_desc");
		if (mode === "register") return t("create_new_account");
		if (mode === "login" && setupState === "needs_storage")
			return t("storage_setup_login_desc");
		if (mode === "login" && !passwordLoginEnabled)
			return t("choose_login_method");
		if (mode === "login") return t("enter_password");
		return t("sign_in_to_account");
	};

	return {
		activationResendPanel,
		checking,
		description: description(),
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
		mfaPanel,
		mode,
		modeActionText,
		passkeySubmitting,
		passkeyLoginEnabled,
		passwordLoginEnabled,
		passkeySupported,
		password,
		passwordResetPanel,
		pendingActivation,
		registrationClosed,
		resendingActivation,
		showPassword,
		submitLabel: submitLabel(),
		submitting,
		t,
		title: pendingActivation
			? t("activation_pending_title")
			: activationResendPanel
				? t("activation_resend_title")
				: externalAuthRecoveryFlow
					? t("external_auth_email_verification_title")
					: mfaChallenge
						? t("mfa_required_title")
						: showPasswordResetRequest
							? t("forgot_password_title")
							: mode === "setup"
								? t("welcome_setup")
								: setupState === "needs_storage"
									? t("storage_setup_login_title")
									: t("sign_in_to_account"),
		onActivationResendBack: closeActivationResendRequest,
		onActivationResendEmailChange: (value: string, error: string) => {
			dispatchAuthFlow({
				type: "set_activation_resend_email",
				email: value,
				error,
			});
		},
		onActivationResendSubmit: () => void handleActivationResendRequest(),
		onExternalAuthEmailChange: (value: string, error: string) => {
			dispatchAuthFlow({
				type: "set_external_email",
				email: value,
				error,
			});
		},
		onExternalAuthIdentifierChange: (value: string) => {
			dispatchAuthFlow({
				type: "set_external_password_identifier",
				identifier: value,
			});
		},
		onExternalAuthLogin: (provider: ExternalAuthPublicProvider) =>
			void handleExternalAuthLogin(provider),
		onExternalAuthModeChange: (nextMode: "password" | "email") =>
			dispatchAuthFlow({ type: "set_external_mode", mode: nextMode }),
		onExternalAuthPasswordChange: (value: string) => {
			let error: string | undefined;
			if (externalAuthRecovery?.passwordError) {
				const result = existingPasswordSchema.safeParse(value);
				error = result.success ? "" : (result.error.issues[0]?.message ?? "");
			}
			dispatchAuthFlow({
				type: "set_external_password",
				password: value,
				error,
			});
		},
		onExternalAuthRecoveryBack: closeExternalAuthRecovery,
		onExtraFieldChange: (value: string) => {
			setExtraField(value);
			const schema = isEmail ? usernameSchema : emailSchema;
			validateSingle("extra", value, schema);
		},
		onForgotPassword: () => {
			dispatchAuthFlow({
				type: "open_password_reset",
				email: passwordResetPrefill,
			});
		},
		onIdentifierChange: (value: string) => {
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
		},
		onMfaBack: closeMfaChallenge,
		onMfaCodeChange: (value: string) => {
			dispatchAuthFlow({
				type: "set_mfa_code",
				code: value,
			});
		},
		onMfaEmailCodeSend: () => void handleMfaEmailCodeSend(),
		onMfaMethodChange: (method: MfaMethod) => {
			dispatchAuthFlow({ type: "set_mfa_method", method });
		},
		onPasskeyLogin: () => void handlePasskeyLogin(),
		onPasswordChange: (value: string) => {
			setPassword(value);
			if (mode !== "login" || errors.password) {
				validateSingle(
					"password",
					value,
					mode === "login" ? existingPasswordSchema : passwordSchema,
				);
			}
		},
		onPasswordResetBack: closePasswordResetRequest,
		onPasswordResetEmailChange: (value: string, error: string) => {
			dispatchAuthFlow({
				type: "set_password_reset_email",
				email: value,
				error,
			});
		},
		onPasswordResetSubmit: () => void handlePasswordResetRequest(),
		onPendingActivationReset: resetPendingActivation,
		onResendActivation: () => void handleResendActivation(),
		onResendActivationRequest: () => {
			dispatchAuthFlow({
				type: "open_activation_resend",
				email: activationResendPrefill,
			});
		},
		onShowPasswordChange: setShowPassword,
		onSubmit: handleSubmit,
		onSwitchAuthMode: switchAuthMode,
	};
}

export default function LoginPage() {
	return <LoginPageView {...useLoginPageController()} />;
}
