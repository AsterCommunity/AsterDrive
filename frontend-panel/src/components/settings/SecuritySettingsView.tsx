import { useTranslation } from "react-i18next";
import { SettingsSection } from "@/components/common/SettingsScaffold";
import { SecurityAccountPane } from "@/components/settings/security-settings/SecurityAccountPane";
import { SecurityExternalAuthLinksSection } from "@/components/settings/security-settings/SecurityExternalAuthLinksSection";
import { SecurityMfaSection } from "@/components/settings/security-settings/SecurityMfaSection";
import { SecurityPasskeysSection } from "@/components/settings/security-settings/SecurityPasskeysSection";
import { SecuritySessionsSection } from "@/components/settings/security-settings/SecuritySessionsSection";
import { SecuritySummaryCard } from "@/components/settings/security-settings/SecuritySummaryCard";
import { SecurityTabsShell } from "@/components/settings/security-settings/SecurityTabsShell";
import { useSecuritySettingsController } from "@/components/settings/security-settings/useSecuritySettingsController";
import { TabsContent } from "@/components/ui/tabs";

export function SecuritySettingsView() {
	const { t } = useTranslation(["settings"]);
	const controller = useSecuritySettingsController();

	return (
		<SettingsSection
			title={t("settings:settings_security")}
			description={t("settings:settings_security_desc")}
			contentClassName="pt-4"
		>
			<div className="space-y-4">
				<SecuritySummaryCard
					sessionCount={controller.sessionState.sessions.length}
					user={controller.user}
				/>

				<SecurityTabsShell
					activePane={controller.activePane}
					onActivePaneChange={controller.setActivePane}
				>
					<TabsContent
						value="account"
						className="space-y-4 animate-in fade-in duration-150 motion-reduce:animate-none"
					>
						<SecurityAccountPane
							canSubmitEmailChange={controller.canSubmitEmailChange}
							canSubmitPassword={controller.canSubmitPassword}
							confirmPassword={controller.account.confirmPassword}
							currentPassword={controller.account.currentPassword}
							emailBusy={controller.account.emailBusy}
							emailError={controller.account.errors.email}
							errors={controller.account.errors}
							newEmail={controller.account.newEmail}
							newPassword={controller.account.newPassword}
							passwordBusy={controller.account.passwordBusy}
							resendingEmailChange={controller.account.resendingEmailChange}
							user={controller.user}
							onConfirmPasswordChange={controller.onConfirmPasswordChange}
							onCurrentPasswordChange={controller.onCurrentPasswordChange}
							onEmailSubmit={(event) => void controller.onEmailSubmit(event)}
							onNewEmailChange={controller.onNewEmailChange}
							onNewPasswordChange={controller.onNewPasswordChange}
							onPasswordSubmit={(event) =>
								void controller.onPasswordSubmit(event)
							}
							onResendEmailChange={() => void controller.onResendEmailChange()}
						/>
					</TabsContent>

					<TabsContent
						value="mfa"
						className="animate-in fade-in duration-150 motion-reduce:animate-none"
					>
						<SecurityMfaSection />
					</TabsContent>

					<TabsContent
						value="passkeys"
						className="animate-in fade-in duration-150 motion-reduce:animate-none"
					>
						<SecurityPasskeysSection />
					</TabsContent>

					<TabsContent
						value="external"
						className="animate-in fade-in duration-150 motion-reduce:animate-none"
					>
						<SecurityExternalAuthLinksSection />
					</TabsContent>

					<TabsContent
						value="sessions"
						className="animate-in fade-in duration-150 motion-reduce:animate-none"
					>
						<SecuritySessionsSection
							hasOtherSessions={controller.hasOtherSessions}
							revokeBusyId={controller.sessionState.revokeBusyId}
							revokeOthersBusy={controller.sessionState.revokeOthersBusy}
							sessions={controller.sessionState.sessions}
							sessionsLoading={controller.sessionState.sessionsLoading}
							onRefreshSessions={() => void controller.onRefreshSessions()}
							onRevokeOtherSessions={() =>
								void controller.onRevokeOtherSessions()
							}
							onRevokeSession={(session) =>
								void controller.onRevokeSession(session)
							}
						/>
					</TabsContent>
				</SecurityTabsShell>
			</div>
		</SettingsSection>
	);
}
