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
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";

export interface CreateTeamFormState {
	name: string;
	description: string;
	adminIdentifier: string;
	quotaValue: string;
	policyGroupId: string;
}

export interface TeamPolicyGroupOption {
	disabled?: boolean;
	label: string;
	value: string;
}

interface CreateTeamDialogProps {
	form: CreateTeamFormState;
	open: boolean;
	policyGroupOptions: TeamPolicyGroupOption[];
	policyGroupUnavailable: boolean;
	policyGroupsLoading: boolean;
	submitting: boolean;
	onFieldChange: (key: keyof CreateTeamFormState, value: string) => void;
	onOpenChange: (open: boolean) => void;
	onSubmit: (event: FormEvent<HTMLFormElement>) => void;
}

export function CreateTeamDialog({
	form,
	open,
	policyGroupOptions,
	policyGroupUnavailable,
	policyGroupsLoading,
	submitting,
	onFieldChange,
	onOpenChange,
	onSubmit,
}: CreateTeamDialogProps) {
	const { t } = useTranslation(["admin", "core"]);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent keepMounted>
				<form onSubmit={onSubmit}>
					<DialogHeader>
						<DialogTitle>{t("new_team")}</DialogTitle>
						<DialogDescription>{t("create_team_desc")}</DialogDescription>
					</DialogHeader>
					<div className="space-y-4 py-2">
						<div className="space-y-2">
							<Label htmlFor="admin-team-name">{t("core:name")}</Label>
							<Input
								id="admin-team-name"
								value={form.name}
								maxLength={128}
								disabled={submitting}
								onChange={(event) => onFieldChange("name", event.target.value)}
							/>
						</div>
						<div className="space-y-2">
							<Label htmlFor="admin-team-admin">
								{t("team_admin_identifier")}
							</Label>
							<Input
								id="admin-team-admin"
								value={form.adminIdentifier}
								disabled={submitting}
								placeholder={t("team_admin_placeholder")}
								onChange={(event) =>
									onFieldChange("adminIdentifier", event.target.value)
								}
							/>
							<p className="text-xs text-muted-foreground">
								{t("team_admin_identifier_desc")}
							</p>
						</div>
						<div className="space-y-2">
							<Label>{t("team_policy_group")}</Label>
							<Select
								items={policyGroupOptions}
								value={form.policyGroupId}
								onValueChange={(value) =>
									onFieldChange("policyGroupId", value ?? "")
								}
							>
								<SelectTrigger disabled={submitting || policyGroupsLoading}>
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
							{policyGroupUnavailable ? (
								<p className="text-xs text-destructive">
									{t("policy_group_no_assignable_groups")}
								</p>
							) : null}
						</div>
						<div className="space-y-2">
							<Label htmlFor="admin-team-storage-quota">
								{t("team_quota_mb")}
							</Label>
							<Input
								id="admin-team-storage-quota"
								type="number"
								min={0}
								value={form.quotaValue}
								disabled={submitting}
								placeholder={t("team_quota_default_short")}
								onChange={(event) =>
									onFieldChange("quotaValue", event.target.value)
								}
							/>
							<p className="text-xs text-muted-foreground">
								{t("team_quota_create_desc")}
							</p>
						</div>
						<div className="space-y-2">
							<Label htmlFor="admin-team-description">{t("description")}</Label>
							<textarea
								id="admin-team-description"
								aria-label={t("description")}
								value={form.description}
								disabled={submitting}
								rows={4}
								className="min-h-24 w-full rounded-lg border border-input bg-transparent px-3 py-2 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
								onChange={(event) =>
									onFieldChange("description", event.target.value)
								}
							/>
						</div>
					</div>
					<DialogFooter>
						<Button
							type="submit"
							disabled={
								submitting ||
								!form.name.trim() ||
								!form.adminIdentifier.trim() ||
								!form.policyGroupId
							}
						>
							{t("create_team")}
						</Button>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>
	);
}
