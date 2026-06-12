import { type FormEvent, useEffect, useMemo, useReducer } from "react";
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
const CLOSED_TARGET_KEY = "__closed__";

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

interface FolderPolicyDialogState {
	folderInfo: FolderInfo | null;
	loading: boolean;
	policies: StoragePolicy[];
	saving: boolean;
	selectedPolicyId: string;
	targetKey: string;
}

type FolderPolicyDialogAction =
	| { type: "target_changed"; targetKey: string }
	| {
			type: "load_succeeded";
			targetKey: string;
			folderInfo: FolderInfo;
			policies: StoragePolicy[];
	  }
	| { type: "load_failed"; targetKey: string }
	| { type: "select_policy"; policyId: string }
	| { type: "save_started" }
	| { type: "save_succeeded"; folderInfo: FolderInfo }
	| { type: "save_finished" };

const initialState: FolderPolicyDialogState = {
	folderInfo: null,
	loading: false,
	policies: [],
	saving: false,
	selectedPolicyId: INHERIT_POLICY_VALUE,
	targetKey: CLOSED_TARGET_KEY,
};

function nextTargetState(targetKey: string): FolderPolicyDialogState {
	if (targetKey === CLOSED_TARGET_KEY) return initialState;
	return {
		folderInfo: null,
		loading: true,
		policies: [],
		saving: false,
		selectedPolicyId: INHERIT_POLICY_VALUE,
		targetKey,
	};
}

function folderPolicyDialogReducer(
	state: FolderPolicyDialogState,
	action: FolderPolicyDialogAction,
): FolderPolicyDialogState {
	switch (action.type) {
		case "target_changed":
			return nextTargetState(action.targetKey);
		case "load_succeeded":
			if (action.targetKey !== state.targetKey) return state;
			return {
				...state,
				folderInfo: action.folderInfo,
				loading: false,
				policies: action.policies,
				selectedPolicyId: buildInitialPolicyValue(action.folderInfo),
			};
		case "load_failed":
			if (action.targetKey !== state.targetKey) return state;
			return { ...state, loading: false };
		case "select_policy":
			return { ...state, selectedPolicyId: action.policyId };
		case "save_started":
			return { ...state, saving: true };
		case "save_succeeded":
			return {
				...state,
				folderInfo: action.folderInfo,
				selectedPolicyId: buildInitialPolicyValue(action.folderInfo),
			};
		case "save_finished":
			return { ...state, saving: false };
		default:
			return state;
	}
}

export function FolderPolicyDialog({
	open,
	onOpenChange,
	onOpenChangeComplete,
	folder,
	onUpdated,
}: FolderPolicyDialogProps) {
	const { t } = useTranslation(["files", "core"]);
	const [state, dispatch] = useReducer(folderPolicyDialogReducer, initialState);
	const targetFolderId = open && folder != null ? folder.id : null;
	const targetKey =
		targetFolderId != null ? String(targetFolderId) : CLOSED_TARGET_KEY;

	if (state.targetKey !== targetKey) {
		dispatch({ type: "target_changed", targetKey });
	}

	const currentState =
		state.targetKey === targetKey ? state : nextTargetState(targetKey);
	const { folderInfo, loading, policies, saving, selectedPolicyId } =
		currentState;

	useEffect(() => {
		if (targetFolderId == null) return;

		let canceled = false;
		const requestTargetKey = String(targetFolderId);

		Promise.all([
			fileService.getFolderInfo(targetFolderId),
			adminPolicyService.listAll(),
		])
			.then(([nextFolderInfo, nextPolicies]) => {
				if (canceled) return;
				dispatch({
					type: "load_succeeded",
					targetKey: requestTargetKey,
					folderInfo: nextFolderInfo,
					policies: nextPolicies,
				});
			})
			.catch((error: unknown) => {
				if (canceled) return;
				dispatch({ type: "load_failed", targetKey: requestTargetKey });
				handleApiError(error);
			})
			.catch(() => undefined);

		return () => {
			canceled = true;
		};
	}, [targetFolderId]);

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

		dispatch({ type: "save_started" });
		try {
			const updated = await adminFolderService.setPolicy(folder.id, {
				policy_id: policyId,
			});
			dispatch({ type: "save_succeeded", folderInfo: updated });
			toast.success(t("folder_policy_updated"));
			await onUpdated?.();
			onOpenChange(false);
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatch({ type: "save_finished" });
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
								if (value != null) {
									dispatch({ type: "select_policy", policyId: value });
								}
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
