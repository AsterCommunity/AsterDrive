import type { ZodType } from "zod/v4";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";

type Translate = (key: string) => string;

interface PasswordResetRequestPanelProps {
	emailSchema: ZodType;
	passwordResetEmail: string;
	passwordResetError: string;
	requestingPasswordReset: boolean;
	t: Translate;
	onBack: () => void;
	onEmailChange: (value: string, error: string) => void;
	onSubmit: () => void;
}

export function PasswordResetRequestPanel({
	emailSchema,
	passwordResetEmail,
	passwordResetError,
	requestingPasswordReset,
	t,
	onBack,
	onEmailChange,
	onSubmit,
}: PasswordResetRequestPanelProps) {
	return (
		<div className="space-y-4">
			<p className="text-sm text-muted-foreground">
				{t("password_reset_request_hint")}
			</p>

			<div className="space-y-1.5">
				<Label htmlFor="password-reset-email" className="text-sm">
					{t("core:email")}
				</Label>
				<Input
					id="password-reset-email"
					placeholder="you@example.com"
					value={passwordResetEmail}
					onChange={(event) => {
						const nextValue = event.target.value;
						const result = emailSchema.safeParse(nextValue);
						onEmailChange(
							nextValue,
							result.success ? "" : (result.error.issues[0]?.message ?? ""),
						);
					}}
					autoFocus
					autoComplete="email"
					className={cn(
						"h-10",
						passwordResetError &&
							"border-destructive focus-visible:ring-destructive",
					)}
				/>
				{passwordResetError ? (
					<p className="text-xs text-destructive">{passwordResetError}</p>
				) : null}
			</div>

			<div className="grid gap-2 sm:grid-cols-2">
				<Button
					type="button"
					className="h-10"
					disabled={
						requestingPasswordReset || passwordResetEmail.trim().length === 0
					}
					onClick={onSubmit}
				>
					{requestingPasswordReset ? (
						<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
					) : (
						<Icon name="EnvelopeSimple" className="mr-2 size-4" />
					)}
					{requestingPasswordReset
						? t("sending_password_reset")
						: t("send_password_reset")}
				</Button>
				<Button
					type="button"
					variant="outline"
					className="h-10"
					onClick={onBack}
				>
					<Icon name="ArrowLeft" className="mr-2 size-4" />
					{t("back_to_sign_in")}
				</Button>
			</div>
		</div>
	);
}
