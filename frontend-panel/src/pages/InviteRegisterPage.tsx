import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate, useParams } from "react-router-dom";
import { AsterDriveWordmark } from "@/components/common/AsterDriveWordmark";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { handleApiError } from "@/hooks/useApiError";
import { usePageTitle } from "@/hooks/usePageTitle";
import { passwordSchema, usernameSchema } from "@/lib/validation";
import { authService } from "@/services/authService";
import { ApiError } from "@/services/http";
import type { PublicUserInvitationInfo } from "@/types/api";
import { ApiErrorCode } from "@/types/api-helpers";

type InviteStatus =
	| "loading"
	| "form"
	| "missing"
	| "invalid"
	| "expired"
	| "revoked"
	| "accepted";

function normalizeToken(value: string | undefined) {
	return value?.trim() ?? "";
}

function statusFromInvitationError(error: ApiError): InviteStatus | null {
	if (error.code === ApiErrorCode.AuthInvitationInvalid) {
		return "invalid";
	}
	if (error.code === ApiErrorCode.AuthInvitationExpired) {
		return "expired";
	}
	if (error.code === ApiErrorCode.AuthInvitationRevoked) {
		return "revoked";
	}
	if (error.code === ApiErrorCode.AuthInvitationAccepted) {
		return "accepted";
	}
	return null;
}

export default function InviteRegisterPage() {
	const { t } = useTranslation(["auth", "core"]);
	const navigate = useNavigate();
	const params = useParams();
	const token = useMemo(() => normalizeToken(params.token), [params.token]);
	const [invitation, setInvitation] = useState<PublicUserInvitationInfo | null>(
		null,
	);
	const [username, setUsername] = useState("");
	const [password, setPassword] = useState("");
	const [showPassword, setShowPassword] = useState(false);
	const [submitting, setSubmitting] = useState(false);
	const [usernameError, setUsernameError] = useState("");
	const [passwordError, setPasswordError] = useState("");
	const [status, setStatus] = useState<InviteStatus>(
		token ? "loading" : "missing",
	);

	usePageTitle(t("invitation_register_title"));

	useEffect(() => {
		let canceled = false;

		if (!token) {
			setStatus("missing");
			setInvitation(null);
			return () => {
				canceled = true;
			};
		}

		setStatus("loading");
		authService
			.verifyInvitation(token)
			.then((data) => {
				if (canceled) return;
				setInvitation(data);
				setStatus("form");
			})
			.catch((error) => {
				if (canceled) return;
				if (error instanceof ApiError) {
					const nextStatus = statusFromInvitationError(error);
					if (nextStatus) {
						setStatus(nextStatus);
						return;
					}
				}
				handleApiError(error);
				setStatus("invalid");
			});

		return () => {
			canceled = true;
		};
	}, [token]);

	const title =
		status === "missing"
			? t("invitation_missing_title")
			: status === "invalid"
				? t("invitation_invalid_title")
				: status === "expired"
					? t("invitation_expired_title")
					: status === "revoked"
						? t("invitation_revoked_title")
						: status === "accepted"
							? t("invitation_accepted_title")
							: status === "loading"
								? t("invitation_loading_title")
								: t("invitation_register_title");

	const description =
		status === "missing"
			? t("invitation_missing_desc")
			: status === "invalid"
				? t("invitation_invalid_desc")
				: status === "expired"
					? t("invitation_expired_desc")
					: status === "revoked"
						? t("invitation_revoked_desc")
						: status === "accepted"
							? t("invitation_accepted_desc")
							: status === "loading"
								? t("invitation_loading_desc")
								: t("invitation_register_desc", {
										email: invitation?.email ?? "",
									});

	const validateUsername = (value: string) => {
		const result = usernameSchema.safeParse(value.trim());
		const message = result.success
			? ""
			: (result.error.issues[0]?.message ?? "");
		setUsernameError(message);
		return result.success;
	};

	const validatePassword = (value: string) => {
		const result = passwordSchema.safeParse(value);
		const message = result.success
			? ""
			: (result.error.issues[0]?.message ?? "");
		setPasswordError(message);
		return result.success;
	};

	const handleSubmit = async (event: React.FormEvent) => {
		event.preventDefault();
		if (status !== "form") {
			return;
		}

		const usernameValid = validateUsername(username);
		const passwordValid = validatePassword(password);
		if (!usernameValid || !passwordValid) {
			return;
		}

		try {
			setSubmitting(true);
			await authService.acceptInvitation(token, {
				username: username.trim(),
				password,
			});
			navigate("/login?invitation=accepted", { replace: true });
		} catch (error) {
			if (error instanceof ApiError) {
				const nextStatus = statusFromInvitationError(error);
				if (nextStatus) {
					setStatus(nextStatus);
					return;
				}
			}
			handleApiError(error);
		} finally {
			setSubmitting(false);
		}
	};

	return (
		<div className="flex min-h-screen items-center justify-center bg-background p-6">
			<div className="w-full max-w-sm rounded-3xl border bg-card p-6 shadow-sm">
				<div className="mb-8 text-center">
					<AsterDriveWordmark
						alt="AsterDrive"
						className="mx-auto h-16 w-auto"
					/>
				</div>

				<div className="mb-6 space-y-1">
					<h1 className="text-xl font-semibold tracking-tight">{title}</h1>
					<p className="text-sm text-muted-foreground">{description}</p>
				</div>

				{status === "loading" ? (
					<div className="flex h-28 items-center justify-center">
						<Icon name="Spinner" className="size-5 animate-spin" />
					</div>
				) : status === "form" ? (
					<form onSubmit={handleSubmit} className="space-y-4">
						<div className="space-y-1.5">
							<Label htmlFor="invite-email" className="text-sm">
								{t("core:email")}
							</Label>
							<Input
								id="invite-email"
								value={invitation?.email ?? ""}
								readOnly
								className="bg-muted/35"
							/>
						</div>
						<div className="space-y-1.5">
							<Label htmlFor="invite-username" className="text-sm">
								{t("core:username")}
							</Label>
							<Input
								id="invite-username"
								value={username}
								onChange={(event) => {
									const value = event.target.value;
									setUsername(value);
									if (usernameError) {
										validateUsername(value);
									}
								}}
								autoComplete="username"
								className={
									usernameError
										? "border-destructive focus-visible:ring-destructive"
										: undefined
								}
								aria-invalid={!!usernameError}
							/>
							{usernameError ? (
								<p className="text-xs text-destructive">{usernameError}</p>
							) : null}
						</div>
						<div className="space-y-1.5">
							<Label htmlFor="invite-password" className="text-sm">
								{t("core:password")}
							</Label>
							<div className="relative">
								<Input
									id="invite-password"
									type={showPassword ? "text" : "password"}
									value={password}
									onChange={(event) => {
										const value = event.target.value;
										setPassword(value);
										if (passwordError) {
											validatePassword(value);
										}
									}}
									autoComplete="new-password"
									className={
										passwordError
											? "border-destructive pr-10 focus-visible:ring-destructive"
											: "pr-10"
									}
									aria-invalid={!!passwordError}
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
							disabled={
								submitting ||
								username.trim().length === 0 ||
								password.length === 0
							}
						>
							{submitting ? (
								<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
							) : null}
							{submitting
								? t("invitation_register_submitting")
								: t("invitation_register_submit")}
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
					</div>
				)}

				<p className="mt-8 text-center text-xs text-muted-foreground/50">
					Self-hosted cloud storage
				</p>
			</div>
		</div>
	);
}
