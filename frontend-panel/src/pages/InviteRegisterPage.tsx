import { useEffect, useMemo, useReducer, useRef } from "react";
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
import { useAuthStore } from "@/stores/authStore";
import type { MeResponse, PublicUserInvitationInfo } from "@/types/api";
import { ApiErrorCode } from "@/types/api-helpers";

type InviteStatus =
	| "loading"
	| "form"
	| "missing"
	| "invalid"
	| "expired"
	| "revoked"
	| "accepted"
	| "signed_in_same_email"
	| "signed_in_different_email"
	| "switching_to_invite"
	| "revealing_form";

type InviteRegisterState = {
	invitation: PublicUserInvitationInfo | null;
	password: string;
	passwordError: string;
	showPassword: boolean;
	signedInUser: MeResponse | null;
	status: InviteStatus;
	submitting: boolean;
	switchingContentVisible: boolean;
	username: string;
	usernameError: string;
};

type InviteRegisterAction =
	| { type: "acceptStatusFailed"; status: InviteStatus }
	| { type: "formRevealFinished" }
	| { type: "genericVerifyFailed" }
	| { type: "logoutCompleted" }
	| { type: "logoutFailed"; previousStatus: InviteStatus }
	| { type: "logoutTransitionFinished" }
	| { type: "logoutTransitionStarted" }
	| { type: "missingToken" }
	| { type: "passwordChanged"; password: string; passwordError?: string }
	| { type: "passwordErrorSet"; passwordError: string }
	| { type: "submitFinished" }
	| { type: "submitStarted" }
	| { type: "switchingContentRevealed" }
	| { type: "togglePasswordVisibility" }
	| { type: "usernameChanged"; username: string; usernameError?: string }
	| { type: "usernameErrorSet"; usernameError: string }
	| { type: "verifyStarted" }
	| {
			type: "verifySucceeded";
			currentUser: MeResponse | null;
			invitation: PublicUserInvitationInfo;
	  }
	| { type: "verifyStatusFailed"; status: InviteStatus };

type InviteTimerName = "continueTransition" | "formReveal" | "switchingReveal";
type InviteTimers = Map<InviteTimerName, ReturnType<typeof setTimeout>>;
type Translate = (key: string, values?: Record<string, unknown>) => string;

type InviteRegisterFormProps = {
	onPasswordChange: (value: string) => void;
	onSubmit: (event: React.FormEvent) => void;
	onTogglePasswordVisibility: () => void;
	onUsernameChange: (value: string) => void;
	password: string;
	passwordError: string;
	showPassword: boolean;
	status: InviteStatus;
	statusDescription: string;
	statusTitle: string;
	submitting: boolean;
	t: Translate;
	username: string;
	usernameError: string;
};

type SignedInInviteStateProps = {
	onContinue: () => void;
	onCurrentAccount: () => void;
	reverseActions?: boolean;
	signedInUser: MeResponse | null;
	statusDescription: string;
	statusTitle: string;
	submitting: boolean;
	t: Translate;
};

function normalizeToken(value: string | undefined) {
	return value?.trim() ?? "";
}

