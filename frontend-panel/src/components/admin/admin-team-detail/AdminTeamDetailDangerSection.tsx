import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type { AdminTeamInfo } from "@/types/api";

interface DangerSectionProps {
	archiveConfirmValue: string;
	archiving: boolean;
	detailLoading: boolean;
	managerCount: number;
	ownerCount: number;
	restoring: boolean;
	setArchiveConfirmValue: (value: string) => void;
	team: AdminTeamInfo | null;
	onArchive: () => Promise<void>;
	onRestore: () => Promise<void>;
}

export function AdminTeamDetailDangerSection({
	archiveConfirmValue,
	archiving,
	detailLoading,
	managerCount,
	ownerCount,
	restoring,
	setArchiveConfirmValue,
	team,
	onArchive,
	onRestore,
}: DangerSectionProps) {
	const { t } = useTranslation(["admin", "core", "settings"]);

	return (
		<section className="rounded-2xl bg-destructive/10 p-6">
			<div className="mb-5">
				<h4 className="text-base font-semibold text-foreground">
					{t("team_danger_zone")}
				</h4>
				<p className="mt-1 text-sm text-muted-foreground">
					{t("team_danger_zone_desc")}
				</p>
			</div>
			<div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_320px]">
				<div className="space-y-3">
					<div className="flex items-center justify-between gap-3">
						<span className="text-sm text-muted-foreground">
							{t("team_owner_count")}
						</span>
						<span className="font-medium">{ownerCount}</span>
					</div>
					<div className="flex items-center justify-between gap-3">
						<span className="text-sm text-muted-foreground">
							{t("team_manager_count")}
						</span>
						<span className="font-medium">{managerCount}</span>
					</div>
					<div className="flex items-center justify-between gap-3">
						<span className="text-sm text-muted-foreground">
							{t("team_status")}
						</span>
						<span className="font-medium">
							{team?.archived_at ? t("archived_badge") : t("core:active")}
						</span>
					</div>
					<p className="text-xs text-muted-foreground">
						{t("team_danger_zone_hint")}
					</p>
				</div>
				<div className="space-y-3">
					{team?.archived_at ? (
						<>
							<p className="text-sm text-muted-foreground">
								{t("team_restore_danger_desc")}
							</p>
							<Button
								type="button"
								variant="outline"
								disabled={detailLoading || restoring}
								onClick={() => void onRestore()}
							>
								{restoring ? (
									<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
								) : (
									<Icon name="ArrowCounterClockwise" className="mr-1 size-4" />
								)}
								{t("restore")}
							</Button>
						</>
					) : (
						<>
							<div className="space-y-2">
								<Label htmlFor="admin-team-archive-confirm">
									{t("team_archive_confirm_label")}
								</Label>
								<Input
									id="admin-team-archive-confirm"
									value={archiveConfirmValue}
									placeholder={t("team_archive_confirm_placeholder")}
									onChange={(event) =>
										setArchiveConfirmValue(event.target.value)
									}
									className={ADMIN_CONTROL_HEIGHT_CLASS}
								/>
								<p className="text-xs text-muted-foreground">
									{t("team_archive_confirm_hint", {
										name: team?.name ?? "",
									})}
								</p>
							</div>
							<Button
								type="button"
								variant="destructive"
								disabled={
									detailLoading ||
									archiving ||
									archiveConfirmValue.trim() !== (team?.name ?? "")
								}
								onClick={() => void onArchive()}
							>
								{archiving ? (
									<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
								) : (
									<Icon name="Trash" className="mr-1 size-4" />
								)}
								{t("delete_team")}
							</Button>
						</>
					)}
				</div>
			</div>
		</section>
	);
}
