import {
	type FormEvent,
	useCallback,
	useEffect,
	useReducer,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { useLocation, useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { handleApiError } from "@/hooks/useApiError";
import {
	clearContactVerificationRedirectSearch,
	getContactVerificationRedirectState,
} from "@/lib/contactVerificationRedirect";
import {
	emailSchema,
	existingPasswordSchema,
	passwordSchema,
} from "@/lib/validation";
import { authService } from "@/services/authService";
import { forceLogout, useAuthStore } from "@/stores/authStore";
import type { AuthSessionInfo } from "@/types/api";
import type { SecurityPane } from "./securityPanes";
import type { SecurityFormErrors } from "./types";

type PasswordField = "confirmPassword" | "currentPassword" | "newPassword";

interface SecurityAccountState {
	confirmPassword: string;
	currentPassword: string;
	emailBusy: boolean;
	errors: SecurityFormErrors;
	newEmail: string;
	newPassword: string;
	passwordBusy: boolean;
	resendingEmailChange: boolean;
}

type SecurityAccountAction =
	| { type: "request_email_change_success" }
	| { type: "reset_password_form" }
	| { type: "set_email_busy"; busy: boolean }
	| { type: "set_error"; field: keyof SecurityFormErrors; message?: string }
	| { type: "set_errors"; errors: SecurityFormErrors }
	| { type: "set_new_email"; value: string }
	| { type: "set_password_busy"; busy: boolean }
	| { type: "set_password_field"; field: PasswordField; value: string }
	| { type: "set_resending_email_change"; busy: boolean };

const initialSecurityAccountState: SecurityAccountState = {
	confirmPassword: "",
	currentPassword: "",
	emailBusy: false,
	errors: {},
	newEmail: "",
	newPassword: "",
	passwordBusy: false,
	resendingEmailChange: false,
};

function securityAccountReducer(
	state: SecurityAccountState,
	action: SecurityAccountAction,
): SecurityAccountState {
	switch (action.type) {
		case "request_email_change_success":
			return {
				...state,
				newEmail: "",
				errors: { ...state.errors, email: undefined },
			};
		case "reset_password_form":
			return {
				...state,
				confirmPassword: "",
				currentPassword: "",
				errors: {},
				newPassword: "",
			};
		case "set_email_busy":
			return { ...state, emailBusy: action.busy };
		case "set_error":
			return {
				...state,
				errors: { ...state.errors, [action.field]: action.message },
			};
		case "set_errors":
			return { ...state, errors: action.errors };
		case "set_new_email":
			return {
				...state,
				newEmail: action.value,
				errors: { ...state.errors, email: undefined },
			};
		case "set_password_busy":
			return { ...state, passwordBusy: action.busy };
		case "set_password_field":
			return {
				...state,
				[action.field]: action.value,
				errors: { ...state.errors, [action.field]: undefined },
			};
		case "set_resending_email_change":
			return { ...state, resendingEmailChange: action.busy };
	}
}

interface SecuritySessionsState {
	revokeBusyId: string | null;
	revokeOthersBusy: boolean;
	sessions: AuthSessionInfo[];
	sessionsLoading: boolean;
}

type SecuritySessionsAction =
	| { type: "keep_current_sessions" }
	| { type: "remove_session"; sessionId: string }
	| { type: "set_loading"; loading: boolean }
	| { type: "set_revoke_busy_id"; sessionId: string | null }
	| { type: "set_revoke_others_busy"; busy: boolean }
	| { type: "set_sessions"; sessions: AuthSessionInfo[] };

const initialSecuritySessionsState: SecuritySessionsState = {
	revokeBusyId: null,
	revokeOthersBusy: false,
	sessions: [],
	sessionsLoading: false,
};

function securitySessionsReducer(
	state: SecuritySessionsState,
	action: SecuritySessionsAction,
): SecuritySessionsState {
	switch (action.type) {
		case "keep_current_sessions":
			return {
				...state,
				sessions: state.sessions.filter((session) => session.is_current),
			};
		case "remove_session":
			return {
				...state,
				sessions: state.sessions.filter(
					(session) => session.id !== action.sessionId,
				),
			};
		case "set_loading":
			return { ...state, sessionsLoading: action.loading };
		case "set_revoke_busy_id":
			return { ...state, revokeBusyId: action.sessionId };
		case "set_revoke_others_busy":
			return { ...state, revokeOthersBusy: action.busy };
		case "set_sessions":
			return { ...state, sessions: action.sessions };
	}
}

export function useSecuritySettingsController() {
	const { t } = useTranslation(["auth", "core", "settings"]);
	const { hash, pathname, search } = useLocation();
	const navigate = useNavigate();
	const user = useAuthStore((s) => s.user);
	const refreshUser = useAuthStore((s) => s.refreshUser);
	const syncSession = useAuthStore((s) => s.syncSession);
	const [account, dispatchAccount] = useReducer(
		securityAccountReducer,
		initialSecurityAccountState,
	);
	const [sessionState, dispatchSessions] = useReducer(
		securitySessionsReducer,
		initialSecuritySessionsState,
	);
	const [activePane, setActivePane] = useState<SecurityPane>("account");

	useEffect(() => {
		const verification = getContactVerificationRedirectState(search);
		if (!verification) {
			return;
		}

		switch (verification.status) {
			case "email-changed":
				if (!verification.email) {
					break;
				}
				toast.success(
					t("settings:settings_email_change_confirmed", {
						email: verification.email,
					}),
					{
						id: `contact-verification-email-changed-settings:${verification.email}`,
					},
				);
				break;
			case "expired":
				toast.error(t("auth:verify_contact_expired_title"), {
					description: t("auth:verify_contact_expired_desc"),
					id: "contact-verification-expired-settings",
				});
				break;
			case "invalid":
				toast.error(t("auth:verify_contact_invalid_title"), {
					description: t("auth:verify_contact_invalid_desc"),
					id: "contact-verification-invalid-settings",
				});
				break;
			case "missing":
				toast.error(t("auth:verify_contact_missing_token_title"), {
					description: t("auth:verify_contact_missing_token_desc"),
					id: "contact-verification-missing-settings",
				});
				break;
			case "register-activated":
				toast.success(t("auth:activation_confirmed"), {
					id: "contact-verification-register-activated-settings",
				});
				break;
		}

		navigate(
			{
				hash,
				pathname,
				search: clearContactVerificationRedirectSearch(search),
			},
			{ replace: true },
		);
	}, [hash, pathname, search, navigate, t]);

	const loadSessions = useCallback(async () => {
		try {
			dispatchSessions({ type: "set_loading", loading: true });
			dispatchSessions({
				type: "set_sessions",
				sessions: await authService.listSessions(),
			});
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchSessions({ type: "set_loading", loading: false });
		}
	}, []);

	useEffect(() => {
		void loadSessions();
	}, [loadSessions]);

	const canSubmitPassword =
		!account.passwordBusy &&
		account.currentPassword.length > 0 &&
		account.newPassword.length > 0 &&
		account.confirmPassword.length > 0;
	const canSubmitEmailChange =
		!account.emailBusy &&
		!!user?.email_verified &&
		account.newEmail.trim().length > 0;
	const hasOtherSessions = sessionState.sessions.some(
		(session) => !session.is_current,
	);

	const validateEmailChange = () => {
		const email = account.newEmail.trim();
		const emailResult = emailSchema.safeParse(email);
		if (!emailResult.success) {
			dispatchAccount({
				type: "set_error",
				field: "email",
				message: emailResult.error.issues[0]?.message ?? "",
			});
			return false;
		}

		if (email === user?.email) {
			dispatchAccount({
				type: "set_error",
				field: "email",
				message: t("settings:settings_email_change_same"),
			});
			return false;
		}

		dispatchAccount({ type: "set_error", field: "email" });
		return true;
	};

	const validatePassword = () => {
		const nextErrors: SecurityFormErrors = {};
		const currentResult = existingPasswordSchema.safeParse(
			account.currentPassword,
		);
		if (!currentResult.success) {
			nextErrors.currentPassword = currentResult.error.issues[0]?.message ?? "";
		}

		const newResult = passwordSchema.safeParse(account.newPassword);
		if (!newResult.success) {
			nextErrors.newPassword = newResult.error.issues[0]?.message ?? "";
		}

		const confirmResult = passwordSchema.safeParse(account.confirmPassword);
		if (!confirmResult.success) {
			nextErrors.confirmPassword = confirmResult.error.issues[0]?.message ?? "";
		} else if (account.confirmPassword !== account.newPassword) {
			nextErrors.confirmPassword = t(
				"settings:settings_password_confirm_mismatch",
			);
		}

		dispatchAccount({ type: "set_errors", errors: nextErrors });
		return Object.keys(nextErrors).length === 0;
	};

	const handleEmailChangeSubmit = async (event: FormEvent<HTMLFormElement>) => {
		event.preventDefault();
		if (!user || !validateEmailChange()) return;

		try {
			dispatchAccount({ type: "set_email_busy", busy: true });
			await authService.requestEmailChange(account.newEmail.trim());
			dispatchAccount({ type: "request_email_change_success" });
			await refreshUser();
			toast.success(t("settings:settings_email_change_requested"));
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchAccount({ type: "set_email_busy", busy: false });
		}
	};

	const handleResendEmailChange = async () => {
		if (!user?.pending_email) return;

		try {
			dispatchAccount({ type: "set_resending_email_change", busy: true });
			await authService.resendEmailChange();
			toast.success(t("settings:settings_email_change_resent"));
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchAccount({ type: "set_resending_email_change", busy: false });
		}
	};

	const handlePasswordSubmit = async (event: FormEvent<HTMLFormElement>) => {
		event.preventDefault();
		if (!validatePassword()) return;

		try {
			dispatchAccount({ type: "set_password_busy", busy: true });
			const session = await authService.changePassword({
				current_password: account.currentPassword,
				new_password: account.newPassword,
			});
			syncSession(session.expiresIn);
			dispatchAccount({ type: "reset_password_form" });
			toast.success(t("settings:settings_password_updated"));
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchAccount({ type: "set_password_busy", busy: false });
		}
	};

	const handleRevokeSession = async (session: AuthSessionInfo) => {
		try {
			dispatchSessions({
				type: "set_revoke_busy_id",
				sessionId: session.id,
			});
			await authService.revokeSession(session.id);
			if (session.is_current) {
				toast.success(t("settings:settings_sessions_revoked_current"));
				forceLogout();
				navigate("/login", { replace: true });
			} else {
				dispatchSessions({
					type: "remove_session",
					sessionId: session.id,
				});
				toast.success(t("settings:settings_sessions_revoked"));
			}
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchSessions({ type: "set_revoke_busy_id", sessionId: null });
		}
	};

	const handleRevokeOtherSessions = async () => {
		try {
			dispatchSessions({ type: "set_revoke_others_busy", busy: true });
			const removed = await authService.revokeOtherSessions();
			dispatchSessions({ type: "keep_current_sessions" });
			toast.success(
				t("settings:settings_sessions_revoke_others_success", {
					count: removed,
				}),
			);
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchSessions({ type: "set_revoke_others_busy", busy: false });
		}
	};

	return {
		account,
		activePane,
		canSubmitEmailChange,
		canSubmitPassword,
		hasOtherSessions,
		sessionState,
		setActivePane,
		user,
		onConfirmPasswordChange: (value: string) =>
			dispatchAccount({
				type: "set_password_field",
				field: "confirmPassword",
				value,
			}),
		onCurrentPasswordChange: (value: string) =>
			dispatchAccount({
				type: "set_password_field",
				field: "currentPassword",
				value,
			}),
		onEmailSubmit: handleEmailChangeSubmit,
		onNewEmailChange: (value: string) =>
			dispatchAccount({ type: "set_new_email", value }),
		onNewPasswordChange: (value: string) =>
			dispatchAccount({
				type: "set_password_field",
				field: "newPassword",
				value,
			}),
		onPasswordSubmit: handlePasswordSubmit,
		onRefreshSessions: loadSessions,
		onResendEmailChange: handleResendEmailChange,
		onRevokeOtherSessions: handleRevokeOtherSessions,
		onRevokeSession: handleRevokeSession,
	};
}
