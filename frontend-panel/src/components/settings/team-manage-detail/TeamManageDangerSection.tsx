import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type { TeamInfo } from "@/types/api";

interface DangerSectionProps {
	archiveConfirmValue: string;
	managerCount: number;
	mutating: boolean;
	ownerCount: number;
	setArchiveConfirmValue: (value: string) => void;
	team: TeamInfo | null;
	onArchive: () => Promise<void>;
}

export function TeamManageDangerSection({
	archiveConfirmValue,
	managerCount,
	mutating,
	ownerCount,
	setArchiveConfirmValue,
	team,
	onArchive,
}: DangerSectionProps) {
	const { t } = useTranslation(["core", "settings"]);

	return (
		<section className="rounded-2xl bg-destructive/10 p-6">
			<div className="mb-5">
				<h4 className="text-base font-semibold text-foreground">
					{t("settings:settings_team_danger_zone")}
				</h4>
				<p className="mt-1 text-sm text-muted-foreground">
					{t("settings:settings_team_danger_zone_desc")}
				</p>
			</div>
			<div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_320px]">
				<div className="space-y-3">
					<div className="flex items-center justify-between gap-3">
						<span className="text-sm text-muted-foreground">
							{t("settings:settings_team_owner_count")}
						</span>
						<span className="font-medium">{ownerCount}</span>
					</div>
					<div className="flex items-center justify-between gap-3">
						<span className="text-sm text-muted-foreground">
							{t("settings:settings_team_manager_count")}
						</span>
						<span className="font-medium">{managerCount}</span>
					</div>
					<div className="flex items-center justify-between gap-3">
						<span className="text-sm text-muted-foreground">
							{t("settings:settings_team_status")}
						</span>
						<span className="font-medium">{t("core:active")}</span>
					</div>
					<p className="text-xs text-muted-foreground">
						{t("settings:settings_team_danger_zone_hint")}
					</p>
				</div>
				<div className="space-y-3">
					<div className="space-y-2">
						<Label htmlFor="team-archive-confirm">
							{t("settings:settings_team_archive_confirm_label")}
						</Label>
						<Input
							id="team-archive-confirm"
							value={archiveConfirmValue}
							placeholder={t(
								"settings:settings_team_archive_confirm_placeholder",
							)}
							onChange={(event) => setArchiveConfirmValue(event.target.value)}
							className={ADMIN_CONTROL_HEIGHT_CLASS}
						/>
						<p className="text-xs text-muted-foreground">
							{t("settings:settings_team_archive_confirm_hint", {
								name: team?.name ?? "",
							})}
						</p>
					</div>
					<Button
						type="button"
						variant="destructive"
						disabled={
							mutating || archiveConfirmValue.trim() !== (team?.name ?? "")
						}
						onClick={() => void onArchive()}
					>
						{t("settings:settings_team_archive")}
					</Button>
				</div>
			</div>
		</section>
	);
}
