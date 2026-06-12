import { type FormEvent, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
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
import { handleApiError } from "@/hooks/useApiError";
import { cn } from "@/lib/utils";
import {
	adminFolderService,
	adminPolicyService,
} from "@/services/adminService";
import { fileService } from "@/services/fileService";
import type { FolderInfo, FolderListItem, StoragePolicy } from "@/types/api";

const INHERIT_POLICY_VALUE = "__inherit__";

interface FolderPolicyDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onOpenChangeComplete?: (open: boolean) => void;
	folder: FolderInfo | FolderListItem | null;
	onUpdated?: () => Promise<void> | void;
}

function formatPolicyDriver(policy: StoragePolicy) {
	return policy.driver_type === "remote"
		? `remote #${policy.remote_node_id ?? "-"}`
		: policy.driver_type;
}

function buildInitialPolicyValue(folderInfo: FolderInfo | null) {
	return folderInfo?.policy_id != null
		? String(folderInfo.policy_id)
		: INHERIT_POLICY_VALUE;
}

function selectedPolicyLabel(
	value: string,
	policies: StoragePolicy[],
	inheritLabel: string,
) {
	if (value === INHERIT_POLICY_VALUE) return inheritLabel;
	const policy = policies.find((item) => String(item.id) === value);
	return policy ? `${policy.name} (#${policy.id})` : `#${value}`;
}

