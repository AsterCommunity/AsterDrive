import type { FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type {
	AdminUserInvitationInfo,
	CreateUserInvitationRequest,
} from "@/types/api";

interface InviteUserDialogProps {
	createdInvitation: AdminUserInvitationInfo | null;
	errors: Partial<CreateUserInvitationRequest>;
	form: CreateUserInvitationRequest;
	inviting: boolean;
	open: boolean;
	onCopyLink: (value: string) => void;
	onFieldChange: (
		key: keyof CreateUserInvitationRequest,
		value: string,
	) => void;
	onFieldValidate: (
		field: keyof CreateUserInvitationRequest,
		value: string,
	) => void;
	onOpenChange: (open: boolean) => void;
	onSubmit: (event: FormEvent<HTMLFormElement>) => void;
}

export function InviteUserDialog({
	createdInvitation,
	errors,
	form,
	inviting,
	open,
	onCopyLink,
	onFieldChange,
	onFieldValidate,
	onOpenChange,
	onSubmit,
}: InviteUserDialogProps) {
	const { t } = useTranslation(["admin", "core"]);
	const invitationUrl = createdInvitation?.invitation_url?.trim() ?? "";

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent keepMounted className="sm:max-w-md">
				<form onSubmit={onSubmit} autoComplete="off" className="space-y-4">
					<DialogHeader>
						<DialogTitle>{t("invite_user")}</DialogTitle>
						<DialogDescription>{t("invite_user_desc")}</DialogDescription>
					</DialogHeader>
					<div className="space-y-2">
						<Label htmlFor="invite-user-email">{t("core:email")}</Label>
						<Input
							id="invite-user-email"
							name="admin-invite-user-email"
							type="email"
							value={form.email}
							onChange={(event) => {
								const value = event.target.value;
								onFieldChange("email", value);
								onFieldValidate("email", value.trim());
							}}
							autoComplete="off"
							required
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							aria-invalid={!!errors.email}
						/>
						{errors.email ? (
							<p className="text-xs text-destructive">{errors.email}</p>
						) : null}
					</div>
					{createdInvitation ? (
						<div className="space-y-2 rounded-lg border border-border/70 bg-muted/25 p-3">
							<div className="flex items-center justify-between gap-3">
								<p className="text-sm font-medium">{t("invitation_created")}</p>
								{createdInvitation.mail_queued ? (
									<span className="text-xs text-muted-foreground">
										{t("invitation_mail_queued")}
									</span>
								) : null}
							</div>
							{invitationUrl ? (
								<div className="flex min-w-0 items-center gap-2">
									<Input
										readOnly
										value={invitationUrl}
										className="h-9 min-w-0 font-mono text-xs"
										onFocus={(event) => event.target.select()}
									/>
									<Button
										type="button"
										variant="outline"
										size="icon"
										className="size-9 shrink-0"
										onClick={() => onCopyLink(invitationUrl)}
										aria-label={t("invitation_copy_link")}
										title={t("invitation_copy_link")}
									>
										<Icon name="Copy" className="size-4" />
									</Button>
								</div>
							) : null}
						</div>
					) : null}
					<DialogFooter>
						<Button
							type="button"
							variant="outline"
							onClick={() => onOpenChange(false)}
							disabled={inviting}
						>
							{t("core:cancel")}
						</Button>
						<Button type="submit" disabled={inviting}>
							{inviting ? (
								<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
							) : (
								<Icon name="EnvelopeSimple" className="mr-1 size-4" />
							)}
							{t("send_invitation")}
						</Button>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>
	);
}
