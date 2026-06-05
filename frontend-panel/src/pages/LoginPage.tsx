import {
	type FormEvent,
	useCallback,
	useEffect,
	useReducer,
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
import type { ExternalAuthPublicProvider } from "@/types/api";
import { ErrorCode } from "@/types/api-helpers";
import { LoginPageView } from "./login/LoginPageView";
import {
	authPanelReducer,
	initialAuthPanelState,
} from "./login/loginPageState";
import type { AuthMode } from "./login/types";

const MFA_METHODS: MfaMethod[] = ["totp", "recovery_code", "email_code"];

function parseMfaMethods(value: string | null): MfaMethod[] {
	if (!value) return ["totp", "recovery_code"];
	const methods = value
		.split(",")
		.map((method) => method.trim())
		.filter((method): method is MfaMethod =>
			MFA_METHODS.includes(method as MfaMethod),
		);
	return methods.length > 0 ? methods : ["totp", "recovery_code"];
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

function resolveMfaRedirectExpiresAt(expiresIn: string | null) {
	const parsed = Number(expiresIn);
	const ttlSeconds = Number.isFinite(parsed) && parsed > 0 ? parsed : 300;
	return Date.now() + ttlSeconds * 1000;
}

function useLoginPageController() {
	const { t } = useTranslation(["auth", "core", "settings"]);
	const { hash, pathname, search } = useLocation();
	const navigate = useNavigate();
	const refreshUser = useAuthStore((s) => s.refreshUser);
	const syncSession = useAuthStore((s) => s.syncSession);
	const publicPasskeyLoginEnabled = useFrontendConfigStore(
		(s) => s.passkeyLoginEnabled,
	);
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
	const [passkeySubmitting, setPasskeySubmitting] = useState(false);
	const [externalAuthProviders, setExternalAuthProviders] = useState<
		ExternalAuthPublicProvider[]
	>([]);
	const [externalAuthLoading, setExternalAuthLoading] = useState(true);
	const [externalAuthBusyProvider, setExternalAuthBusyProvider] = useState<
		string | null
	>(null);
	const [passkeySupported] = useState(() => isWebAuthnSupported());
	const [checkedPasskeyLoginEnabled, setCheckedPasskeyLoginEnabled] = useState<
		boolean | null
	>(null);
	const [registrationClosed, setRegistrationClosed] = useState(false);
	const [exiting, setExiting] = useState(false);
	const [errors, setErrors] = useState<Record<string, string>>({});
	const [authPanel, dispatchAuthPanel] = useReducer(
		authPanelReducer,
		initialAuthPanelState,
	);
	const pendingActivation =
		authPanel.kind === "pending-activation"
			? authPanel.pendingActivation
			: null;
	const passwordResetPanel =
		authPanel.kind === "password-reset" ? authPanel.passwordReset : null;
	const externalAuthRecovery =
		authPanel.kind === "external-auth-recovery" ? authPanel.recovery : null;
	const mfaPanel = authPanel.kind === "mfa" ? authPanel : null;
	const mfaChallenge = mfaPanel?.challenge ?? null;
	const showPasswordResetRequest = passwordResetPanel !== null;
	const externalAuthRecoveryFlow = externalAuthRecovery?.flowToken ?? null;
	const externalAuthRecoveryMode = externalAuthRecovery?.mode ?? "password";

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
	const passkeyLoginEnabled =
		checkedPasskeyLoginEnabled ?? publicPasskeyLoginEnabled;
	const canUsePasskeyLogin = passkeyLoginEnabled && passkeySupported;
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
		const mfaStatus = searchParams.get("mfa");
		const mfaFlow = searchParams.get("flow");
		const mfaExpiresIn = searchParams.get("expires_in");
		const mfaMethods = searchParams.get("methods");
		const returnPath = searchParams.get("return_path") || "/";
		const externalAuthStatus = searchParams.get("external_auth");
		const externalAuthMessage = searchParams.get("message");
		const externalAuthFlow = searchParams.get("flow");
		const verification = getContactVerificationRedirectState(search);
		const passwordReset = getPasswordResetRedirectState(search);
		if (!verification && !passwordReset && !externalAuthStatus && !mfaStatus) {
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

		if (mfaStatus === "required" && mfaFlow) {
			dispatchAuthPanel({
				type: "open_mfa",
				challenge: {
					expiresAt: resolveMfaRedirectExpiresAt(mfaExpiresIn),
					flowToken: mfaFlow,
					methods: parseMfaMethods(mfaMethods),
					returnPath,
					successMessage: loginSuccessMessage,
				},
			});
		} else if (externalAuthStatus === "email_required" && externalAuthFlow) {
			dispatchAuthPanel({
				type: "open_external_auth_recovery",
				recovery: {
					email: passwordResetPrefill,
					emailError: "",
					emailSubmitting: false,
					flowToken: externalAuthFlow,
					mode: "password",
					password: "",
					passwordError: "",
					passwordIdentifier: identifier.trim(),
					passwordIdentifierError: "",
					passwordSubmitting: false,
					returnPath,
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

		searchParams.delete("external_auth");
		searchParams.delete("mfa");
		searchParams.delete("code");
		searchParams.delete("message");
		searchParams.delete("flow");
		searchParams.delete("expires_in");
		searchParams.delete("methods");
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
	}, [
		hash,
		pathname,
		search,
		navigate,
		identifier,
		loginSuccessMessage,
		passwordResetPrefill,
		t,
	]);

	useEffect(() => {
		if (!mfaChallenge) return;
		dispatchAuthPanel({ type: "set_mfa_now", now: Date.now() });
		const timer = window.setInterval(
			() => dispatchAuthPanel({ type: "set_mfa_now", now: Date.now() }),
			1000,
		);
		return () => window.clearInterval(timer);
	}, [mfaChallenge]);

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
				setCheckedPasskeyLoginEnabled(result.passkey_login_enabled !== false);
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
		let cancelled = false;

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

		const passwordValidationSchema =
			mode === "login" ? existingPasswordSchema : passwordSchema;
		const pwResult = passwordValidationSchema.safeParse(password);
		if (!pwResult.success)
			errs.password = pwResult.error.issues[0]?.message ?? "";

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
			exitAndNavigateTo(returnPath || "/");
		},
		[exitAndNavigateTo, refreshUser, syncSession],
	);

	const handleLoginResult = useCallback(
		async (result: LoginResult, returnPath: string, successMessage: string) => {
			if (result.status === "authenticated") {
				await finishAuthenticatedLogin(result, returnPath, successMessage);
				return;
			}
			const methods: MfaMethod[] =
				result.methods.length > 0 ? result.methods : ["totp"];
			dispatchAuthPanel({
				type: "open_mfa",
				challenge: {
					expiresAt: Date.now() + result.expiresIn * 1000,
					flowToken: result.flowToken,
					methods,
					returnPath,
					successMessage,
				},
			});
			setPassword("");
			setShowPassword(false);
		},
		[finishAuthenticatedLogin],
	);

	const resetPendingActivation = () => {
		dispatchAuthPanel({ type: "open_auth" });
		setErrors({});
		setPassword("");
		setShowPassword(false);
	};

	const closePasswordResetRequest = () => {
		dispatchAuthPanel({ type: "close_password_reset" });
	};

	const closeExternalAuthRecovery = () => {
		dispatchAuthPanel({ type: "close_external_auth_recovery" });
	};

	const closeMfaChallenge = () => {
		dispatchAuthPanel({ type: "close_mfa" });
		setMode("login");
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
		if (!passwordResetPanel) return;
		const email = passwordResetPanel.email.trim();
		const result = emailSchema.safeParse(email);
		if (!result.success) {
			dispatchAuthPanel({
				type: "set_password_reset_error",
				error: result.error.issues[0]?.message ?? "",
			});
			return;
		}

		try {
			dispatchAuthPanel({
				type: "set_password_reset_requesting",
				requesting: true,
			});
			await authService.requestPasswordReset({ email });
			toast.success(t("password_reset_request_sent"));
			setIdentifier(email);
			dispatchAuthPanel({ type: "close_password_reset" });
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchAuthPanel({
				type: "set_password_reset_requesting",
				requesting: false,
			});
		}
	};

	const handleExternalAuthEmailVerificationRequest = async () => {
		if (!externalAuthRecovery) return;
		const email = externalAuthRecovery.email.trim();
		const result = emailSchema.safeParse(email);
		if (!result.success) {
			dispatchAuthPanel({
				type: "set_external_email_error",
				error: result.error.issues[0]?.message ?? "",
			});
			return;
		}

		try {
			dispatchAuthPanel({
				type: "set_external_email_submitting",
				submitting: true,
			});
			await authService.startExternalAuthEmailVerification({
				flow_token: externalAuthRecovery.flowToken,
				email,
			});
			dispatchAuthPanel({ type: "external_email_sent" });
			toast.success(t("external_auth_email_verification_sent_toast"));
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchAuthPanel({
				type: "set_external_email_submitting",
				submitting: false,
			});
		}
	};

	const handleExternalAuthPasswordLink = async () => {
		if (!externalAuthRecovery) return;
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
		dispatchAuthPanel({
			type: "set_external_password_errors",
			identifier: errs.identifier ?? "",
			password: errs.password ?? "",
		});
		if (Object.keys(errs).length > 0) return;

		try {
			dispatchAuthPanel({
				type: "set_external_password_submitting",
				submitting: true,
			});
			const result = await authService.linkExternalAuthWithPassword({
				flow_token: externalAuthRecovery.flowToken,
				identifier: id,
				password: pw,
			});
			await handleLoginResult(
				result,
				externalAuthRecovery.returnPath,
				t("external_auth_password_link_success"),
			);
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchAuthPanel({
				type: "set_external_password_submitting",
				submitting: false,
			});
		}
	};

	const finishPasskeyLogin = useCallback(
		async (flowId: string, credential: unknown) => {
			const session = await authService.finishPasskeyLogin(flowId, credential);
			await finishAuthenticatedLogin(session, "/", loginSuccessMessage);
		},
		[finishAuthenticatedLogin, loginSuccessMessage],
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
			mfaChallenge ||
			showPasswordResetRequest ||
			externalAuthRecoveryFlow ||
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
		mfaChallenge,
		passkeyLoginEnabled,
		pendingActivation,
		showPasswordResetRequest,
	]);

	// ── Submit ──

	const handleMfaSubmit = async () => {
		if (!mfaPanel) return;
		const { challenge } = mfaPanel;
		if (challenge.expiresAt <= Date.now()) {
			dispatchAuthPanel({
				type: "set_mfa_error",
				error: t("mfa_flow_expired"),
			});
			return;
		}
		const code = mfaPanel.code.trim();
		if (mfaPanel.selectedMethod === "email_code" && !mfaPanel.emailCodeSent) {
			dispatchAuthPanel({
				type: "set_mfa_error",
				error: t("mfa_email_code_required_send"),
			});
			return;
		}
		if (!code) {
			dispatchAuthPanel({
				type: "set_mfa_error",
				error: t("mfa_code_required"),
			});
			return;
		}

		try {
			const method = challenge.methods.includes(mfaPanel.selectedMethod)
				? mfaPanel.selectedMethod
				: resolveMfaMethod(code, challenge.methods);
			const normalizedCode =
				method === "totp" ? normalizeTotpCode(code) : code.trim();
			dispatchAuthPanel({ type: "set_mfa_submitting", submitting: true });
			const session = await authService.verifyMfaChallenge({
				flow_token: challenge.flowToken,
				method,
				code: normalizedCode,
			});
			await finishAuthenticatedLogin(
				session,
				challenge.returnPath,
				challenge.successMessage,
			);
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchAuthPanel({ type: "set_mfa_submitting", submitting: false });
		}
	};

	const handleMfaEmailCodeSend = async () => {
		if (!mfaPanel) return;
		if (mfaPanel.selectedMethod !== "email_code") return;
		if (mfaPanel.challenge.expiresAt <= Date.now()) {
			dispatchAuthPanel({
				type: "set_mfa_error",
				error: t("mfa_flow_expired"),
			});
			return;
		}
		if (mfaPanel.emailCodeResendAt > Date.now()) return;

		try {
			dispatchAuthPanel({ type: "set_mfa_email_code_sending", sending: true });
			const result = await authService.sendMfaEmailCode({
				flow_token: mfaPanel.challenge.flowToken,
			});
			dispatchAuthPanel({
				type: "set_mfa_email_code_sent",
				expiresIn: result.expires_in,
				now: Date.now(),
				resendAfter: result.resend_after,
			});
			toast.success(t("mfa_email_code_sent"));
		} catch (error) {
			dispatchAuthPanel({
				type: "set_mfa_email_code_sending",
				sending: false,
			});
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
				await handleLoginResult(
					await authService.login(id, password),
					"/",
					loginSuccessMessage,
				);
				return;
			}

			const un = isEmail ? extra : id;
			const em = isEmail ? id : extra;

			if (mode === "setup") {
				await authService.setup(un, em, password);
				toast.success(t("setup_complete"));
				await handleLoginResult(
					await authService.login(em, password),
					"/",
					loginSuccessMessage,
				);
				return;
			}

			const registeredUser = await authService.register(un, em, password);
			setPassword("");
			setShowPassword(false);
			setErrors({});
			if (registeredUser.email_verified) {
				toast.success(t("register_success_direct"));
				dispatchAuthPanel({ type: "open_auth" });
				setMode("login");
				setIdentifier(em);
				setExtraField("");
			} else {
				toast.success(t("register_success"));
				dispatchAuthPanel({
					type: "set_pending_activation",
					pendingActivation: {
						email: em,
						identifier: em,
						username: un,
					},
				});
			}
		} catch (error) {
			if (
				error instanceof ApiError &&
				error.code === ErrorCode.PendingActivation
			) {
				dispatchAuthPanel({
					type: "set_pending_activation",
					pendingActivation: {
						email: isEmail ? identifier.trim() : undefined,
						identifier: identifier.trim(),
						username: isEmail ? undefined : identifier.trim(),
					},
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
		if (externalAuthRecoveryFlow)
			return t("external_auth_account_recovery_desc");
		if (mfaChallenge) return t("mfa_required_desc");
		if (showPasswordResetRequest) return t("password_reset_request_desc");
		if (mode === "setup") return t("setup_desc");
		if (mode === "register") return t("create_new_account");
		if (mode === "login") return t("enter_password");
		return t("sign_in_to_account");
	};

	return {
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
			: externalAuthRecoveryFlow
				? t("external_auth_email_verification_title")
				: mfaChallenge
					? t("mfa_required_title")
					: showPasswordResetRequest
						? t("forgot_password_title")
						: mode === "setup"
							? t("welcome_setup")
							: t("sign_in_to_account"),
		onExternalAuthEmailChange: (value: string, error: string) => {
			dispatchAuthPanel({
				type: "set_external_email",
				email: value,
				error,
			});
		},
		onExternalAuthIdentifierChange: (value: string) => {
			dispatchAuthPanel({
				type: "set_external_password_identifier",
				identifier: value,
			});
		},
		onExternalAuthLogin: (provider: ExternalAuthPublicProvider) =>
			void handleExternalAuthLogin(provider),
		onExternalAuthModeChange: (nextMode: "password" | "email") =>
			dispatchAuthPanel({ type: "set_external_mode", mode: nextMode }),
		onExternalAuthPasswordChange: (value: string) => {
			let error: string | undefined;
			if (externalAuthRecovery?.passwordError) {
				const result = existingPasswordSchema.safeParse(value);
				error = result.success ? "" : (result.error.issues[0]?.message ?? "");
			}
			dispatchAuthPanel({
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
			dispatchAuthPanel({
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
			dispatchAuthPanel({
				type: "set_mfa_code",
				code: value,
			});
		},
		onMfaEmailCodeSend: () => void handleMfaEmailCodeSend(),
		onMfaMethodChange: (method: MfaMethod) => {
			dispatchAuthPanel({ type: "set_mfa_method", method });
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
			dispatchAuthPanel({
				type: "set_password_reset_email",
				email: value,
				error,
			});
		},
		onPasswordResetSubmit: () => void handlePasswordResetRequest(),
		onPendingActivationReset: resetPendingActivation,
		onResendActivation: () => void handleResendActivation(),
		onShowPasswordChange: setShowPassword,
		onSubmit: handleSubmit,
		onSwitchAuthMode: switchAuthMode,
	};
}

export default function LoginPage() {
	return <LoginPageView {...useLoginPageController()} />;
}
