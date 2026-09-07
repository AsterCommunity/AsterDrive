import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { handleApiError } from "@/hooks/useApiError";
import { adminPolicyService } from "@/services/adminService";
import type {
	StoragePolicy,
	StoragePolicyForcedPurgePreview,
	StoragePolicyRecoveryProbe,
} from "@/types/api";

/** Coordinates recovery probing, migration creation, and confirmed destructive purge. */
export function useStoragePolicyRecoveryController() {
	const { t } = useTranslation("admin");
	const navigate = useNavigate();
	const [open, setOpen] = useState(false);
	const [policy, setPolicy] = useState<StoragePolicy | null>(null);
	const [policies, setPolicies] = useState<StoragePolicy[]>([]);
	const [probe, setProbe] = useState<StoragePolicyRecoveryProbe | null>(null);
	const [targetPolicyId, setTargetPolicyId] = useState("");
	const [purgePreview, setPurgePreview] =
		useState<StoragePolicyForcedPurgePreview | null>(null);
	const [reason, setReason] = useState("");
	const [confirmation, setConfirmation] = useState("");
	const [loading, setLoading] = useState(false);
	const [submitting, setSubmitting] = useState(false);

	/** Opens the workflow and obtains fresh writer-backed probe evidence. */
	const openForPolicy = async (selected: StoragePolicy) => {
		setPolicy(selected);
		setProbe(null);
		setPurgePreview(null);
		setReason("");
		setConfirmation("");
		setOpen(true);
		setLoading(true);
		try {
			const [allPolicies, nextProbe] = await Promise.all([
				adminPolicyService.listAll(),
				adminPolicyService.probeRecovery(selected.id),
			]);
			setPolicies(allPolicies);
			setProbe(nextProbe);
			setTargetPolicyId(
				String(allPolicies.find((item) => item.id !== selected.id)?.id ?? ""),
			);
		} catch (error) {
			handleApiError(error);
		} finally {
			setLoading(false);
		}
	};

	/** Repeats the read-only probe after credentials, network, or storage state changes. */
	const retryProbe = async () => {
		if (!policy || loading || submitting) return;
		setLoading(true);
		try {
			setProbe(await adminPolicyService.probeRecovery(policy.id));
			setPurgePreview(null);
		} catch (error) {
			handleApiError(error);
		} finally {
			setLoading(false);
		}
	};

	/** Creates a recoverable-data migration bound to the latest probe hash. */
	const startRecovery = async () => {
		if (!policy || !probe?.can_start_recovery || submitting) return;
		const targetId = Number(targetPolicyId);
		if (
			!Number.isSafeInteger(targetId) ||
			targetId <= 0 ||
			targetId === policy.id
		) {
			return;
		}
		setSubmitting(true);
		try {
			const task = await adminPolicyService.createMigration({
				source_policy_id: policy.id,
				target_policy_id: targetId,
				mode: "recover_available",
				recovery_plan_hash: probe.plan_hash,
			});
			setOpen(false);
			toast.success(t("policy_recovery_task_created", { id: task.id }));
			navigate("/admin/tasks?kind=storage_policy_migration", {
				viewTransition: false,
			});
		} catch (error) {
			handleApiError(error);
		} finally {
			setSubmitting(false);
		}
	};

	/** Loads a fresh destructive impact snapshot before revealing confirmation controls. */
	const previewForcedPurge = async () => {
		if (!policy || loading || submitting) return;
		setLoading(true);
		try {
			setPurgePreview(await adminPolicyService.previewForcedPurge(policy.id));
			setConfirmation("");
		} catch (error) {
			handleApiError(error);
		} finally {
			setLoading(false);
		}
	};

	/** Creates the destructive task only when phrase, reason, and digest all match. */
	const startForcedPurge = async () => {
		if (!policy || !purgePreview?.can_start || submitting) return;
		setSubmitting(true);
		try {
			const task = await adminPolicyService.createForcedPurge(policy.id, {
				impact_digest: purgePreview.impact_digest,
				confirmation,
				reason,
			});
			setOpen(false);
			toast.success(t("policy_forced_purge_task_created", { id: task.id }));
			navigate("/admin/tasks?kind=storage_policy_forced_purge", {
				viewTransition: false,
			});
		} catch (error) {
			handleApiError(error);
		} finally {
			setSubmitting(false);
		}
	};

	return {
		confirmation,
		loading,
		open,
		openForPolicy,
		policies,
		policy,
		previewForcedPurge,
		probe,
		purgePreview,
		reason,
		retryProbe,
		setConfirmation,
		setOpen,
		setReason,
		setTargetPolicyId,
		startForcedPurge,
		startRecovery,
		submitting,
		targetPolicyId,
	};
}
