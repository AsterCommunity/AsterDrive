import { useTranslation } from "react-i18next";
import {
	ADMIN_TABLE_BADGE_CELL_CLASS,
	ADMIN_TABLE_MONO_TEXT_CLASS,
	ADMIN_TABLE_MUTED_TEXT_CLASS,
	ADMIN_TABLE_STACKED_CELL_CLASS,
	ADMIN_TABLE_TEXT_CELL_CLASS,
	AdminTableShell,
	AdminTable as Table,
	AdminTableBody as TableBody,
	AdminTableCell as TableCell,
	AdminTableHead as TableHead,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_ICON_BUTTON_CLASS } from "@/lib/constants";
import { formatDateAbsolute, formatDateAbsoluteWithOffset } from "@/lib/format";
import { cn } from "@/lib/utils";
import type {
	AdminUserInvitationInfo,
	UserInvitationStatus,
} from "@/types/api";

interface UserInvitationsTableProps {
	invitations: AdminUserInvitationInfo[];
	revokingInvitationId: number | null;
	onCopyLink: (value: string) => void;
	onRevokeInvitation: (invitation: AdminUserInvitationInfo) => void;
}

function getInvitationStatusClass(status: UserInvitationStatus) {
	if (status === "pending") {
		return "border-blue-200 bg-blue-50 text-blue-700 dark:border-blue-900/60 dark:bg-blue-950/40 dark:text-blue-300";
	}
	if (status === "accepted") {
		return "border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900/60 dark:bg-emerald-950/40 dark:text-emerald-300";
	}
	if (status === "revoked") {
		return "border-zinc-200 bg-zinc-50 text-zinc-600 dark:border-zinc-800 dark:bg-zinc-950/40 dark:text-zinc-300";
	}
	return "border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-900/60 dark:bg-amber-950/40 dark:text-amber-300";
}

export function UserInvitationsTable({
	invitations,
	revokingInvitationId,
	onCopyLink,
	onRevokeInvitation,
}: UserInvitationsTableProps) {
	const { t } = useTranslation("admin");

	return (
		<AdminTableShell>
			<Table>
				<TableHeader>
					<TableRow>
						<TableHead className="w-16">{t("id")}</TableHead>
						<TableHead>{t("core:email")}</TableHead>
						<TableHead className="w-32">{t("core:status")}</TableHead>
						<TableHead className="w-44">{t("invitation_expires_at")}</TableHead>
						<TableHead className="w-44">{t("invitation_created_at")}</TableHead>
						<TableHead className="w-32">{t("core:actions")}</TableHead>
					</TableRow>
				</TableHeader>
				<TableBody>
					{invitations.map((invitation) => {
						const invitationUrl = invitation.invitation_url?.trim() ?? "";
						const isPending = invitation.status === "pending";
						const isRevoking = revokingInvitationId === invitation.id;
						const statusKey = `invitation_status_${invitation.status}`;

						return (
							<TableRow key={invitation.id}>
								<TableCell>
									<span className={ADMIN_TABLE_MONO_TEXT_CLASS}>
										{invitation.id}
									</span>
								</TableCell>
								<TableCell>
									<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
										<div className={ADMIN_TABLE_STACKED_CELL_CLASS}>
											<div className="truncate font-medium text-foreground">
												{invitation.email}
											</div>
											{invitation.accepted_user_id ? (
												<div className={ADMIN_TABLE_MUTED_TEXT_CLASS}>
													{t("invitation_accepted_user", {
														id: invitation.accepted_user_id,
													})}
												</div>
											) : null}
										</div>
									</div>
								</TableCell>
								<TableCell>
									<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
										<Badge
											variant="outline"
											className={cn(
												getInvitationStatusClass(invitation.status),
											)}
										>
											{t(statusKey)}
										</Badge>
									</div>
								</TableCell>
								<TableCell>
									<span
										className="text-sm text-muted-foreground"
										title={formatDateAbsoluteWithOffset(invitation.expires_at)}
									>
										{formatDateAbsolute(invitation.expires_at)}
									</span>
								</TableCell>
								<TableCell>
									<span
										className="text-sm text-muted-foreground"
										title={formatDateAbsoluteWithOffset(invitation.created_at)}
									>
										{formatDateAbsolute(invitation.created_at)}
									</span>
								</TableCell>
								<TableCell>
									<div className="flex justify-end gap-1">
										<Button
											variant="ghost"
											size="icon"
											className={ADMIN_ICON_BUTTON_CLASS}
											onClick={() => onCopyLink(invitationUrl)}
											aria-label={t("invitation_copy_link")}
											title={t("invitation_copy_link")}
											disabled={!invitationUrl}
										>
											<Icon name="Copy" className="size-3.5" />
										</Button>
										<Button
											variant="ghost"
											size="icon"
											className={`${ADMIN_ICON_BUTTON_CLASS} text-destructive`}
											onClick={() => onRevokeInvitation(invitation)}
											aria-label={t("revoke_invitation")}
											title={t("revoke_invitation")}
											disabled={!isPending || isRevoking}
										>
											<Icon
												name={isRevoking ? "Spinner" : "X"}
												className={`size-3.5 ${isRevoking ? "animate-spin" : ""}`}
											/>
										</Button>
									</div>
								</TableCell>
							</TableRow>
						);
					})}
				</TableBody>
			</Table>
		</AdminTableShell>
	);
}
