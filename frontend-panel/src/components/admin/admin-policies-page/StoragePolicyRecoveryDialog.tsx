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
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { writeTextToClipboard } from "@/lib/clipboard";
import { formatBytes } from "@/lib/format";
import type {
	StoragePolicy,
	StoragePolicyForcedPurgePreview,
	StoragePolicyRecoveryProbe,
} from "@/types/api";
import { getStoragePolicyRecoveryStatusPresentation } from "./policyPresentation";

interface StoragePolicyRecoveryDialogProps {
	confirmation: string;
	loading: boolean;
	open: boolean;
	policies: StoragePolicy[];
	policy: StoragePolicy | null;
	probe: StoragePolicyRecoveryProbe | null;
	purgePreview: StoragePolicyForcedPurgePreview | null;
	reason: string;
	submitting: boolean;
	targetPolicyId: string;
	onConfirmationChange: (value: string) => void;
	onOpenChange: (open: boolean) => void;
	onPreviewForcedPurge: () => void;
	onReasonChange: (value: string) => void;
	onRetryProbe: () => void;
	onStartForcedPurge: () => void;
	onStartRecovery: () => void;
	onTargetPolicyChange: (value: string) => void;
}

/** Presents read-only recovery evidence before any migration or destructive action. */
export function StoragePolicyRecoveryDialog({
	confirmation,
	loading,
	open,
	policies,
	policy,
	probe,
	purgePreview,
	reason,
	submitting,
	targetPolicyId,
	onConfirmationChange,
	onOpenChange,
	onPreviewForcedPurge,
	onReasonChange,
	onRetryProbe,
	onStartForcedPurge,
	onStartRecovery,
	onTargetPolicyChange,
}: StoragePolicyRecoveryDialogProps) {
	const { t } = useTranslation("admin");
	const targetOptions = policies
		.filter((item) => item.id !== policy?.id)
		.map((item) => ({
			label: `#${item.id} · ${item.name}`,
			value: String(item.id),
		}));
	const canRecover = Boolean(
		probe?.can_start_recovery &&
			targetOptions.some((item) => item.value === targetPolicyId),
	);
	const canPurge = Boolean(
		purgePreview?.can_start &&
			confirmation === purgePreview.confirmation_phrase,
	);
	const readableSampleCount =
		probe?.samples.filter((sample) => sample.status === "readable").length ?? 0;
	const failedSamples =
		probe?.samples.filter((sample) => sample.status !== "readable") ?? [];
	const probePresentation = getStoragePolicyRecoveryStatusPresentation(
		probe?.status,
		loading,
	);

	/** Copies the generated phrase and fills the controlled confirmation input. */
	const copyConfirmationPhrase = async () => {
		if (!purgePreview) return;
		onConfirmationChange(purgePreview.confirmation_phrase);
		try {
			await writeTextToClipboard(purgePreview.confirmation_phrase);
		} catch {
			// The filled input remains usable when browser clipboard access is blocked.
		}
	};

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-[46rem]">
				<DialogHeader>
					<DialogTitle>
						{t("policy_recovery_title", { name: policy?.name ?? "" })}
					</DialogTitle>
					<DialogDescription>{t("policy_recovery_desc")}</DialogDescription>
				</DialogHeader>

				<div className="space-y-5">
					<section
						className={`space-y-4 border px-4 py-4 ${probePresentation.toneClass}`}
					>
						<div className="flex items-start justify-between gap-4">
							<div className="flex min-w-0 items-start gap-3">
								<Icon
									name={probePresentation.icon}
									className={`mt-0.5 size-5 shrink-0 ${loading ? "animate-spin" : ""}`}
								/>
								<div className="min-w-0">
									<div className="text-base font-semibold">
										{t(probePresentation.titleKey)}
									</div>
									<div className="mt-1 text-sm opacity-80">
										{probe
											? t(`policy_recovery_status_${probe.status}`)
											: t("policy_recovery_probe_pending")}
									</div>
								</div>
							</div>
							<Button
								type="button"
								variant="outline"
								size="sm"
								onClick={onRetryProbe}
								disabled={loading || submitting}
							>
								<Icon
									name={loading ? "Spinner" : "ArrowClockwise"}
									className={`mr-1 size-4 ${loading ? "animate-spin" : ""}`}
								/>
								{t("policy_recovery_probe_retry")}
							</Button>
						</div>
						{probe ? (
							<>
								<div className="grid gap-3 border-t border-current/15 pt-3 sm:grid-cols-3">
									<div>
										<div className="text-xs opacity-70">
											{t("policy_recovery_probe_readable_samples")}
										</div>
										<div className="font-semibold tabular-nums">
											{readableSampleCount} / {probe.samples.length}
										</div>
									</div>
									<div>
										<div className="text-xs opacity-70">
											{t("policy_recovery_stored_blobs")}
										</div>
										<div className="font-semibold tabular-nums">
											{probe.stored_blob_count}
										</div>
									</div>
									<div>
										<div className="text-xs opacity-70">
											{t("policy_recovery_source_size")}
										</div>
										<div className="font-semibold tabular-nums">
											{formatBytes(probe.stored_total_bytes)}
										</div>
									</div>
								</div>
								{failedSamples.length > 0 ? (
									<div className="space-y-1 border-t border-current/15 pt-3 text-xs">
										<div className="font-medium">
											{t("policy_recovery_probe_failures", {
												count: failedSamples.length,
											})}
										</div>
										{failedSamples.slice(0, 3).map((sample) => (
											<div
												key={sample.blob_id}
												className="break-words opacity-80"
											>
												#{sample.blob_id} · {sample.error_kind ?? sample.status}
												{sample.diagnostic ? ` · ${sample.diagnostic}` : ""}
											</div>
										))}
									</div>
								) : null}
							</>
						) : null}
					</section>

					<section className="space-y-3 border-b pb-5">
						<div className="text-sm font-medium">
							{t("policy_recovery_migrate_title")}
						</div>
						<div className="space-y-2">
							<Label htmlFor="policy-recovery-target">
								{t("policy_recovery_target")}
							</Label>
							<Select
								items={targetOptions}
								value={targetPolicyId}
								onValueChange={(value) => value && onTargetPolicyChange(value)}
								disabled={!probe?.can_start_recovery || submitting}
							>
								<SelectTrigger id="policy-recovery-target">
									<SelectValue
										placeholder={t("policy_recovery_target_placeholder")}
									/>
								</SelectTrigger>
								<SelectContent>
									{targetOptions.map((item) => (
										<SelectItem key={item.value} value={item.value}>
											{item.label}
										</SelectItem>
									))}
								</SelectContent>
							</Select>
						</div>
						<Button
							type="button"
							onClick={onStartRecovery}
							disabled={!canRecover || loading || submitting}
						>
							<Icon
								name={submitting ? "Spinner" : "ArrowsClockwise"}
								className={`mr-1 size-4 ${submitting ? "animate-spin" : ""}`}
							/>
							{t("policy_recovery_start")}
						</Button>
					</section>

					<section className="space-y-3">
						<div>
							<div className="text-sm font-medium text-destructive">
								{t("policy_forced_purge_title")}
							</div>
							<div className="text-xs text-muted-foreground">
								{t("policy_forced_purge_desc")}
							</div>
						</div>
						{purgePreview ? (
							<>
								<div className="grid gap-2 text-sm sm:grid-cols-3">
									<div>
										{t("policy_forced_purge_files", {
											count: purgePreview.file_count,
										})}
									</div>
									<div>
										{t("policy_forced_purge_versions", {
											count: purgePreview.affected_revision_count,
										})}
									</div>
									<div>
										{t("policy_forced_purge_shares", {
											count: purgePreview.direct_share_count,
										})}
									</div>
								</div>
								{!purgePreview.can_start ? (
									<div className="text-sm text-destructive">
										{t("policy_forced_purge_blocked", {
											placements: purgePreview.placement_target_count,
										})}
									</div>
								) : null}
								{purgePreview.upload_session_count > 0 ? (
									<div className="text-sm text-muted-foreground">
										{t("policy_forced_purge_uploads_abandoned", {
											count: purgePreview.upload_session_count,
										})}
									</div>
								) : null}
								<div className="space-y-2">
									<Label htmlFor="policy-purge-reason">
										{t("policy_forced_purge_reason")}
									</Label>
									<textarea
										id="policy-purge-reason"
										className="min-h-24 w-full resize-y border bg-background px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
										value={reason}
										onChange={(event) => onReasonChange(event.target.value)}
										disabled={submitting}
									/>
								</div>
								<div className="space-y-2">
									<Label htmlFor="policy-purge-confirmation">
										{t("policy_forced_purge_confirmation_label")}
									</Label>
									<button
										type="button"
										className="flex w-full items-center justify-between gap-3 border bg-muted/20 px-3 py-2 text-left font-mono text-sm hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
										onClick={() => void copyConfirmationPhrase()}
										disabled={submitting}
										title={t("policy_forced_purge_copy_confirmation")}
									>
										<span className="min-w-0 break-all">
											{purgePreview.confirmation_phrase}
										</span>
										<Icon name="Copy" className="size-4 shrink-0" />
									</button>
									<Input
										id="policy-purge-confirmation"
										value={confirmation}
										onChange={(event) =>
											onConfirmationChange(event.target.value)
										}
										autoComplete="off"
										placeholder={purgePreview.confirmation_phrase}
										disabled={submitting}
									/>
									<div className="grid gap-1 text-xs">
										<div
											className={
												purgePreview.can_start
													? "text-emerald-600"
													: "text-destructive"
											}
										>
											{purgePreview.can_start
												? t("policy_forced_purge_check_topology_ready")
												: t("policy_forced_purge_check_topology_blocked")}
										</div>
										<div
											className={
												confirmation === purgePreview.confirmation_phrase
													? "text-emerald-600"
													: "text-muted-foreground"
											}
										>
											{confirmation === purgePreview.confirmation_phrase
												? t("policy_forced_purge_check_confirmation_ready")
												: t("policy_forced_purge_check_confirmation_pending")}
										</div>
									</div>
								</div>
								<Button
									type="button"
									variant="destructive"
									onClick={onStartForcedPurge}
									disabled={!canPurge || submitting}
								>
									<Icon
										name={submitting ? "Spinner" : "Trash"}
										className={`mr-1 size-4 ${submitting ? "animate-spin" : ""}`}
									/>
									{t("policy_forced_purge_start")}
								</Button>
							</>
						) : (
							<Button
								type="button"
								variant="outline"
								onClick={onPreviewForcedPurge}
								disabled={loading || submitting}
							>
								{t("policy_forced_purge_preview")}
							</Button>
						)}
					</section>
				</div>

				<DialogFooter>
					<Button
						type="button"
						variant="outline"
						onClick={() => onOpenChange(false)}
						disabled={submitting}
					>
						{t("core:cancel")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
