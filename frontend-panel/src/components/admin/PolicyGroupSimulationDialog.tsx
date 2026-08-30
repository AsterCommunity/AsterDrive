import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
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
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { formatBytes } from "@/lib/format";
import type {
	StoragePlacementSimulationResult,
	StoragePolicyGroup,
} from "@/types/api";

const NO_FOLDER_OVERRIDE = "__none__";

interface PolicyGroupSimulationDialogProps {
	open: boolean;
	error: string | null;
	filename: string;
	fileSizeMb: string;
	folderPolicyId: string;
	group: StoragePolicyGroup | null;
	mimeType: string;
	policies: Array<{ id: number; name: string }>;
	result: StoragePlacementSimulationResult | null;
	submitting: boolean;
	onFilenameChange: (value: string) => void;
	onFileSizeMbChange: (value: string) => void;
	onFolderPolicyIdChange: (value: string) => void;
	onMimeTypeChange: (value: string) => void;
	onOpenChange: (open: boolean) => void;
	onSimulate: () => void;
}

function policyName(
	policyId: number,
	group: StoragePolicyGroup | null,
	policies: Array<{ id: number; name: string }>,
) {
	return (
		policies.find((policy) => policy.id === policyId)?.name ??
		group?.rules
			.flatMap((rule) => rule.targets)
			.find((target) => target.policy_id === policyId)?.policy.name ??
		`#${policyId}`
	);
}