function createInitialInviteRegisterState(token: string): InviteRegisterState {
	return {
		invitation: null,
		password: "",
		passwordError: "",
		showPassword: false,
		signedInUser: null,
		status: token ? "loading" : "missing",
		submitting: false,
		switchingContentVisible: false,
		username: "",
		usernameError: "",
	};
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

function sameEmail(left: string, right: string) {
	return left.trim().toLowerCase() === right.trim().toLowerCase();
}

function getStatusTitleKey(status: InviteStatus) {
	switch (status) {
		case "missing":
			return "invitation_missing_title";
		case "invalid":
			return "invitation_invalid_title";
		case "expired":
			return "invitation_expired_title";
		case "revoked":
			return "invitation_revoked_title";
		case "accepted":
			return "invitation_accepted_title";
		case "signed_in_same_email":
			return "invitation_same_account_title";
		case "signed_in_different_email":
			return "invitation_account_mismatch_title";
		case "switching_to_invite":
			return "invitation_switching_title";
		case "revealing_form":
			return "invitation_register_title";
		case "loading":
			return "invitation_loading_title";
		case "form":
			return "invitation_register_title";
	}
}

function getStatusDescription(
	status: InviteStatus,
	statusTitleKey: string,
	t: Translate,
) {
	if (status === "signed_in_same_email") {
		return t("invitation_same_account_desc");
	}
	if (status === "signed_in_different_email") {
		return t("invitation_account_mismatch_desc");
	}
	if (status === "switching_to_invite") {
		return t("invitation_switching_desc");
	}
	if (status === "form" || status === "revealing_form") {
		return t("invitation_register_desc");
	}
	return t(statusTitleKey.replace("_title", "_desc"));
}

function getContentClassName(status: InviteStatus) {
	if (status === "switching_to_invite" || status === "revealing_form") {
		return "h-[16rem] overflow-hidden transition-[height] duration-300 ease-[cubic-bezier(0.22,1,0.36,1)] will-change-[height] motion-reduce:transition-none";
	}
	if (status === "form") {
		return "min-h-[16rem] overflow-visible";
	}
	if (
		status === "signed_in_same_email" ||
		status === "signed_in_different_email"
	) {
		return "h-48 overflow-hidden transition-[height] duration-300 ease-[cubic-bezier(0.22,1,0.36,1)] will-change-[height] motion-reduce:transition-none";
	}
	return "h-32 overflow-hidden transition-[height] duration-300 ease-[cubic-bezier(0.22,1,0.36,1)] will-change-[height] motion-reduce:transition-none";
}

function inviteRegisterReducer(
	state: InviteRegisterState,
	action: InviteRegisterAction,
): InviteRegisterState {
	switch (action.type) {
		case "acceptStatusFailed":
			return { ...state, status: action.status };
		case "formRevealFinished":
			return state.status === "revealing_form"
				? { ...state, status: "form" }
				: state;
		case "genericVerifyFailed":
			return { ...state, status: "invalid" };
		case "logoutCompleted":
			return { ...state, signedInUser: null };
		case "logoutFailed":
			return {
				...state,
				status: action.previousStatus,
				submitting: false,
				switchingContentVisible: false,
			};
		case "logoutTransitionFinished":
			return { ...state, status: "form", submitting: false };
		case "logoutTransitionStarted":
			return {
				...state,
				status: "switching_to_invite",
				submitting: true,
				switchingContentVisible: false,
			};
		case "missingToken":
			return {
				...state,
				invitation: null,
				signedInUser: null,
				status: "missing",
				submitting: false,
			};
		case "passwordChanged":
			return {
				...state,
				password: action.password,
				passwordError: action.passwordError ?? state.passwordError,
			};
		case "passwordErrorSet":
			return { ...state, passwordError: action.passwordError };
		case "submitFinished":
			return { ...state, submitting: false };
		case "submitStarted":
			return { ...state, submitting: true };
		case "switchingContentRevealed":
			return { ...state, switchingContentVisible: true };
		case "togglePasswordVisibility":
			return { ...state, showPassword: !state.showPassword };
		case "usernameChanged":
			return {
				...state,
				username: action.username,
				usernameError: action.usernameError ?? state.usernameError,
			};
		case "usernameErrorSet":
			return { ...state, usernameError: action.usernameError };
		case "verifyStarted":
			return {
				...state,
				invitation: null,
				password: "",
				passwordError: "",
				showPassword: false,
				signedInUser: null,
				status: "loading",
				submitting: false,
				switchingContentVisible: false,
				username: "",
				usernameError: "",
			};
		case "verifyStatusFailed":
			return { ...state, status: action.status };
		case "verifySucceeded":
			if (action.currentUser) {
				return {
					...state,
					invitation: action.invitation,
					signedInUser: action.currentUser,
					status: sameEmail(action.currentUser.email, action.invitation.email)
						? "signed_in_same_email"
						: "signed_in_different_email",
				};
			}
			return {
				...state,
				invitation: action.invitation,
				signedInUser: null,
				status: "revealing_form",
			};
	}
}

function clearInviteTimer(timers: InviteTimers, name: InviteTimerName) {
	const timer = timers.get(name);
	if (timer) {
		clearTimeout(timer);
		timers.delete(name);
	}
}

function scheduleInviteTimer(
	timers: InviteTimers,
	name: InviteTimerName,
	callback: () => void,
	delay: number,
) {
	clearInviteTimer(timers, name);
	const timer = setTimeout(() => {
		timers.delete(name);
		callback();
	}, delay);
	timers.set(name, timer);
}

function clearInviteTimers(timers: InviteTimers) {
	for (const timer of timers.values()) {
		clearTimeout(timer);
	}
	timers.clear();
}

function validateUsernameValue(value: string) {
	const result = usernameSchema.safeParse(value.trim());
	return {
		message: result.success ? "" : (result.error.issues[0]?.message ?? ""),
		valid: result.success,
	};
}

function validatePasswordValue(value: string) {
	const result = passwordSchema.safeParse(value);
	return {
		message: result.success ? "" : (result.error.issues[0]?.message ?? ""),
		valid: result.success,
	};
}

function InviteRegisterHeader({ t }: { t: Translate }) {
	return (
		<>
			<div className="mb-8 text-center">
				<AsterDriveWordmark alt="AsterDrive" className="mx-auto h-16 w-auto" />
			</div>

			<div className="mb-5 space-y-1">
				<h1 className="text-xl font-semibold">{t("invitation_page_title")}</h1>
				<p className="text-sm text-muted-foreground">
					{t("invitation_page_desc")}
				</p>
			</div>
		</>
	);
}

function InvitedAccountSummary({
	invitation,
	t,
}: {
	invitation: PublicUserInvitationInfo | null;
	t: Translate;
}) {
	if (!invitation) {
		return null;
	}

	return (
		<div className="mb-5 rounded-lg border bg-muted/30 px-3 py-2.5">
			<div className="text-xs font-medium text-muted-foreground">
				{t("invitation_invited_account")}
			</div>
			<div className="mt-1 flex min-w-0 items-center gap-2">
				<Icon
					name="EnvelopeSimple"
					className="size-4 shrink-0 text-muted-foreground"
				/>
				<span className="min-w-0 truncate text-sm font-medium">
					{invitation.email}
				</span>
			</div>
		</div>
	);
}

function LoadingInviteState({ statusTitle }: { statusTitle: string }) {
	return (
		<div className="flex h-32 items-center justify-center">
			<div className="flex items-center gap-2 text-sm text-muted-foreground">
				<Icon name="Spinner" className="size-4 animate-spin" />
				<span>{statusTitle}</span>
			</div>
		</div>
	);
}

function InviteRegisterForm({
	onPasswordChange,
	onSubmit,
	onTogglePasswordVisibility,
	onUsernameChange,
	password,
	passwordError,
	showPassword,
	status,
	statusDescription,
	statusTitle,
	submitting,
	t,
	username,
	usernameError,
}: InviteRegisterFormProps) {
	const animationClassName =
		status === "revealing_form"
			? "animate-in fade-in space-y-4 duration-150 motion-reduce:animate-none"
			: "animate-in fade-in slide-in-from-bottom-2 space-y-4 duration-300 motion-reduce:animate-none";

	return (
		<form onSubmit={onSubmit} className={animationClassName}>
			<div className="space-y-1">
				<h2 className="text-sm font-medium">{statusTitle}</h2>
				<p className="text-xs text-muted-foreground">{statusDescription}</p>
			</div>
			<div className="space-y-1.5">
				<Label htmlFor="invite-username" className="text-sm">
					{t("core:username")}
				</Label>
				<Input
					id="invite-username"
					value={username}
					onChange={(event) => onUsernameChange(event.target.value)}
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
						onChange={(event) => onPasswordChange(event.target.value)}
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
						onClick={onTogglePasswordVisibility}
						aria-label={
							showPassword ? t("core:hide_password") : t("core:show_password")
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
					submitting || username.trim().length === 0 || password.length === 0
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
	);
}

function SwitchingInviteState({
	statusDescription,
	statusTitle,
	switchingContentVisible,
}: {
	statusDescription: string;
	statusTitle: string;
	switchingContentVisible: boolean;
}) {
	return (
		<div className="flex h-[16rem] items-center justify-center">
			<div
				className={
					switchingContentVisible
						? "animate-in fade-in slide-in-from-bottom-1 space-y-3 text-center duration-200 motion-reduce:animate-none"
						: "opacity-0"
				}
				aria-hidden={!switchingContentVisible}
			>
				<div className="mx-auto flex size-8 items-center justify-center text-muted-foreground">
					<Icon name="Spinner" className="size-5 animate-spin" />
				</div>
				<div className="space-y-1">
					<h2 className="text-sm font-medium">{statusTitle}</h2>
					<p className="text-xs text-muted-foreground">{statusDescription}</p>
				</div>
			</div>
		</div>
	);
}

function CurrentAccountPanel({
	signedInUser,
	statusDescription,
	statusTitle,
	t,
}: {
	signedInUser: MeResponse | null;
	statusDescription: string;
	statusTitle: string;
	t: Translate;
}) {
	return (
		<div
			className="rounded-lg border bg-muted/30 p-3"
			data-testid="invite-account-status"
		>
			<h2 className="text-sm font-medium">{statusTitle}</h2>
			<p className="mt-1 text-xs text-muted-foreground">{statusDescription}</p>
			{signedInUser ? (
				<div className="mt-3 rounded-md bg-background/70 px-2.5 py-2">
					<div className="text-xs text-muted-foreground">
						{t("invitation_current_account")}
					</div>
					<div className="mt-0.5 truncate text-sm font-medium">
						{signedInUser.email}
					</div>
				</div>
			) : null}
		</div>
	);
}

function SignedInInviteState({
	onContinue,
	onCurrentAccount,
	reverseActions = false,
	signedInUser,
	statusDescription,
	statusTitle,
	submitting,
	t,
}: SignedInInviteStateProps) {
	const currentAccountButton = (
		<Button type="button" className="h-10 w-full" onClick={onCurrentAccount}>
			{t("invitation_go_to_current_account")}
		</Button>
	);
	const continueButton = (
		<Button
			type="button"
			variant={reverseActions ? undefined : "outline"}
			className="h-10 w-full"
			onClick={onContinue}
			disabled={submitting}
		>
			{submitting ? (
				<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
			) : null}
			{t("invitation_logout_and_continue")}
		</Button>
	);

	return (
		<div className="animate-in fade-in space-y-3 duration-200 motion-reduce:animate-none">
			<CurrentAccountPanel
				signedInUser={signedInUser}
				statusDescription={statusDescription}
				statusTitle={statusTitle}
				t={t}
			/>
			{reverseActions ? continueButton : currentAccountButton}
			{reverseActions ? (
				<Button
					type="button"
					variant="outline"
					className="h-10 w-full"
					onClick={onCurrentAccount}
					disabled={submitting}
				>
					{t("invitation_go_to_current_account")}
				</Button>
			) : (
				continueButton
			)}
		</div>
	);
}

function InviteErrorState({
	onGoToLogin,
	statusDescription,
	statusTitle,
	t,
}: {
	onGoToLogin: () => void;
	statusDescription: string;
	statusTitle: string;
	t: Translate;
}) {
	return (
		<div className="space-y-3">
			<div className="rounded-lg border bg-muted/30 p-3">
				<h2 className="text-sm font-medium">{statusTitle}</h2>
				<p className="mt-1 text-xs text-muted-foreground">
					{statusDescription}
				</p>
			</div>
			<Button type="button" className="h-10 w-full" onClick={onGoToLogin}>
				{t("go_to_login")}
			</Button>
		</div>
	);
}

function InviteRegisterContent({
	contentClassName,
	onCurrentAccount,
	onGoToLogin,
	onLogoutAndContinue,
	onPasswordChange,
	onSubmit,
	onTogglePasswordVisibility,
	onUsernameChange,
	state,
	statusDescription,
	statusTitle,
	t,
}: {
	contentClassName: string;
	onCurrentAccount: () => void;
	onGoToLogin: () => void;
	onLogoutAndContinue: () => void;
	onPasswordChange: (value: string) => void;
	onSubmit: (event: React.FormEvent) => void;
	onTogglePasswordVisibility: () => void;
	onUsernameChange: (value: string) => void;
	state: InviteRegisterState;
	statusDescription: string;
	statusTitle: string;
	t: Translate;
}) {
	const { status } = state;

	return (
		<div className={contentClassName} data-testid="invite-content">
			{status === "loading" ? (
				<LoadingInviteState statusTitle={statusTitle} />
			) : status === "form" || status === "revealing_form" ? (
				<InviteRegisterForm
					onPasswordChange={onPasswordChange}
					onSubmit={onSubmit}
					onTogglePasswordVisibility={onTogglePasswordVisibility}
					onUsernameChange={onUsernameChange}
					password={state.password}
					passwordError={state.passwordError}
					showPassword={state.showPassword}
					status={status}
					statusDescription={statusDescription}
					statusTitle={statusTitle}
					submitting={state.submitting}
					t={t}
					username={state.username}
					usernameError={state.usernameError}
				/>
			) : status === "switching_to_invite" ? (
				<SwitchingInviteState
					statusDescription={statusDescription}
					statusTitle={statusTitle}
					switchingContentVisible={state.switchingContentVisible}
				/>
			) : status === "signed_in_same_email" ? (
				<SignedInInviteState
					onContinue={onLogoutAndContinue}
					onCurrentAccount={onCurrentAccount}
					signedInUser={state.signedInUser}
					statusDescription={statusDescription}
					statusTitle={statusTitle}
					submitting={state.submitting}
					t={t}
				/>
			) : status === "signed_in_different_email" ? (
				<SignedInInviteState
					onContinue={onLogoutAndContinue}
					onCurrentAccount={onCurrentAccount}
					reverseActions
					signedInUser={state.signedInUser}
					statusDescription={statusDescription}
					statusTitle={statusTitle}
					submitting={state.submitting}
					t={t}
				/>
			) : (
				<InviteErrorState
					onGoToLogin={onGoToLogin}
					statusDescription={statusDescription}
					statusTitle={statusTitle}
					t={t}
				/>
			)}
		</div>
	);
}

export default function InviteRegisterPage() {
	const { t } = useTranslation(["auth", "core"]);
	const translate = t as Translate;
	const navigate = useNavigate();
	const params = useParams();
	const currentUser = useAuthStore((state) => state.user);
	const logout = useAuthStore((state) => state.logout);
	const initialUserRef = useRef(currentUser);
	const token = useMemo(() => normalizeToken(params.token), [params.token]);
	const timersRef = useRef<InviteTimers | null>(null);
	if (timersRef.current === null) {
		timersRef.current = new Map();
	}
	const timers = timersRef.current;
	const [state, dispatch] = useReducer(
		inviteRegisterReducer,
		token,
		createInitialInviteRegisterState,
	);

	usePageTitle(translate("invitation_page_title"));

	useEffect(() => {
		let canceled = false;

		if (!token) {
			dispatch({ type: "missingToken" });
			return () => {
				canceled = true;
			};
		}

		dispatch({ type: "verifyStarted" });
		authService
			.verifyInvitation(token)
			.then((data) => {
				if (canceled) return;
				dispatch({
					type: "verifySucceeded",
					currentUser: initialUserRef.current,
					invitation: data,
				});
				if (!initialUserRef.current) {
					scheduleInviteTimer(
						timers,
						"formReveal",
						() => dispatch({ type: "formRevealFinished" }),
						260,
					);
				}
			})
			.catch((error) => {
				if (canceled) return;
				if (error instanceof ApiError) {
					const nextStatus = statusFromInvitationError(error);
					if (nextStatus) {
						dispatch({ type: "verifyStatusFailed", status: nextStatus });
						return;
					}
				}
				handleApiError(error);
				dispatch({ type: "genericVerifyFailed" });
			});

		return () => {
			canceled = true;
		};
	}, [timers, token]);

	useEffect(() => () => clearInviteTimers(timers), [timers]);

	const statusTitleKey = getStatusTitleKey(state.status);
	const statusTitle = translate(statusTitleKey);
	const statusDescription = getStatusDescription(
		state.status,
		statusTitleKey,
		translate,
	);
	const contentClassName = getContentClassName(state.status);

	const validateUsername = (value: string) => {
		const result = validateUsernameValue(value);
		dispatch({
			type: "usernameErrorSet",
			usernameError: result.message,
		});
		return result.valid;
	};

	const validatePassword = (value: string) => {
		const result = validatePasswordValue(value);
		dispatch({
			type: "passwordErrorSet",
			passwordError: result.message,
		});
		return result.valid;
	};

	const handleUsernameChange = (value: string) => {
		dispatch({
			type: "usernameChanged",
			username: value,
			usernameError: state.usernameError
				? validateUsernameValue(value).message
				: undefined,
		});
	};

	const handlePasswordChange = (value: string) => {
		dispatch({
			type: "passwordChanged",
			password: value,
			passwordError: state.passwordError
				? validatePasswordValue(value).message
				: undefined,
		});
	};

	const handleSubmit = async (event: React.FormEvent) => {
		event.preventDefault();
		if (state.status !== "form" && state.status !== "revealing_form") {
			return;
		}

		const usernameValid = validateUsername(state.username);
		const passwordValid = validatePassword(state.password);
		if (!usernameValid || !passwordValid) {
			return;
		}

		try {
			dispatch({ type: "submitStarted" });
			await authService.acceptInvitation(token, {
				username: state.username.trim(),
				password: state.password,
			});
			navigate("/login?invitation=accepted", { replace: true });
		} catch (error) {
			if (error instanceof ApiError) {
				const nextStatus = statusFromInvitationError(error);
				if (nextStatus) {
					dispatch({ type: "acceptStatusFailed", status: nextStatus });
					return;
				}
			}
			handleApiError(error);
		} finally {
			dispatch({ type: "submitFinished" });
		}
	};

	const handleLogoutAndContinue = async () => {
		const previousStatus = state.status;
		try {
			dispatch({ type: "logoutTransitionStarted" });
			scheduleInviteTimer(
				timers,
				"switchingReveal",
				() => dispatch({ type: "switchingContentRevealed" }),
				140,
			);
			await logout();
			initialUserRef.current = null;
			dispatch({ type: "logoutCompleted" });
			scheduleInviteTimer(
				timers,
				"continueTransition",
				() => dispatch({ type: "logoutTransitionFinished" }),
				420,
			);
		} catch (error) {
			clearInviteTimer(timers, "switchingReveal");
			handleApiError(error);
			dispatch({ type: "logoutFailed", previousStatus });
		}
	};

	return (
		<div className="flex min-h-screen items-center justify-center bg-background p-6">
			<div className="w-full max-w-sm rounded-3xl border bg-card p-6 shadow-sm">
				<InviteRegisterHeader t={translate} />
				<InvitedAccountSummary invitation={state.invitation} t={translate} />
				<InviteRegisterContent
					contentClassName={contentClassName}
					onCurrentAccount={() => navigate("/")}
					onGoToLogin={() => navigate("/login")}
					onLogoutAndContinue={() => void handleLogoutAndContinue()}
					onPasswordChange={handlePasswordChange}
					onSubmit={handleSubmit}
					onTogglePasswordVisibility={() =>
						dispatch({ type: "togglePasswordVisibility" })
					}
					onUsernameChange={handleUsernameChange}
					state={state}
					statusDescription={statusDescription}
					statusTitle={statusTitle}
					t={translate}
				/>

				<p className="mt-8 text-center text-xs text-muted-foreground/50">
					Self-hosted cloud storage
				</p>
			</div>
		</div>
	);
}
