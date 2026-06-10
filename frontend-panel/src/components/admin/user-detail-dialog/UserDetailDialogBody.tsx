import { useReducer } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { passwordSchema } from "@/lib/validation";
import { adminUserService } from "@/services/adminService";
import type {
	StoragePolicyGroup,
	UpdateUserRequest,
	UserInfo,
	UserRole,
	UserStatus,
} from "@/types/api";
import { buildPolicyGroupOptions, type UserPasswordErrors } from "./types";
import { UserDetailSidebar } from "./UserDetailSidebar";
import { UserPolicyGroupSection } from "./UserPolicyGroupSection";
import { UserProfileSection } from "./UserProfileSection";
import { UserSecurityActionsSection } from "./UserSecurityActionsSection";

interface UserDetailDraftState {
	confirmPasswordValue: string;
	draftEmailVerified: boolean;
	draftMustChangePassword: boolean;
	draftPolicyGroupId: number | null;
	draftRole: UserRole;
	draftStatus: UserStatus;
	passwordErrors: UserPasswordErrors;
	passwordValue: string;
	quotaValue: string;
	resettingMfa: boolean;
	revokingSessions: boolean;
	savingPassword: boolean;
	savingProfile: boolean;
}

type BusyField =
	| "resettingMfa"
	| "revokingSessions"
	| "savingPassword"
	| "savingProfile";

type UserDetailDraftAction =
	| { type: "clear_password_error"; field: keyof UserPasswordErrors }
	| { type: "password_reset_success" }
	| { type: "set_busy"; field: BusyField; value: boolean }
	| { type: "set_confirm_password_value"; value: string }
	| { type: "set_draft_email_verified"; value: boolean }
	| { type: "set_draft_must_change_password"; value: boolean }
	| { type: "set_draft_policy_group_id"; value: number | null }
	| { type: "set_draft_role"; value: UserRole }
	| { type: "set_draft_status"; value: UserStatus }
	| { type: "set_password_errors"; errors: UserPasswordErrors }
	| { type: "set_password_value"; value: string }
	| { type: "set_quota_value"; value: string };

interface UserDetailDialogBodyProps {
	onClose: () => void;
	onRefreshPolicyGroups: () => Promise<void>;
	onUpdate: (id: number, data: UpdateUserRequest) => Promise<void>;
	policyGroups: StoragePolicyGroup[];
	policyGroupsLoading: boolean;
	user: UserInfo;
}

function quotaMbValue(user: UserInfo) {
	return user.storage_quota > 0
		? String(Math.round(user.storage_quota / 1024 / 1024))
		: "";
}

function createUserDraftState(user: UserInfo): UserDetailDraftState {
	return {
		confirmPasswordValue: "",
		draftEmailVerified: user.email_verified,
		draftMustChangePassword: user.must_change_password,
		draftPolicyGroupId: user.policy_group_id ?? null,
		draftRole: user.role,
		draftStatus: user.status,
		passwordErrors: {},
		passwordValue: "",
		quotaValue: quotaMbValue(user),
		resettingMfa: false,
		revokingSessions: false,
		savingPassword: false,
		savingProfile: false,
	};
}

function userDetailDraftReducer(
	state: UserDetailDraftState,
	action: UserDetailDraftAction,
): UserDetailDraftState {
	switch (action.type) {
		case "clear_password_error":
			return {
				...state,
				passwordErrors: {
					...state.passwordErrors,
					[action.field]: undefined,
				},
			};
		case "password_reset_success":
			return {
				...state,
				confirmPasswordValue: "",
				passwordErrors: {},
				passwordValue: "",
			};
		case "set_busy":
			return {
				...state,
				[action.field]: action.value,
			};
		case "set_confirm_password_value":
			return {
				...state,
				confirmPasswordValue: action.value,
			};
		case "set_draft_email_verified":
			return {
				...state,
				draftEmailVerified: action.value,
			};
		case "set_draft_must_change_password":
			return {
				...state,
				draftMustChangePassword: action.value,
			};
		case "set_draft_policy_group_id":
			return {
				...state,
				draftPolicyGroupId: action.value,
			};
		case "set_draft_role":
			return {
				...state,
				draftRole: action.value,
			};
		case "set_draft_status":
			return {
				...state,
				draftStatus: action.value,
			};
		case "set_password_errors":
			return {
				...state,
				passwordErrors: action.errors,
			};
		case "set_password_value":
			return {
				...state,
				passwordValue: action.value,
			};
		case "set_quota_value":
			return {
				...state,
				quotaValue: action.value,
			};
	}
}

