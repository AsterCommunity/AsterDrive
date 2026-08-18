import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLocation, useNavigate } from "react-router-dom";
import { AuthPageShell } from "@/components/auth/AuthPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { handleApiError } from "@/hooks/useApiError";
import { usePageTitle } from "@/hooks/usePageTitle";
import { passwordSchema } from "@/lib/validation";
import { authService } from "@/services/authService";
import { ApiError } from "@/services/http";
import { ApiErrorCode } from "@/types/api-helpers";

type ResetStatus = "form" | "missing" | "invalid" | "expired";

function readToken(search: string) {
	return new URLSearchParams(search).get("token")?.trim() ?? "";
}

export default function ResetPasswordPage() {
	const { t } = useTranslation(["auth", "core"]);
	const { search } = useLocation();
	const navigate = useNavigate();
	const token = useMemo(() => readToken(search), [search]);
	const [password, setPassword] = useState("");
	const [showPassword, setShowPassword] = useState(false);
	const [submitting, setSubmitting] = useState(false);
	const [passwordError, setPasswordError] = useState("");
	const [status, setStatus] = useState<ResetStatus>(token ? "form" : "missing");

	usePageTitle(t("reset_password_title"));

	const title =
		status === "missing"
			? t("reset_password_missing_token_title")
			: status === "invalid"
				? t("reset_password_invalid_title")
				: status === "expired"
					? t("reset_password_expired_title")
					: t("reset_password_title");

	const description =
		status === "missing"
			? t("reset_password_missing_token_desc")
			: status === "invalid"
				? t("reset_password_invalid_desc")
				: status === "expired"
					? t("reset_password_expired_desc")
					: t("auth:reset_password_desc");

	const handleSubmit = async (event: React.FormEvent) => {
		event.preventDefault();
		if (status !== "form") {
			return;
		}

		const result = passwordSchema.safeParse(password);
		if (!result.success) {
			setPasswordError(result.error.issues[0]?.message ?? "");
			return;
		}

		try {
			setSubmitting(true);
			await authService.confirmPasswordReset({
				token,
				new_password: password,
			});
			navigate("/login?password_reset=success", { replace: true });
		} catch (error) {
			if (error instanceof ApiError) {
				if (error.code === ApiErrorCode.ContactVerificationInvalid) {
					setStatus("invalid");
					return;
				}
				if (error.code === ApiErrorCode.ContactVerificationExpired) {
					setStatus("expired");
					return;
				}
			}
			handleApiError(error);
		} finally {
			setSubmitting(false);
		}
	};

	return (
		<AuthPageShell>
			<div className="mb-6 space-y-1">
				<h1 className="text-xl font-semibold tracking-tight">{title}</h1>
				<p className="text-sm text-muted-foreground">{description}</p>
			</div>

			{status === "form" ? (
				<form onSubmit={handleSubmit} className="space-y-4">
					<div className="space-y-1.5">
						<Label htmlFor="reset-password" className="text-sm">
							{t("core:password")}
						</Label>
						<div className="relative">
							<Input
								id="reset-password"
								type={showPassword ? "text" : "password"}
								value={password}
								onChange={(event) => {
									setPassword(event.target.value);
									if (passwordError) {
										const next = passwordSchema.safeParse(event.target.value);
										setPasswordError(
											next.success ? "" : (next.error.issues[0]?.message ?? ""),
										);
									}
								}}
								autoComplete="new-password"
								className={
									passwordError
										? "pr-10 border-destructive focus-visible:ring-destructive"
										: "pr-10"
								}
							/>
							<button
								type="button"
								className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground transition-colors hover:text-foreground"
								onClick={() => setShowPassword((value) => !value)}
								tabIndex={-1}
								aria-label={
									showPassword
										? t("core:hide_password")
										: t("core:show_password")
								}
							>
								{showPassword ? (
									<Icon name="EyeSlash" className="size-4" />
								) : (
									<Icon name="Eye" className="size-4" />
								)}
							</button>
						</div>
						{passwordError ? (
							<p className="text-xs text-destructive">{passwordError}</p>
						) : null}
					</div>

					<Button
						type="submit"
						className="h-10 w-full"
						disabled={submitting || password.length === 0}
					>
						{submitting ? (
							<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
						) : null}
						{submitting ? t("resetting_password") : t("reset_password_submit")}
					</Button>
				</form>
			) : (
				<div className="space-y-3">
					<Button
						type="button"
						className="h-10 w-full"
						onClick={() => navigate("/login")}
					>
						{t("go_to_login")}
					</Button>
					<Button
						type="button"
						variant="outline"
						className="h-10 w-full"
						onClick={() => navigate("/login")}
					>
						{t("forgot_password")}
					</Button>
				</div>
			)}
		</AuthPageShell>
	);
}