export function FolderPolicyDialog({
	open,
	onOpenChange,
	onOpenChangeComplete,
	folder,
	onUpdated,
}: FolderPolicyDialogProps) {
	const { t } = useTranslation(["files", "core"]);
	const [folderInfo, setFolderInfo] = useState<FolderInfo | null>(null);
	const [policies, setPolicies] = useState<StoragePolicy[]>([]);
	const [selectedPolicyId, setSelectedPolicyId] =
		useState(INHERIT_POLICY_VALUE);
	const [loading, setLoading] = useState(false);
	const [saving, setSaving] = useState(false);

	useEffect(() => {
		if (!open || folder == null) {
			setFolderInfo(null);
			setPolicies([]);
			setSelectedPolicyId(INHERIT_POLICY_VALUE);
			setLoading(false);
			setSaving(false);
			return;
		}

		let canceled = false;
		setLoading(true);
		setFolderInfo(null);
		setPolicies([]);
		setSelectedPolicyId(INHERIT_POLICY_VALUE);

		Promise.all([
			fileService.getFolderInfo(folder.id),
			adminPolicyService.listAll(),
		])
			.then(([nextFolderInfo, nextPolicies]) => {
				if (canceled) return;
				setFolderInfo(nextFolderInfo);
				setPolicies(nextPolicies);
				setSelectedPolicyId(buildInitialPolicyValue(nextFolderInfo));
			})
			.catch((error: unknown) => {
				if (canceled) return;
				handleApiError(error);
			})
			.finally(() => {
				if (!canceled) setLoading(false);
			});

		return () => {
			canceled = true;
		};
	}, [folder, open]);

	const currentPolicy = useMemo(() => {
		if (folderInfo?.policy_id == null) return null;
		return (
			policies.find((policy) => policy.id === folderInfo.policy_id) ?? null
		);
	}, [folderInfo, policies]);

	const targetFolderName = folderInfo?.name ?? folder?.name ?? "";
	const initialPolicyValue = buildInitialPolicyValue(folderInfo);
	const changed = selectedPolicyId !== initialPolicyValue;
	const canSubmit = folder != null && folderInfo != null && !loading && !saving;
	const inheritLabel = t("folder_policy_inherit");
	const selectedLabel = selectedPolicyLabel(
		selectedPolicyId,
		policies,
		inheritLabel,
	);

	const handleSubmit = async (event: FormEvent) => {
		event.preventDefault();
		if (!canSubmit || !folder) return;

		const policyId =
			selectedPolicyId === INHERIT_POLICY_VALUE
				? null
				: Number(selectedPolicyId);
		if (policyId !== null && !Number.isFinite(policyId)) {
			return;
		}

		setSaving(true);
		try {
			const updated = await adminFolderService.setPolicy(folder.id, {
				policy_id: policyId,
			});
			setFolderInfo(updated);
			setSelectedPolicyId(buildInitialPolicyValue(updated));
			toast.success(t("folder_policy_updated"));
			await onUpdated?.();
			onOpenChange(false);
		} catch (error) {
			handleApiError(error);
		} finally {
			setSaving(false);
		}
	};

	return (
		<Dialog
			open={open}
			onOpenChange={onOpenChange}
			onOpenChangeComplete={onOpenChangeComplete}
		>
			<DialogContent
				keepMounted
				className="w-[calc(100%-1rem)] max-w-[calc(100%-1rem)] sm:max-w-md"
			>
				<DialogHeader>
					<DialogTitle>{t("folder_policy_title")}</DialogTitle>
					<DialogDescription>
						{t("folder_policy_description", { name: targetFolderName })}
					</DialogDescription>
				</DialogHeader>

				<form onSubmit={handleSubmit} className="space-y-4">
					<div className="rounded-lg border border-border/70 bg-muted/25 p-3">
						<div className="flex items-start gap-3">
							<div className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-lg border border-border/60 bg-background">
								<Icon
									name="FolderOpen"
									className="size-4 text-muted-foreground"
								/>
							</div>
							<div className="min-w-0 flex-1">
								<div className="truncate text-sm font-medium">
									{targetFolderName || t("core:folder")}
								</div>
								<div className="mt-1 text-xs text-muted-foreground">
									{loading
										? t("core:loading")
										: currentPolicy
											? t("folder_policy_current_named", {
													name: currentPolicy.name,
													id: currentPolicy.id,
												})
											: t("folder_policy_current_inherit")}
								</div>
							</div>
						</div>
					</div>

					<div className="space-y-2">
						<Label id="folder-policy-select-label">
							{t("folder_policy_select")}
						</Label>
						<Select
							value={selectedPolicyId}
							onValueChange={(value) => {
								if (value != null) setSelectedPolicyId(value);
							}}
							disabled={loading || saving || folderInfo == null}
						>
							<SelectTrigger aria-labelledby="folder-policy-select-label">
								<SelectValue placeholder={t("folder_policy_select")}>
									{selectedLabel}
								</SelectValue>
							</SelectTrigger>
							<SelectContent>
								<SelectItem value={INHERIT_POLICY_VALUE}>
									<span className="flex min-w-0 items-center gap-2">
										<Icon name="ArrowCounterClockwise" className="size-4" />
										<span className="truncate">{inheritLabel}</span>
									</span>
								</SelectItem>
								{policies.map((policy) => (
									<SelectItem key={policy.id} value={String(policy.id)}>
										<span className="flex min-w-0 flex-1 items-center justify-between gap-3">
											<span className="min-w-0 truncate">{policy.name}</span>
											<span className="shrink-0 text-xs text-muted-foreground">
												#{policy.id} · {formatPolicyDriver(policy)}
											</span>
										</span>
									</SelectItem>
								))}
							</SelectContent>
						</Select>
						<p className="text-xs leading-relaxed text-muted-foreground">
							{t("folder_policy_inherited_hint")}
						</p>
						{!loading && policies.length === 0 ? (
							<p className="text-xs text-muted-foreground">
								{t("folder_policy_empty")}
							</p>
						) : null}
					</div>

					<DialogFooter className="gap-2">
						<Button
							type="button"
							variant="outline"
							onClick={() => onOpenChange(false)}
							disabled={saving}
						>
							{t("core:cancel")}
						</Button>
						<Button
							type="submit"
							disabled={!canSubmit || !changed}
							className={cn(saving && "cursor-wait")}
						>
							{saving ? (
								<Icon name="Spinner" className="size-4 animate-spin" />
							) : (
								<Icon name="FloppyDisk" className="size-4" />
							)}
							{t("folder_policy_save")}
						</Button>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>
	);
}
