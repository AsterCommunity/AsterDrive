import { useTranslation } from "react-i18next";
import { SkeletonTable } from "@/components/common/SkeletonTable";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type { AdminTeamInfo } from "@/types/api";
import type { PolicyGroupOption } from "./AdminTeamDetailSectionTypes";

interface OverviewSectionProps {
	archiving: boolean;
	assignedPolicyGroupIsInvalid: boolean;
	canMutateTeam: boolean;
	description: string;
	detailLoading: boolean;
	hasChanges: boolean;
	name: string;
	onDescriptionChange: (value: string) => void;
	onNameChange: (value: string) => void;
	onPolicyGroupChange: (value: string) => void;
	onRefreshPolicyGroups: () => Promise<void>;
	onSave: () => Promise<void>;
	policyGroupId: string;
	policyGroupOptions: PolicyGroupOption[];
	policyGroupUnavailable: boolean;
	policyGroupsLoading: boolean;
	restoring: boolean;
	saving: boolean;
	team: AdminTeamInfo | null;
}

export function AdminTeamDetailOverviewSection({
	archiving,
	assignedPolicyGroupIsInvalid,
	canMutateTeam,
	description,
	detailLoading,
	hasChanges,
	name,
	onDescriptionChange,
	onNameChange,
	onPolicyGroupChange,
	onRefreshPolicyGroups,
	onSave,
	policyGroupId,
	policyGroupOptions,
	policyGroupUnavailable,
	policyGroupsLoading,
	restoring,
	saving,
	team,
}: OverviewSectionProps) {
	const { t } = useTranslation(["admin", "core", "settings"]);
	const formDisabled = saving || archiving || restoring || !canMutateTeam;

	return (
		<section className="rounded-2xl border bg-background/60 p-6">
			<div className="mb-5 flex items-start justify-between gap-3">
				<div>
					<h4 className="text-base font-semibold text-foreground">
						{t("edit_team")}
					</h4>
					<p className="mt-1 text-sm text-muted-foreground">
						{t("team_details_desc")}
					</p>
				</div>
				<Button
					type="button"
					variant="ghost"
					size="sm"
					className={ADMIN_CONTROL_HEIGHT_CLASS}
					onClick={() => void onRefreshPolicyGroups()}
					disabled={policyGroupsLoading}
				>
					<Icon
						name={policyGroupsLoading ? "Spinner" : "ArrowsClockwise"}
						className={`mr-1 size-3.5 ${policyGroupsLoading ? "animate-spin" : ""}`}
					/>
					{t("core:refresh")}
				</Button>
			</div>
			{detailLoading && !team ? (
				<SkeletonTable columns={2} rows={4} />
			) : (
				<form
					className="space-y-4"
					onSubmit={(event) => {
						event.preventDefault();
						void onSave();
					}}
				>
					<div className="grid gap-5 md:grid-cols-2">
						<div className="space-y-2 md:col-span-2">
							<Label htmlFor="admin-team-detail-name">{t("core:name")}</Label>
							<Input
								id="admin-team-detail-name"
								value={name}
								maxLength={128}
								disabled={formDisabled}
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onChange={(event) => onNameChange(event.target.value)}
							/>
						</div>
						<div className="space-y-2 md:col-span-2">
							<Label>{t("team_policy_group")}</Label>
							<Select
								items={policyGroupOptions}
								value={policyGroupId}
								onValueChange={(value) => onPolicyGroupChange(value ?? "")}
							>
								<SelectTrigger disabled={formDisabled || policyGroupsLoading}>
									<SelectValue placeholder={t("select_policy_group")} />
								</SelectTrigger>
								<SelectContent>
									{policyGroupOptions.map((option) => (
										<SelectItem
											key={option.value}
											value={option.value}
											disabled={option.disabled}
										>
											{option.label}
										</SelectItem>
									))}
								</SelectContent>
							</Select>
							<p className="text-xs text-muted-foreground">
								{t("team_policy_group_desc")}
							</p>
							{assignedPolicyGroupIsInvalid ? (
								<p className="text-xs text-destructive">
									{t("policy_group_invalid_assignment")}
								</p>
							) : null}
							{policyGroupUnavailable ? (
								<p className="text-xs text-destructive">
									{t("policy_group_no_assignable_groups")}
								</p>
							) : null}
						</div>
						<div className="space-y-2 md:col-span-2">
							<Label htmlFor="admin-team-detail-description">
								{t("description")}
							</Label>
							<textarea
								id="admin-team-detail-description"
								aria-label={t("description")}
								value={description}
								disabled={formDisabled}
								rows={6}
								className="min-h-32 w-full rounded-lg border border-input bg-transparent px-3 py-2 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:bg-input/50"
								onChange={(event) => onDescriptionChange(event.target.value)}
							/>
						</div>
					</div>
					<div className="flex items-center justify-end gap-2 border-t pt-4">
						<Button
							type="submit"
							disabled={
								formDisabled || !name.trim() || !policyGroupId || !hasChanges
							}
						>
							{saving ? (
								<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
							) : (
								<Icon name="FloppyDisk" className="mr-1 size-4" />
							)}
							{t("save_changes")}
						</Button>
					</div>
				</form>
			)}
		</section>
	);
}