export function UserDetailDialogBody({
	onClose,
	onRefreshPolicyGroups,
	onUpdate,
	policyGroups,
	policyGroupsLoading,
	user,
}: UserDetailDialogBodyProps) {
	const { t } = useTranslation(["admin", "core"]);
	const [state, dispatch] = useReducer(
		userDetailDraftReducer,
		user,
		createUserDraftState,
	);
	const {
		confirmPasswordValue,
		draftEmailVerified,
		draftMustChangePassword,
		draftPolicyGroupId,
		draftRole,
		draftStatus,
		passwordErrors,
		passwordValue,
		quotaValue,
		resettingMfa,
		revokingSessions,
		savingPassword,
		savingProfile,
	} = state;

	const quota = user.storage_quota ?? 0;
	const used = user.storage_used ?? 0;
	const pct = quota > 0 ? Math.min((used / quota) * 100, 100) : 0;
	const isInitialAdmin = user.id === 1;
	const currentQuotaMb = quotaMbValue(user);
	// Admin PATCH supports assigning a group, but not clearing an existing one.
	const hasPolicyGroupChange =
		draftPolicyGroupId != null &&
		draftPolicyGroupId !== (user.policy_group_id ?? null);
	const hasProfileChanges =
		draftEmailVerified !== user.email_verified ||
		draftMustChangePassword !== user.must_change_password ||
		draftRole !== user.role ||
		draftStatus !== user.status ||
		quotaValue !== currentQuotaMb ||
		hasPolicyGroupChange;
	const currentAssignedPolicyGroup =
		user.policy_group_id == null
			? null
			: (policyGroups.find((group) => group.id === user.policy_group_id) ??
				null);
	const policyGroupOptions = buildPolicyGroupOptions(
		policyGroups,
		draftPolicyGroupId,
	);
	const assignedPolicyGroupIsInvalid =
		!policyGroupsLoading &&
		user.policy_group_id != null &&
		(currentAssignedPolicyGroup === null ||
			!currentAssignedPolicyGroup.is_enabled ||
			currentAssignedPolicyGroup.items.length === 0);
	const statusOptions = [
		{ label: t("core:active"), value: "active" },
		{ label: t("core:disabled_status"), value: "disabled" },
	] satisfies ReadonlyArray<{ label: string; value: UserStatus }>;
	const roleOptions = [
		{ label: t("role_admin"), value: "admin" },
		{ label: t("role_user"), value: "user" },
	] satisfies ReadonlyArray<{ label: string; value: UserRole }>;
	const emailVerificationOptions = [
		{ label: t("email_verified"), value: "verified" },
		{ label: t("email_unverified"), value: "unverified" },
	] satisfies ReadonlyArray<{
		label: string;
		value: "verified" | "unverified";
	}>;

	const runDialogAction = async (
		field: BusyField,
		action: () => Promise<void>,
		successMessage: string,
	) => {
		try {
			dispatch({ type: "set_busy", field, value: true });
			await action();
			toast.success(successMessage);
		} catch (e) {
			handleApiError(e);
		} finally {
			dispatch({ type: "set_busy", field, value: false });
		}
	};

	const handleProfileSave = async () => {
		const mb = Number.parseInt(quotaValue, 10);
		const newQuota = Number.isNaN(mb) || mb <= 0 ? 0 : mb * 1024 * 1024;
		const data: UpdateUserRequest = {};

		if (draftEmailVerified !== user.email_verified) {
			data.email_verified = draftEmailVerified;
		}
		if (draftMustChangePassword !== user.must_change_password) {
			data.must_change_password = draftMustChangePassword;
		}
		if (draftRole !== user.role) data.role = draftRole;
		if (draftStatus !== user.status) data.status = draftStatus;
		if (newQuota !== (user.storage_quota ?? 0)) data.storage_quota = newQuota;
		if (hasPolicyGroupChange) {
			data.policy_group_id = draftPolicyGroupId;
		}
		if (Object.keys(data).length === 0) return;

		try {
			dispatch({ type: "set_busy", field: "savingProfile", value: true });
			await onUpdate(user.id, data);
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatch({ type: "set_busy", field: "savingProfile", value: false });
		}
	};

	const handlePasswordReset = async () => {
		const nextErrors: { confirm?: string; password?: string } = {};
		const passwordResult = passwordSchema.safeParse(passwordValue);
		if (!passwordResult.success) {
			nextErrors.password = passwordResult.error.issues[0]?.message ?? "";
		}
		if (confirmPasswordValue !== passwordValue) {
			nextErrors.confirm = t("password_confirm_mismatch");
		}
		dispatch({ type: "set_password_errors", errors: nextErrors });
		if (Object.keys(nextErrors).length > 0) return;

		await runDialogAction(
			"savingPassword",
			async () => {
				await adminUserService.resetPassword(user.id, {
					password: passwordValue,
				});
				dispatch({ type: "password_reset_success" });
			},
			t("password_reset_success"),
		);
	};

	const handleSessionRevoke = async () => {
		await runDialogAction(
			"revokingSessions",
			async () => {
				await adminUserService.revokeSessions(user.id);
			},
			t("revoke_sessions_success"),
		);
	};

	const handleMfaReset = async () => {
		await runDialogAction(
			"resettingMfa",
			async () => {
				await adminUserService.resetMfa(user.id);
			},
			t("reset_mfa_success"),
		);
	};

	return (
		<>
			<div className="flex min-h-0 flex-1 flex-col overflow-y-auto lg:overflow-hidden">
				<div className="flex min-h-full flex-col lg:h-full lg:min-h-0 lg:flex-1 lg:flex-row">
					<UserDetailSidebar
						quota={quota}
						usagePercentage={pct}
						used={used}
						user={user}
					/>

					<div className="min-h-0 min-w-0 lg:flex-1 lg:overflow-y-auto">
						<div className="space-y-4 p-6 max-lg:p-4">
							<UserProfileSection
								draftEmailVerified={draftEmailVerified}
								draftRole={draftRole}
								draftStatus={draftStatus}
								emailVerificationOptions={emailVerificationOptions}
								isInitialAdmin={isInitialAdmin}
								onDraftEmailVerifiedChange={(value) =>
									dispatch({
										type: "set_draft_email_verified",
										value,
									})
								}
								onDraftRoleChange={(value) =>
									dispatch({ type: "set_draft_role", value })
								}
								onDraftStatusChange={(value) =>
									dispatch({ type: "set_draft_status", value })
								}
								onQuotaValueChange={(value) =>
									dispatch({ type: "set_quota_value", value })
								}
								quotaValue={quotaValue}
								roleOptions={roleOptions}
								savingProfile={savingProfile}
								statusOptions={statusOptions}
								user={user}
							/>
							<UserPolicyGroupSection
								assignedPolicyGroupIsInvalid={assignedPolicyGroupIsInvalid}
								draftPolicyGroupId={draftPolicyGroupId}
								onDraftPolicyGroupIdChange={(value) =>
									dispatch({
										type: "set_draft_policy_group_id",
										value,
									})
								}
								onRefreshPolicyGroups={onRefreshPolicyGroups}
								policyGroupOptions={policyGroupOptions}
								policyGroupsLoading={policyGroupsLoading}
								savingProfile={savingProfile}
							/>
							<UserSecurityActionsSection
								confirmPasswordValue={confirmPasswordValue}
								mustChangePassword={draftMustChangePassword}
								onConfirmPasswordValueChange={(value) => {
									dispatch({ type: "set_confirm_password_value", value });
									dispatch({
										type: "clear_password_error",
										field: "confirm",
									});
								}}
								onPasswordReset={handlePasswordReset}
								onPasswordValueChange={(value) => {
									dispatch({ type: "set_password_value", value });
									dispatch({
										type: "clear_password_error",
										field: "password",
									});
								}}
								onMustChangePasswordChange={(value) =>
									dispatch({
										type: "set_draft_must_change_password",
										value,
									})
								}
								onMfaReset={handleMfaReset}
								onSessionRevoke={handleSessionRevoke}
								passwordErrors={passwordErrors}
								passwordValue={passwordValue}
								resettingMfa={resettingMfa}
								revokingSessions={revokingSessions}
								savingPassword={savingPassword}
								savingProfile={savingProfile}
							/>
						</div>
					</div>
				</div>
			</div>
			<DialogFooter className="mx-0 mb-0 w-full shrink-0 border-t bg-muted/10 px-6 py-4 max-lg:px-4 max-lg:py-3 sm:flex-row sm:items-center sm:justify-between">
				<p className="text-xs text-muted-foreground">
					{t("user_details_footer_hint")}
				</p>
				<div className="flex w-full flex-col-reverse gap-2 sm:w-auto sm:flex-row sm:items-center sm:justify-end">
					<Button variant="outline" onClick={onClose}>
						{t("core:close")}
					</Button>
					{hasProfileChanges ? (
						<Button
							onClick={() => void handleProfileSave()}
							disabled={savingProfile}
						>
							{savingProfile ? (
								<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
							) : null}
							{t("save_changes")}
						</Button>
					) : null}
				</div>
			</DialogFooter>
		</>
	);
}
