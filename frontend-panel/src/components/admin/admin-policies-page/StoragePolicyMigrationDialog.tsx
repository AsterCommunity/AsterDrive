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
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { formatBytes } from "@/lib/format";
import type { StoragePolicy, StoragePolicyMigrationDryRun } from "@/types/api";

interface StoragePolicyMigrationDialogProps {
	dryRun: StoragePolicyMigrationDryRun | null;
	dryRunLoading: boolean;
	open: boolean;
	policies: StoragePolicy[];
	sourcePolicyId: string;
	submitting: boolean;
	targetPolicyId: string;
	onOpenChange: (open: boolean) => void;
	onDryRun: () => void;
	onSourcePolicyChange: (policyId: string) => void;
	onSubmit: () => void;
	onTargetPolicyChange: (policyId: string) => void;
}

function policyOptionLabel(policy: StoragePolicy) {
	return `#${policy.id} · ${policy.name}`;
}

function selectedPolicyLabel(policies: StoragePolicy[], policyId: string) {
	const policy = policies.find((item) => String(item.id) === policyId);
	return policy ? policyOptionLabel(policy) : undefined;
}

export function StoragePolicyMigrationDialog({
	dryRun,
	dryRunLoading,
	open,
	policies,
	sourcePolicyId,
	submitting,
	targetPolicyId,
	onDryRun,
	onOpenChange,
	onSourcePolicyChange,
	onSubmit,
	onTargetPolicyChange,
}: StoragePolicyMigrationDialogProps) {
	const { t } = useTranslation("admin");
	const sourceId = Number(sourcePolicyId);
	const targetId = Number(targetPolicyId);
	const sourceLabel = selectedPolicyLabel(policies, sourcePolicyId);
	const targetLabel = selectedPolicyLabel(policies, targetPolicyId);
	const canSubmit =
		Number.isSafeInteger(sourceId) &&
		Number.isSafeInteger(targetId) &&
		sourceId > 0 &&
		targetId > 0 &&
		sourceId !== targetId &&
		dryRun?.source_policy_id === sourceId &&
		dryRun?.target_policy_id === targetId &&
		dryRun.can_start &&
		!dryRunLoading &&
		!submitting;
	const canDryRun =
		Number.isSafeInteger(sourceId) &&
		Number.isSafeInteger(targetId) &&
		sourceId > 0 &&
		targetId > 0 &&
		sourceId !== targetId &&
		!dryRunLoading &&
		!submitting;
	const targetAvailableBytes = dryRun?.target_capacity.available_bytes;
	const targetTotalBytes = dryRun?.target_capacity.total_bytes;
	const targetCapacityDetail =
		typeof targetAvailableBytes === "number" &&
		typeof targetTotalBytes === "number"
			? t("policy_migration_capacity_available_of_total", {
					available: formatBytes(targetAvailableBytes),
					total: formatBytes(targetTotalBytes),
				})
			: null;

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="sm:max-w-[42rem]">
				<DialogHeader>
					<DialogTitle>{t("policy_migration_title")}</DialogTitle>
					<DialogDescription>{t("policy_migration_desc")}</DialogDescription>
				</DialogHeader>

				<div className="space-y-4">
					<div className="grid gap-4 sm:grid-cols-2">
						<div className="space-y-2">
							<Label htmlFor="storage-migration-source">
								{t("policy_migration_source")}
							</Label>
							<Select
								value={sourcePolicyId}
								onValueChange={(value) => {
									if (value) onSourcePolicyChange(value);
								}}
								disabled={submitting}
							>
								<SelectTrigger id="storage-migration-source">
									<SelectValue
										placeholder={t("policy_migration_select_source")}
									>
										{sourceLabel}
									</SelectValue>
								</SelectTrigger>
								<SelectContent>
									{policies.map((policy) => (
										<SelectItem key={policy.id} value={String(policy.id)}>
											<span className="truncate">
												{policyOptionLabel(policy)}
											</span>
										</SelectItem>
									))}
								</SelectContent>
							</Select>
						</div>

						<div className="space-y-2">
							<Label htmlFor="storage-migration-target">
								{t("policy_migration_target")}
							</Label>
							<Select
								value={targetPolicyId}
								onValueChange={(value) => {
									if (value) onTargetPolicyChange(value);
								}}
								disabled={submitting}
							>
								<SelectTrigger id="storage-migration-target">
									<SelectValue
										placeholder={t("policy_migration_select_target")}
									>
										{targetLabel}
									</SelectValue>
								</SelectTrigger>
								<SelectContent>
									{policies.map((policy) => (
										<SelectItem
											key={policy.id}
											value={String(policy.id)}
											disabled={String(policy.id) === sourcePolicyId}
										>
											<span className="truncate">
												{policyOptionLabel(policy)}
											</span>
										</SelectItem>
									))}
								</SelectContent>
							</Select>
						</div>
					</div>

					{sourcePolicyId &&
					targetPolicyId &&
					sourcePolicyId === targetPolicyId ? (
						<div className="rounded-lg border border-destructive/25 bg-destructive/5 px-3 py-2 text-sm text-destructive">
							{t("policy_migration_same_policy_error")}
						</div>
					) : null}

					{dryRun ? (
						<div className="space-y-3 rounded-lg border bg-muted/15 p-3">
							<div className="flex flex-wrap items-start justify-between gap-2">
								<div>
									<div className="text-sm font-medium">
										{t("policy_migration_dry_run_title")}
									</div>
									<div className="mt-0.5 text-xs text-muted-foreground">
										{dryRun.can_start
											? t("policy_migration_dry_run_ready")
											: t("policy_migration_dry_run_blocked")}
									</div>
								</div>
								<span
									className={`rounded-full border px-2 py-0.5 text-xs font-medium ${
										dryRun.can_start
											? "border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900 dark:bg-emerald-950/60 dark:text-emerald-300"
											: "border-destructive/25 bg-destructive/5 text-destructive"
									}`}
								>
									{dryRun.can_start
										? t("policy_migration_can_start")
										: t("policy_migration_cannot_start")}
								</span>
							</div>
							<div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-5">
								{[
									{
										label: t("policy_migration_source_objects"),
										value: dryRun.source_blob_count,
									},
									{
										label: t("policy_migration_source_size"),
										value: formatBytes(dryRun.source_total_bytes),
									},
									{
										label: t("policy_migration_estimated_copy"),
										value: dryRun.estimated_copy_blob_count,
									},
									{
										label: t("policy_migration_target_matching"),
										value: dryRun.target_matching_blob_count,
									},
									{
										label: t("policy_migration_opaque_key_conflicts"),
										value: dryRun.opaque_key_conflict_count,
									},
								].map((item) => (
									<div
										key={item.label}
										className="rounded-md bg-background/70 px-2.5 py-2"
									>
										<div className="text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
											{item.label}
										</div>
										<div className="mt-1 text-sm font-semibold tabular-nums">
											{item.value}
										</div>
									</div>
								))}
							</div>
							<div className="grid gap-2 text-xs text-muted-foreground sm:grid-cols-2">
								<div>
									<span className="font-medium text-foreground">
										{t("policy_migration_target_probe")}:
									</span>{" "}
									{dryRun.target_connection_ok
										? t("policy_migration_ok")
										: t("policy_migration_failed")}
								</div>
								<div>
									<span className="font-medium text-foreground">
										{t("policy_migration_stream_upload")}:
									</span>{" "}
									{dryRun.target_supports_stream_upload
										? t("policy_migration_supported")
										: t("policy_migration_unsupported")}
								</div>
								<div>
									<span className="font-medium text-foreground">
										{t("policy_migration_capacity_check")}:
									</span>{" "}
									{t(
										`policy_migration_capacity_${dryRun.target_capacity_check}`,
									)}
									{targetCapacityDetail ? (
										<span className="ml-1 text-muted-foreground">
											{targetCapacityDetail}
										</span>
									) : null}
								</div>
								<div>
									<span className="font-medium text-foreground">
										{t("policy_migration_identity_mix")}:
									</span>{" "}
									{t("policy_migration_identity_counts", {
										content: dryRun.content_sha256_blob_count,
										opaque: dryRun.opaque_blob_count,
									})}
								</div>
							</div>
							{dryRun.warnings.length > 0 ? (
								<div className="space-y-1 rounded-md border border-amber-200 bg-amber-50 px-2.5 py-2 text-xs text-amber-800 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-200">
									{dryRun.warnings.map((warning) => (
										<div key={warning}>
											{t(`policy_migration_warning_${warning}`)}
										</div>
									))}
								</div>
							) : null}
						</div>
					) : null}
				</div>

				<DialogFooter className="gap-2 sm:justify-between">
					<Button
						type="button"
						variant="outline"
						onClick={() => onOpenChange(false)}
						disabled={submitting}
					>
						{t("core:cancel")}
					</Button>
					<div className="flex flex-wrap gap-2">
						<Button
							type="button"
							variant="outline"
							onClick={onDryRun}
							disabled={!canDryRun}
						>
							<Icon
								name={dryRunLoading ? "Spinner" : "ListChecks"}
								className={`mr-1 size-4 ${dryRunLoading ? "animate-spin" : ""}`}
							/>
							{dryRunLoading
								? t("policy_migration_dry_running")
								: t("policy_migration_dry_run")}
						</Button>
						<Button type="button" onClick={onSubmit} disabled={!canSubmit}>
							<Icon
								name={submitting ? "Spinner" : "ArrowsClockwise"}
								className={`mr-1 size-4 ${submitting ? "animate-spin" : ""}`}
							/>
							{submitting
								? t("policy_migration_creating")
								: t("policy_migration_submit")}
						</Button>
					</div>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
