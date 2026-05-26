import { Button } from "@/components/ui/button";
import { Icon, type IconName } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";
import type { MfaMethod } from "@/services/authService";

type Translate = (key: string, options?: Record<string, unknown>) => string;

interface MfaChallengePanelProps {
	code: string;
	emailCodeError: string;
	emailCodeExpiresAt: number | null;
	emailCodeResendAt: number;
	emailCodeSending: boolean;
	emailCodeSent: boolean;
	error: string;
	expired: boolean;
	methods: MfaMethod[];
	remainingSeconds: number;
	selectedMethod: MfaMethod;
	submitting: boolean;
	t: Translate;
	onBack: () => void;
	onCodeChange: (value: string) => void;
	onEmailCodeSend: () => void;
	onMethodChange: (method: MfaMethod) => void;
}

export function MfaChallengePanel({
	code,
	emailCodeError,
	emailCodeExpiresAt,
	emailCodeResendAt,
	emailCodeSending,
	emailCodeSent,
	error,
	expired,
	methods,
	remainingSeconds,
	selectedMethod,
	submitting,
	t,
	onBack,
	onCodeChange,
	onEmailCodeSend,
	onMethodChange,
}: MfaChallengePanelProps) {
	const availableMethods = methods.length > 0 ? methods : [selectedMethod];
	const emailCodeResendSeconds = Math.max(
		0,
		Math.ceil((emailCodeResendAt - Date.now()) / 1000),
	);
	const emailCodeRemainingSeconds = emailCodeExpiresAt
		? Math.max(0, Math.ceil((emailCodeExpiresAt - Date.now()) / 1000))
		: 0;
	const isEmailMethod = selectedMethod === "email_code";
	const canSubmit =
		!submitting &&
		!expired &&
		(isEmailMethod
			? emailCodeSent && /^\d{8}$/.test(code.trim())
			: code.trim().length > 0);
	const codeLabel = mfaCodeLabel(selectedMethod, t);
	const codePlaceholder = mfaCodePlaceholder(selectedMethod, t);
	const codeInputMode = selectedMethod === "recovery_code" ? "text" : "numeric";

	const handleCodeChange = (value: string) => {
		if (selectedMethod === "email_code") {
			onCodeChange(value.replace(/\D/g, "").slice(0, 8));
			return;
		}
		if (selectedMethod === "totp") {
			onCodeChange(value.replace(/\D/g, "").slice(0, 6));
			return;
		}
		onCodeChange(value);
	};

	return (
		<div className="space-y-4 rounded-2xl border bg-muted/20 p-4 transition-[background-color,border-color] duration-200">
			<div className="flex items-start gap-3">
				<div className="rounded-xl bg-primary/10 p-2 text-primary">
					<Icon name="Shield" className="size-5" />
				</div>
				<div className="min-w-0 space-y-1">
					<p className="text-sm font-medium">{t("mfa_panel_title")}</p>
					<p className="text-sm text-muted-foreground">
						{expired
							? t("mfa_flow_expired")
							: t("mfa_flow_remaining", { seconds: remainingSeconds })}
					</p>
				</div>
			</div>

			{availableMethods.length > 1 ? (
				<div
					className={cn(
						"grid gap-1 rounded-lg bg-muted p-1",
						availableMethods.length === 2 ? "grid-cols-2" : "grid-cols-3",
					)}
				>
					{availableMethods.map((method) => (
						<button
							key={method}
							type="button"
							className={cn(
								"inline-flex h-9 min-w-0 items-center justify-center gap-1.5 rounded-md px-2 text-xs font-medium text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50",
								method === selectedMethod &&
									"bg-background text-foreground shadow-sm",
							)}
							aria-pressed={method === selectedMethod}
							disabled={submitting || emailCodeSending}
							onClick={() => onMethodChange(method)}
						>
							<Icon name={mfaMethodIcon(method)} className="size-4 shrink-0" />
							<span className="truncate">{mfaMethodLabel(method, t)}</span>
						</button>
					))}
				</div>
			) : null}

			{isEmailMethod ? (
				<div className="space-y-2 rounded-lg border bg-background/60 p-3">
					<div className="flex items-start gap-2 text-sm text-muted-foreground">
						<Icon name="EnvelopeSimple" className="mt-0.5 size-4 shrink-0" />
						<p className="min-w-0">
							{emailCodeSent
								? emailCodeRemainingSeconds > 0
									? t("mfa_email_code_sent_remaining", {
											seconds: emailCodeRemainingSeconds,
										})
									: t("mfa_email_code_sent")
								: t("mfa_email_code_hint")}
						</p>
					</div>
					<Button
						type="button"
						variant="outline"
						className="h-9 w-full"
						disabled={
							submitting ||
							expired ||
							emailCodeSending ||
							emailCodeResendSeconds > 0
						}
						onClick={onEmailCodeSend}
					>
						{emailCodeSending ? (
							<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
						) : (
							<Icon name="EnvelopeSimple" className="mr-2 size-4" />
						)}
						{emailCodeSending
							? t("mfa_email_code_sending")
							: emailCodeResendSeconds > 0
								? t("mfa_email_code_resend_in", {
										seconds: emailCodeResendSeconds,
									})
								: emailCodeSent
									? t("mfa_email_code_resend")
									: t("mfa_email_code_send")}
					</Button>
					{emailCodeError ? (
						<p className="text-xs text-destructive">{emailCodeError}</p>
					) : null}
				</div>
			) : null}

			<div className="space-y-1.5">
				<Label htmlFor="mfa-code" className="text-sm">
					{codeLabel}
				</Label>
				<Input
					id="mfa-code"
					value={code}
					disabled={submitting || expired || (isEmailMethod && !emailCodeSent)}
					autoComplete="one-time-code"
					autoFocus
					inputMode={codeInputMode}
					placeholder={codePlaceholder}
					className={
						error ? "border-destructive focus-visible:ring-destructive" : ""
					}
					onChange={(event) => handleCodeChange(event.target.value)}
				/>
				{error ? <p className="text-xs text-destructive">{error}</p> : null}
			</div>

			<Button type="submit" className="h-10 w-full" disabled={!canSubmit}>
				{submitting ? (
					<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
				) : (
					<Icon name="SignIn" className="mr-2 size-4" />
				)}
				{submitting ? t("mfa_verifying") : t("mfa_verify")}
			</Button>

			<Button
				type="button"
				variant="ghost"
				className="h-9 w-full"
				disabled={submitting}
				onClick={onBack}
			>
				<Icon name="ArrowLeft" className="mr-2 size-4" />
				{t("back_to_sign_in")}
			</Button>
		</div>
	);
}

function mfaMethodLabel(method: MfaMethod, t: Translate) {
	switch (method) {
		case "totp":
			return t("mfa_method_totp");
		case "recovery_code":
			return t("mfa_method_recovery_code");
		case "email_code":
			return t("mfa_method_email_code");
	}
}

function mfaMethodIcon(method: MfaMethod): IconName {
	switch (method) {
		case "totp":
			return "Shield";
		case "recovery_code":
			return "Key";
		case "email_code":
			return "EnvelopeSimple";
	}
}

function mfaCodeLabel(method: MfaMethod, t: Translate) {
	switch (method) {
		case "totp":
			return t("mfa_totp_code_label");
		case "recovery_code":
			return t("mfa_recovery_code_label");
		case "email_code":
			return t("mfa_email_code_label");
	}
}

function mfaCodePlaceholder(method: MfaMethod, t: Translate) {
	switch (method) {
		case "totp":
			return t("mfa_totp_code_placeholder");
		case "recovery_code":
			return t("mfa_recovery_code_placeholder");
		case "email_code":
			return t("mfa_email_code_placeholder");
	}
}
