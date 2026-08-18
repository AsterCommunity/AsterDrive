import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";

type Translate = (
	key: string,
	values?: Record<string, number | string>,
) => string;

export interface PendingActivationState {
	email?: string;
	identifier: string;
	username?: string;
}

interface PendingActivationPanelProps {
	pendingActivation: PendingActivationState;
	resendingActivation: boolean;
	t: Translate;
	onResendActivation: () => void;
	onReset: () => void;
}

export function PendingActivationPanel({
	pendingActivation,
	resendingActivation,
	t,
	onResendActivation,
	onReset,
}: PendingActivationPanelProps) {
	return (
		<div className="space-y-4">
			<div className="space-y-1">
				<p className="text-sm text-muted-foreground">
					{t("activation_pending_hint")}
				</p>
				{pendingActivation.username ? (
					<p className="text-xs text-muted-foreground">
						{t("core:username")}: {pendingActivation.username}
					</p>
				) : null}
				{pendingActivation.email ? (
					<p className="text-xs text-muted-foreground">
						{t("core:email")}: {pendingActivation.email}
					</p>
				) : null}
			</div>

			<div className="grid gap-2 sm:grid-cols-2">
				<Button
					type="button"
					className="h-10"
					disabled={resendingActivation}
					onClick={onResendActivation}
				>
					{resendingActivation ? (
						<Icon name="Spinner" className="mr-2 size-4 animate-spin" />
					) : (
						<Icon name="ArrowClockwise" className="mr-2 size-4" />
					)}
					{resendingActivation
						? t("resending_activation")
						: t("resend_activation")}
				</Button>
				<Button
					type="button"
					variant="outline"
					className="h-10"
					onClick={onReset}
				>
					<Icon name="ArrowLeft" className="mr-2 size-4" />
					{t("not_you")}
				</Button>
			</div>
		</div>
	);
}
