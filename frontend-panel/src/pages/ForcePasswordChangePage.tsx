import type { FormEvent } from "react";
import { useReducer, useState } from "react";
import { useTranslation } from "react-i18next";
import { Navigate, useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { AsterDriveWordmark } from "@/components/common/AsterDriveWordmark";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { handleApiError } from "@/hooks/useApiError";
import { usePageTitle } from "@/hooks/usePageTitle";
import { logger } from "@/lib/logger";
import {
	passwordChangeMatchSchema,
	passwordChangeSchema,
} from "@/lib/validation";
import { authService } from "@/services/authService";
import { useAuthStore } from "@/stores/authStore";

type PasswordChangeField =
	| "confirmPassword"
	| "currentPassword"
	| "newPassword";

type PasswordChangeErrors = {
	confirm?: string;
	current?: string;
	next?: string;
};

type ValidationIssue = {
	message: string;
	path: PropertyKey[];
};

type PasswordChangeFormState = {
	confirmPassword: string;
	currentPassword: string;
	errors: PasswordChangeErrors;
	newPassword: string;
	showPasswords: boolean;
};

type PasswordChangeFormAction =
	| {
			type: "fieldChanged";
			confirmMismatchMessage: string;
			field: PasswordChangeField;
			matchErrors: PasswordChangeErrors;
			samePasswordMessage: string;
			value: string;
	  }
	| { type: "validated"; errors: PasswordChangeErrors }
	| { type: "showPasswordsToggled" };

const initialPasswordChangeFormState: PasswordChangeFormState = {
	confirmPassword: "",
	currentPassword: "",
	errors: {},
	newPassword: "",
	showPasswords: false,
};

function errorsFromIssues(issues: ValidationIssue[]): PasswordChangeErrors {
	const nextErrors: PasswordChangeErrors = {};
	for (const issue of issues) {
		const field = issue.path[0];
		if (field === "currentPassword") {
			nextErrors.current ??= issue.message;
		} else if (field === "newPassword") {
			nextErrors.next ??= issue.message;
		} else if (field === "confirmPassword") {
			nextErrors.confirm ??= issue.message;
		}
	}
	return nextErrors;
}

function passwordChangeFormReducer(
	state: PasswordChangeFormState,
	action: PasswordChangeFormAction,
): PasswordChangeFormState {
	switch (action.type) {
		case "fieldChanged": {
			const nextErrors = { ...state.errors };
			if (action.field === "currentPassword") {
				delete nextErrors.current;
				if (nextErrors.next === action.samePasswordMessage) {
					delete nextErrors.next;
				}
			} else if (action.field === "newPassword") {
				delete nextErrors.next;
				if (nextErrors.confirm === action.confirmMismatchMessage) {
					delete nextErrors.confirm;
				}
			} else {
				delete nextErrors.confirm;
			}
			return {
				...state,
				[action.field]: action.value,
				errors: {
					...nextErrors,
					...action.matchErrors,
				},
			};
		}
		case "showPasswordsToggled":
			return { ...state, showPasswords: !state.showPasswords };
		case "validated":
			return { ...state, errors: action.errors };
	}
}

export default function ForcePasswordChangePage() {
	const { t } = useTranslation(["auth", "core", "settings", "validation"]);
	const navigate = useNavigate();
	const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
	const isChecking = useAuthStore((s) => s.isChecking);
	const logout = useAuthStore((s) => s.logout);
	const mustChangePassword = useAuthStore(
		(s) => s.user?.must_change_password ?? false,
	);
	const refreshUser = useAuthStore((s) => s.refreshUser);
	const syncSession = useAuthStore((s) => s.syncSession);
	const [formState, dispatchForm] = useReducer(
		passwordChangeFormReducer,
		initialPasswordChangeFormState,
	);
	const [submitting, setSubmitting] = useState(false);
	const [signingOut, setSigningOut] = useState(false);
	const {
		confirmPassword,
		currentPassword,
		errors,
		newPassword,
		showPasswords,
	} = formState;

	usePageTitle(t("force_password_change_title"));

	if (isChecking) {
		return (
			<div className="flex min-h-screen items-center justify-center bg-background">
				<Icon
					name="Spinner"
					className="size-6 animate-spin text-muted-foreground"
				/>
			</div>
		);
	}
	if (!isAuthenticated) return <Navigate to="/login" replace />;
	if (!mustChangePassword) return <Navigate to="/" replace />;

	const validate = () => {
		const result = passwordChangeSchema.safeParse({
			confirmPassword,
			currentPassword,
			newPassword,
		});
		const nextErrors = result.success
			? {}
			: errorsFromIssues(result.error.issues);
		dispatchForm({ type: "validated", errors: nextErrors });
		return result.success;
	};

	const updateField = (
		field: "confirmPassword" | "currentPassword" | "newPassword",
		value: string,
	) => {
		const samePasswordMessage = t("validation:password_same_as_current");
		const confirmMismatchMessage = t("validation:password_confirm_mismatch");
		const nextValues = {
			confirmPassword,
			currentPassword,
			newPassword,
			[field]: value,
		};
		const matchResult = passwordChangeMatchSchema.safeParse(nextValues);
		dispatchForm({
			type: "fieldChanged",
			confirmMismatchMessage,
			field,
			matchErrors: matchResult.success
				? {}
				: errorsFromIssues(matchResult.error.issues),
			samePasswordMessage,
			value,
		});
	};

	const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
		event.preventDefault();
		if (!validate()) return;

		try {
			setSubmitting(true);
			const session = await authService.changePassword({
				current_password: currentPassword,
				new_password: newPassword,
			});
			syncSession(session.expiresIn);
			toast.success(t("force_password_change_success"));
			navigate("/", { replace: true });
			void Promise.resolve(refreshUser()).catch((error) => {
				logger.warn("refreshUser after password change failed", error);
			});
		} catch (error) {
			handleApiError(error);
		} finally {
			setSubmitting(false);
		}
	};

	const handleLogout = async () => {
		try {
			setSigningOut(true);
			await logout();
			navigate("/login", { replace: true });
		} catch (error) {
			setSigningOut(false);
			handleApiError(error);
		} finally {
			setSigningOut(false);
		}
	};

	return (
		<div className="flex min-h-screen items-center justify-center bg-background p-6">
			<div className="w-full max-w-md rounded-2xl border bg-card p-6 shadow-sm">
				<div className="mb-8 text-center">
					<AsterDriveWordmark
						alt="AsterDrive"
						className="mx-auto h-16 w-auto"
					/>
				</div>
				<div className="mb-6 space-y-2">
					<h1 className="text-xl font-semibold tracking-tight">
						{t("force_password_change_title")}
					</h1>
					<p className="text-sm text-muted-foreground">
						{t("force_password_change_desc")}
					</p>
				</div>
				<form onSubmit={handleSubmit} className="space-y-4">
					<div className="space-y-1.5">
						<Label htmlFor="force-current-password">
							{t("settings:settings_password_current")}
						</Label>
						<Input
							id="force-current-password"
							type={showPasswords ? "text" : "password"}
							value={currentPassword}
							onChange={(event) => {
								updateField("currentPassword", event.target.value);
							}}
							autoComplete="current-password"
							aria-invalid={errors.current ? true : undefined}
						/>
						{errors.current ? (
							<p className="text-xs text-destructive">{errors.current}</p>
						) : null}
					</div>
					<div className="space-y-1.5">
						<Label htmlFor="force-new-password">
							{t("settings:settings_password_new")}
						</Label>
						<Input
							id="force-new-password"
							type={showPasswords ? "text" : "password"}
							value={newPassword}
							onChange={(event) => {
								updateField("newPassword", event.target.value);
							}}
							autoComplete="new-password"
							aria-invalid={errors.next ? true : undefined}
						/>
						{errors.next ? (
							<p className="text-xs text-destructive">{errors.next}</p>
						) : (
							<p className="text-xs text-muted-foreground">
								{t("settings:settings_password_hint")}
							</p>
						)}
					</div>
					<div className="space-y-1.5">
						<Label htmlFor="force-confirm-password">
							{t("settings:settings_password_confirm")}
						</Label>
						<Input
							id="force-confirm-password"
							type={showPasswords ? "text" : "password"}
							value={confirmPassword}
							onChange={(event) => {
								updateField("confirmPassword", event.target.value);
							}}
							autoComplete="new-password"
							aria-invalid={errors.confirm ? true : undefined}
						/>
						{errors.confirm ? (
							<p className="text-xs text-destructive">{errors.confirm}</p>
						) : null}
					</div>
					<div className="flex items-center justify-between gap-3">
						<Button
							type="button"
							variant="ghost"
							onClick={() => dispatchForm({ type: "showPasswordsToggled" })}
						>
							<Icon
								name={showPasswords ? "EyeSlash" : "Eye"}
								className="mr-1 size-4"
							/>
							{showPasswords
								? t("core:hide_password")
								: t("core:show_password")}
						</Button>
						<Button type="submit" disabled={submitting} className="min-w-36">
							{submitting ? (
								<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
							) : (
								<Icon name="Key" className="mr-1 size-4" />
							)}
							{t("force_password_change_submit")}
						</Button>
					</div>
				</form>
				<div className="mt-6 border-t pt-4">
					<Button
						type="button"
						variant="outline"
						onClick={() => void handleLogout()}
						disabled={signingOut}
						className="w-full"
					>
						{signingOut ? (
							<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
						) : (
							<Icon name="SignOut" className="mr-1 size-4" />
						)}
						{t("core:logout")}
					</Button>
				</div>
			</div>
		</div>
	);
}
