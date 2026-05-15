import { type FormEvent, useEffect, useEffectEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLocation, useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { SettingsSection } from "@/components/common/SettingsScaffold";
import { SecurityEmailSection } from "@/components/settings/security-settings/SecurityEmailSection";
import { SecurityPasskeysSection } from "@/components/settings/security-settings/SecurityPasskeysSection";
import { SecurityPasswordSection } from "@/components/settings/security-settings/SecurityPasswordSection";
import { SecuritySessionsSection } from "@/components/settings/security-settings/SecuritySessionsSection";
import { SecuritySummaryCard } from "@/components/settings/security-settings/SecuritySummaryCard";
import type { SecurityFormErrors } from "@/components/settings/security-settings/types";
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

export function SecuritySettingsView() {
	const { t } = useTranslation(["auth", "core", "settings"]);
	const location = useLocation();
	const navigate = useNavigate();
	const user = useAuthStore((s) => s.user);
	const refreshUser = useAuthStore((s) => s.refreshUser);
	const syncSession = useAuthStore((s) => s.syncSession);
	const [emailBusy, setEmailBusy] = useState(false);
	const [newEmail, setNewEmail] = useState("");
	const [passwordBusy, setPasswordBusy] = useState(false);
	const [resendingEmailChange, setResendingEmailChange] = useState(false);
	const [currentPassword, setCurrentPassword] = useState("");
	const [newPassword, setNewPassword] = useState("");
	const [confirmPassword, setConfirmPassword] = useState("");
	const [errors, setErrors] = useState<SecurityFormErrors>({});
	const [sessions, setSessions] = useState<AuthSessionInfo[]>([]);
	const [sessionsLoading, setSessionsLoading] = useState(false);
	const [revokeBusyId, setRevokeBusyId] = useState<string | null>(null);
	const [revokeOthersBusy, setRevokeOthersBusy] = useState(false);

	useEffect(() => {
		const verification = getContactVerificationRedirectState(location.search);
		if (!verification) {
			return;
		}

		switch (verification.status) {
			case "email-changed":
				if (!verification.email) {
					return;
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
				hash: location.hash,
				pathname: location.pathname,
				search: clearContactVerificationRedirectSearch(location.search),
			},
			{ replace: true },
		);
	}, [location.hash, location.pathname, location.search, navigate, t]);

	const loadSessions = useEffectEvent(async () => {
		try {
			setSessionsLoading(true);
			setSessions(await authService.listSessions());
		} catch (error) {
			handleApiError(error);
		} finally {
			setSessionsLoading(false);
		}
	});

	useEffect(() => {
		void loadSessions();
	}, []);

	const canSubmitPassword =
		!passwordBusy &&
		currentPassword.length > 0 &&
		newPassword.length > 0 &&
		confirmPassword.length > 0;
	const canSubmitEmailChange =
		!emailBusy && !!user?.email_verified && newEmail.trim().length > 0;
	const hasOtherSessions = sessions.some((session) => !session.is_current);
	const validateEmailChange = () => {
		const email = newEmail.trim();
		const emailResult = emailSchema.safeParse(email);
		if (!emailResult.success) {
			setErrors((prev) => ({
				...prev,
				email: emailResult.error.issues[0]?.message ?? "",
			}));
			return false;
		}

		if (email === user?.email) {
			setErrors((prev) => ({
				...prev,
				email: t("settings:settings_email_change_same"),
			}));
			return false;
		}

		setErrors((prev) => ({ ...prev, email: undefined }));
		return true;
	};

	const validate = () => {
		const nextErrors: SecurityFormErrors = {};
		const currentResult = existingPasswordSchema.safeParse(currentPassword);
		if (!currentResult.success) {
			nextErrors.currentPassword = currentResult.error.issues[0]?.message ?? "";
		}

		const newResult = passwordSchema.safeParse(newPassword);
		if (!newResult.success) {
			nextErrors.newPassword = newResult.error.issues[0]?.message ?? "";
		}

		const confirmResult = passwordSchema.safeParse(confirmPassword);
		if (!confirmResult.success) {
			nextErrors.confirmPassword = confirmResult.error.issues[0]?.message ?? "";
		} else if (confirmPassword !== newPassword) {
			nextErrors.confirmPassword = t(
				"settings:settings_password_confirm_mismatch",
			);
		}

		setErrors(nextErrors);
		return Object.keys(nextErrors).length === 0;
	};

	const handleEmailChangeSubmit = async (event: FormEvent<HTMLFormElement>) => {
		event.preventDefault();
		if (!user || !validateEmailChange()) return;

		try {
			setEmailBusy(true);
			await authService.requestEmailChange(newEmail.trim());
			setNewEmail("");
			setErrors((prev) => ({ ...prev, email: undefined }));
			await refreshUser();
			toast.success(t("settings:settings_email_change_requested"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setEmailBusy(false);
		}
	};

	const handleResendEmailChange = async () => {
		if (!user?.pending_email) return;

		try {
			setResendingEmailChange(true);
			await authService.resendEmailChange();
			toast.success(t("settings:settings_email_change_resent"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setResendingEmailChange(false);
		}
	};

	const handlePasswordSubmit = async (event: FormEvent<HTMLFormElement>) => {
		event.preventDefault();
		if (!validate()) return;

		try {
			setPasswordBusy(true);
			const session = await authService.changePassword({
				current_password: currentPassword,
				new_password: newPassword,
			});
			syncSession(session.expiresIn);
			setCurrentPassword("");
			setNewPassword("");
			setConfirmPassword("");
			setErrors({});
			toast.success(t("settings:settings_password_updated"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setPasswordBusy(false);
		}
	};

	const handleRevokeSession = async (session: AuthSessionInfo) => {
		try {
			setRevokeBusyId(session.id);
			await authService.revokeSession(session.id);
			if (session.is_current) {
				toast.success(t("settings:settings_sessions_revoked_current"));
				forceLogout();
				navigate("/login", { replace: true });
				return;
			}
			setSessions((prev) => prev.filter((item) => item.id !== session.id));
			toast.success(t("settings:settings_sessions_revoked"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setRevokeBusyId(null);
		}
	};

	const handleRevokeOtherSessions = async () => {
		try {
			setRevokeOthersBusy(true);
			const removed = await authService.revokeOtherSessions();
			setSessions((prev) => prev.filter((session) => session.is_current));
			toast.success(
				t("settings:settings_sessions_revoke_others_success", {
					count: removed,
				}),
			);
		} catch (error) {
			handleApiError(error);
		} finally {
			setRevokeOthersBusy(false);
		}
	};

	return (
		<SettingsSection
			title={t("settings:settings_security")}
			description={t("settings:settings_security_desc")}
			contentClassName="pt-4"
		>
			<div className="grid gap-5 rounded-xl border bg-muted/20 p-4 lg:grid-cols-[minmax(0,1fr)_280px]">
				<div className="space-y-4">
					<SecurityEmailSection
						canSubmitEmailChange={canSubmitEmailChange}
						emailBusy={emailBusy}
						emailError={errors.email}
						newEmail={newEmail}
						resendingEmailChange={resendingEmailChange}
						user={user}
						onNewEmailChange={(value) => {
							setNewEmail(value);
							setErrors((prev) => ({ ...prev, email: undefined }));
						}}
						onResendEmailChange={() => void handleResendEmailChange()}
						onSubmit={(event) => void handleEmailChangeSubmit(event)}
					/>

					<SecurityPasswordSection
						canSubmitPassword={canSubmitPassword}
						confirmPassword={confirmPassword}
						currentPassword={currentPassword}
						errors={errors}
						newPassword={newPassword}
						passwordBusy={passwordBusy}
						onConfirmPasswordChange={(value) => {
							setConfirmPassword(value);
							setErrors((prev) => ({
								...prev,
								confirmPassword: undefined,
							}));
						}}
						onCurrentPasswordChange={(value) => {
							setCurrentPassword(value);
							setErrors((prev) => ({
								...prev,
								currentPassword: undefined,
							}));
						}}
						onNewPasswordChange={(value) => {
							setNewPassword(value);
							setErrors((prev) => ({ ...prev, newPassword: undefined }));
						}}
						onSubmit={(event) => void handlePasswordSubmit(event)}
					/>

					<SecurityPasskeysSection />

					<SecuritySessionsSection
						hasOtherSessions={hasOtherSessions}
						revokeBusyId={revokeBusyId}
						revokeOthersBusy={revokeOthersBusy}
						sessions={sessions}
						sessionsLoading={sessionsLoading}
						onRefreshSessions={() => void loadSessions()}
						onRevokeOtherSessions={() => void handleRevokeOtherSessions()}
						onRevokeSession={(session) => void handleRevokeSession(session)}
					/>
				</div>

				<SecuritySummaryCard user={user} />
			</div>
		</SettingsSection>
	);
}
