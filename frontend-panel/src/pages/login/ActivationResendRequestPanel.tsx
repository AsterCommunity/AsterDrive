import type { ZodType } from "zod/v4";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";

type Translate = (key: string) => string;

interface ActivationResendRequestPanelProps {
	email: string;
	emailError: string;
	emailSchema: ZodType;
	requesting: boolean;
	t: Translate;
	onBack: () => void;
	onEmailChange: (value: string, error: string) => void;
	onSubmit: () => void;
}

export function ActivationResendRequestPanel({
	email,
	emailError,
	emailSchema,
	requesting,
	t,
	onBack,
	onEmailChange,
	onSubmit,
}: ActivationResendRequestPanelProps) {
	const emailErrorId = "activation-resend-email-error";

	return (
		<div className="space-y-4 rounded-2xl border bg-muted/20 p-4">
			<div className="flex items-start gap-3">
				<div className="rounded-xl bg-primary/10 p-2 text-primary">
					<Icon name="EnvelopeSimple" className="size-5" />
				</div>
				<div className="space-y-1">
					<p className="text-sm font-medium">{t("activation_resend_title")}</p>
					<p className="text-sm text-muted-foreground">
						{t("activation_resend_hint")}
					</p>
				</div>
			</div>

			<div className="space-y-1.5">
				<Label htmlFor="activation-resend-email" className="text-sm">
					{t("core:email")}
				</Label>
				<Input
					id="activation-resend-email"
					placeholder="you@example.com"
					value={email}
					aria-describedby={emailError ? emailErrorId : undefined}
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
						emailError && "border-destructive focus-visible:ring-destructive",
					)}
				/>
				{emailError ? (
					<p
						id={emailErrorId}
						role="alert"
						className="text-xs text-destructive"
					>
						{emailError}
					</p>
				) : null}
			</div>

			<div className="grid gap-2 sm:grid-cols-2">
				<Button
					type="button"
					className="h-10"
					disabled={requesting || email.trim().length === 0 || !!emailError}
					onClick={onSubmit}
				>
					{requesting ? (
						<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
					) : (
						<Icon name="EnvelopeSimple" className="mr-2 size-4" />
					)}
					{requesting ? t("resending_activation") : t("resend_activation")}
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
