import type { FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { SkeletonTable } from "@/components/common/SkeletonTable";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type { TeamInfo } from "@/types/api";

interface OverviewSectionProps {
	canManageTeam: boolean;
	detailLoading: boolean;
	mutating: boolean;
	onDescriptionChange: (value: string) => void;
	onSubmit: (event: FormEvent<HTMLFormElement>) => void;
	onTeamNameChange: (value: string) => void;
	team: TeamInfo | null;
	teamDescription: string;
	teamName: string;
}

export function TeamManageOverviewSection({
	canManageTeam,
	detailLoading,
	mutating,
	onDescriptionChange,
	onSubmit,
	onTeamNameChange,
	team,
	teamDescription,
	teamName,
}: OverviewSectionProps) {
	const { t } = useTranslation(["core", "settings"]);

	return (
		<section className="rounded-2xl border bg-background/60 p-6">
			<div className="mb-5">
				<h4 className="text-base font-semibold text-foreground">
					{t("settings:settings_team_details")}
				</h4>
				<p className="mt-1 text-sm text-muted-foreground">
					{t("settings:settings_team_details_desc")}
				</p>
			</div>
			{detailLoading && !team ? (
				<SkeletonTable columns={2} rows={4} />
			) : (
				<form className="space-y-4" onSubmit={onSubmit}>
					<div className="space-y-2">
						<Label htmlFor="team-manage-name">{t("core:name")}</Label>
						<Input
							id="team-manage-name"
							value={teamName}
							maxLength={128}
							readOnly={!canManageTeam}
							disabled={mutating || detailLoading}
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							onChange={(event) => onTeamNameChange(event.target.value)}
						/>
					</div>
					<div className="space-y-2">
						<Label htmlFor="team-manage-description">
							{t("settings:settings_team_description")}
						</Label>
						<textarea
							id="team-manage-description"
							aria-label={t("settings:settings_team_description")}
							value={teamDescription}
							readOnly={!canManageTeam}
							disabled={mutating || detailLoading}
							rows={5}
							className="min-h-28 w-full rounded-lg border border-input bg-transparent px-3 py-2 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:bg-input/50"
							onChange={(event) => onDescriptionChange(event.target.value)}
						/>
					</div>
					<div className="flex flex-wrap items-center justify-between gap-3 border-t pt-4">
						<p className="text-xs text-muted-foreground">
							{detailLoading
								? t("core:loading")
								: t("settings:settings_team_dialog_hint")}
						</p>
						{canManageTeam ? (
							<Button
								type="submit"
								disabled={mutating || detailLoading || !teamName.trim()}
							>
								{t("core:save")}
							</Button>
						) : null}
					</div>
				</form>
			)}
		</section>
	);
}
