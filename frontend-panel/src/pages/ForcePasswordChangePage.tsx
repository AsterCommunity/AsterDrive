import type { FormEvent } from "react";
import { useState } from "react";
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
import { existingPasswordSchema, passwordSchema } from "@/lib/validation";
import { authService } from "@/services/authService";
import { useAuthStore } from "@/stores/authStore";

export default function ForcePasswordChangePage() {
	const { t } = useTranslation(["auth", "core", "settings"]);
	const navigate = useNavigate();
	const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
	const isChecking = useAuthStore((s) => s.isChecking);
	const logout = useAuthStore((s) => s.logout);
	const mustChangePassword = useAuthStore(
		(s) => s.user?.must_change_password ?? false,
	);
	const refreshUser = useAuthStore((s) => s.refreshUser);
	const syncSession = useAuthStore((s) => s.syncSession);
	const [currentPassword, setCurrentPassword] = useState("");
	const [newPassword, setNewPassword] = useState("");
	const [confirmPassword, setConfirmPassword] = useState("");
	const [showPasswords, setShowPasswords] = useState(false);
	const [submitting, setSubmitting] = useState(false);
	const [signingOut, setSigningOut] = useState(false);
	const [errors, setErrors] = useState<{
		confirm?: string;
		current?: string;
		next?: string;
	}>({});

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
		const nextErrors: typeof errors = {};
		const currentResult = existingPasswordSchema.safeParse(currentPassword);
		if (!currentResult.success) {
			nextErrors.current = currentResult.error.issues[0]?.message ?? "";
		}
		const nextResult = passwordSchema.safeParse(newPassword);
		if (!nextResult.success) {
			nextErrors.next = nextResult.error.issues[0]?.message ?? "";
		}
		if (confirmPassword !== newPassword) {
			nextErrors.confirm = t("settings:settings_password_confirm_mismatch");
		}
		setErrors(nextErrors);
		return Object.keys(nextErrors).length === 0;
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
			await refreshUser();
			toast.success(t("force_password_change_success"));
			navigate("/", { replace: true });
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
								setCurrentPassword(event.target.value);
								setErrors((current) => ({ ...current, current: undefined }));
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
								setNewPassword(event.target.value);
								setErrors((current) => ({ ...current, next: undefined }));
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
								setConfirmPassword(event.target.value);
								setErrors((current) => ({ ...current, confirm: undefined }));
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
							onClick={() => setShowPasswords((value) => !value)}
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
