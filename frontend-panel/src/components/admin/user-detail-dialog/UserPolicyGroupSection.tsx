import { useTranslation } from "react-i18next";
import { SkeletonTable } from "@/components/common/SkeletonTable";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type { PolicyGroupOption } from "./types";

interface UserPolicyGroupSectionProps {
	assignedPolicyGroupIsInvalid: boolean;
	draftPolicyGroupId: number | null;
	onDraftPolicyGroupIdChange: (value: number) => void;
	onRefreshPolicyGroups: () => Promise<void>;
	policyGroupOptions: PolicyGroupOption[];
	policyGroupsLoading: boolean;
	savingProfile: boolean;
}

export function UserPolicyGroupSection({
	assignedPolicyGroupIsInvalid,
	draftPolicyGroupId,
	onDraftPolicyGroupIdChange,
	onRefreshPolicyGroups,
	policyGroupOptions,
	policyGroupsLoading,
	savingProfile,
}: UserPolicyGroupSectionProps) {
	const { t } = useTranslation(["admin", "core"]);

	return (
		<section>
			<div className="mb-4 flex items-start justify-between gap-3">
				<div>
					<h4 className="text-base font-semibold text-foreground">
						{t("storage_policy_assignments")}
					</h4>
					<p className="mt-1 text-sm text-muted-foreground">
						{t("storage_policy_assignments_desc")}
					</p>
				</div>
				<Button
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

			{policyGroupsLoading ? (
				<SkeletonTable columns={2} rows={3} />
			) : (
				<div className="space-y-3">
					<div className="space-y-3">
						<div className="space-y-2">
							<Label>{t("policy_groups")}</Label>
							<Select
								items={policyGroupOptions}
								value={
									draftPolicyGroupId != null ? String(draftPolicyGroupId) : ""
								}
								onValueChange={(value) => {
									if (!value) {
										// The API does not allow unassigning a policy group.
										return;
									}
									onDraftPolicyGroupIdChange(Number(value));
								}}
								disabled={savingProfile || policyGroupOptions.length === 0}
							>
								<SelectTrigger>
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
						</div>

						{assignedPolicyGroupIsInvalid ? (
							<div className="rounded-lg bg-amber-500/10 px-3 py-2 text-sm text-amber-700 dark:text-amber-300">
								{t("policy_group_invalid_assignment")}
							</div>
						) : null}

						{policyGroupOptions.length === 0 ? (
							<p className="text-sm text-muted-foreground">
								{t("policy_group_no_assignable_groups")}
							</p>
						) : null}
					</div>
				</div>
			)}
		</section>
	);
}