export function PolicyGroupSimulationDialog({
	open,
	error,
	filename,
	fileSizeMb,
	folderPolicyId,
	group,
	mimeType,
	policies,
	result,
	submitting,
	onFilenameChange,
	onFileSizeMbChange,
	onFolderPolicyIdChange,
	onMimeTypeChange,
	onOpenChange,
	onSimulate,
}: PolicyGroupSimulationDialogProps) {
	const { t } = useTranslation("admin");
	const folderPolicyOptions = [
		{
			label: t("policy_group_simulator_no_folder_override"),
			value: NO_FOLDER_OVERRIDE,
		},
		...policies.map((policy) => ({
			label: policy.name,
			value: String(policy.id),
		})),
	];
	const excludedTargets =
		result?.excluded_targets.filter(
			([policyId, reason], index, values) =>
				values.findIndex(
					([candidatePolicyId, candidateReason]) =>
						candidatePolicyId === policyId && candidateReason === reason,
				) === index,
		) ?? [];

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-3xl">
				<DialogHeader>
					<DialogTitle>{t("policy_group_simulator_title")}</DialogTitle>
					<DialogDescription>
						{group?.name ?? t("policy_group_simulator_group_unavailable")}
					</DialogDescription>
				</DialogHeader>

				<div className="space-y-5">
					<div className="grid gap-4 sm:grid-cols-2">
						<div className="space-y-2">
							<Label htmlFor="policy-group-simulator-filename">
								{t("policy_group_simulator_filename")}
							</Label>
							<Input
								id="policy-group-simulator-filename"
								value={filename}
								onChange={(event) => onFilenameChange(event.target.value)}
								className={ADMIN_CONTROL_HEIGHT_CLASS}
							/>
						</div>
						<div className="space-y-2">
							<Label htmlFor="policy-group-simulator-size">
								{t("policy_group_simulator_size_mb")}
							</Label>
							<Input
								id="policy-group-simulator-size"
								type="number"
								min="0"
								step="any"
								value={fileSizeMb}
								onChange={(event) => onFileSizeMbChange(event.target.value)}
								className={ADMIN_CONTROL_HEIGHT_CLASS}
							/>
						</div>
						<div className="space-y-2">
							<Label htmlFor="policy-group-simulator-mime">
								{t("policy_group_simulator_mime_type")}
							</Label>
							<Input
								id="policy-group-simulator-mime"
								value={mimeType}
								onChange={(event) => onMimeTypeChange(event.target.value)}
								className={ADMIN_CONTROL_HEIGHT_CLASS}
							/>
						</div>
						<div className="space-y-2">
							<Label>{t("policy_group_simulator_folder_override")}</Label>
							<Select
								items={folderPolicyOptions}
								value={folderPolicyId || NO_FOLDER_OVERRIDE}
								onValueChange={(value) =>
									onFolderPolicyIdChange(
										value === NO_FOLDER_OVERRIDE ? "" : (value ?? ""),
									)
								}
							>
								<SelectTrigger
									className={`${ADMIN_CONTROL_HEIGHT_CLASS} w-full`}
								>
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									{folderPolicyOptions.map((option) => (
										<SelectItem key={option.value} value={option.value}>
											{option.label}
										</SelectItem>
									))}
								</SelectContent>
							</Select>
						</div>
					</div>

					{error ? (
						<div className="border-l-2 border-destructive pl-3 text-sm text-destructive">
							{error}
						</div>
					) : null}

					{result ? (
						<div className="space-y-5 border-t pt-5" aria-live="polite">
							<div className="flex flex-wrap items-center gap-2">
								<Badge variant={result.decision ? "default" : "destructive"}>
									{result.decision
										? t("policy_group_simulator_selected")
										: t("policy_group_simulator_rejected")}
								</Badge>
								<Badge variant="outline">
									{result.admitted
										? t("policy_group_simulator_admission_passed")
										: t("policy_group_simulator_admission_rejected")}
								</Badge>
								{result.rejection_code ? (
									<code className="text-xs text-muted-foreground">
										{result.rejection_code}
									</code>
								) : null}
							</div>

							<div className="grid gap-x-6 gap-y-3 text-sm sm:grid-cols-2 lg:grid-cols-3">
								<div>
									<div className="text-xs text-muted-foreground">
										{t("policy_group_simulator_classification")}
									</div>
									<div className="font-medium">
										{result.classification.category}
									</div>
								</div>
								<div>
									<div className="text-xs text-muted-foreground">
										{t("policy_group_simulator_extension")}
									</div>
									<div className="font-medium">
										{result.classification.compound_extension ||
											result.classification.extension ||
											t("policy_group_simulator_extensionless")}
									</div>
								</div>
								<div>
									<div className="text-xs text-muted-foreground">
										{t("policy_group_simulator_normalized_size")}
									</div>
									<div className="font-medium">
										{formatBytes(result.classification.file_size)}
									</div>
								</div>
								{result.decision ? (
									<>
										<div>
											<div className="text-xs text-muted-foreground">
												{t("policy_group_simulator_selected_policy")}
											</div>
											<div className="font-medium">
												{policyName(result.decision.policy_id, group, policies)}
											</div>
										</div>
										<div>
											<div className="text-xs text-muted-foreground">
												{t("policy_group_simulator_selection_mode")}
											</div>
											<div className="font-medium">
												{t(
													`policy_group_selection_${result.decision.selection_mode}`,
												)}
											</div>
										</div>
										<div>
											<div className="text-xs text-muted-foreground">
												{t("policy_group_simulator_revision")}
											</div>
											<div className="font-medium">
												{result.decision.revision}
											</div>
										</div>
									</>
								) : null}
							</div>

							<div className="grid gap-5 border-t pt-5 md:grid-cols-2">
								<div className="space-y-2">
									<h3 className="text-sm font-semibold">
										{t("policy_group_simulator_evaluated_rules")}
									</h3>
									{result.evaluated_rules.length === 0 ? (
										<p className="text-sm text-muted-foreground">
											{t("policy_group_simulator_no_evaluated_rules")}
										</p>
									) : (
										<ul className="divide-y text-sm">
											{result.evaluated_rules.map((evaluation) => {
												const rule = group?.rules.find(
													(candidate) => candidate.id === evaluation.rule_id,
												);
												return (
													<li
														key={evaluation.rule_id}
														className="flex items-center justify-between gap-3 py-2"
													>
														<span>
															{rule?.name || `#${evaluation.rule_id}`}
														</span>
														<code className="text-xs text-muted-foreground">
															{evaluation.matched
																? t("policy_group_simulator_rule_matched")
																: (evaluation.reason_code ?? "-")}
														</code>
													</li>
												);
											})}
										</ul>
									)}
								</div>

								<div className="space-y-2">
									<h3 className="text-sm font-semibold">
										{t("policy_group_simulator_excluded_targets")}
									</h3>
									{excludedTargets.length === 0 ? (
										<p className="text-sm text-muted-foreground">
											{t("policy_group_simulator_no_excluded_targets")}
										</p>
									) : (
										<ul className="divide-y text-sm">
											{excludedTargets.map(([policyId, reason]) => (
												<li
													key={`${policyId}-${reason}`}
													className="flex items-center justify-between gap-3 py-2"
												>
													<span>{policyName(policyId, group, policies)}</span>
													<code className="text-xs text-muted-foreground">
														{reason}
													</code>
												</li>
											))}
										</ul>
									)}
								</div>
							</div>
						</div>
					) : null}
				</div>

				<DialogFooter className="gap-2">
					<Button
						type="button"
						variant="outline"
						onClick={() => onOpenChange(false)}
					>
						{t("core:cancel")}
					</Button>
					<Button
						type="button"
						onClick={onSimulate}
						disabled={submitting || !group}
					>
						<Icon
							name={submitting ? "Spinner" : "Play"}
							className={`mr-1 size-4 ${submitting ? "animate-spin" : ""}`}
						/>
						{submitting
							? t("policy_group_simulator_running")
							: t("policy_group_simulator_run")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
