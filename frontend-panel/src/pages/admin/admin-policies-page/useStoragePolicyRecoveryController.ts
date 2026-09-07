import { useRef, useState } from "react";
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
	const workflowRef = useRef({
		generation: 0,
		policyId: null as number | null,
	});

	/** Starts a request generation tied to one source policy. */
	const beginWorkflowGeneration = (policyId: number) => {
		const generation = workflowRef.current.generation + 1;
		workflowRef.current = { generation, policyId };
		return generation;
	};

	/** Returns whether an asynchronous result still belongs to the visible workflow. */
	const isCurrentWorkflow = (generation: number, policyId: number) =>
		workflowRef.current.generation === generation &&
		workflowRef.current.policyId === policyId;

	/** Invalidates pending responses and closes the current workflow. */
	const closeWorkflow = () => {
		workflowRef.current = {
			generation: workflowRef.current.generation + 1,
			policyId: null,
		};
		setOpen(false);
		setLoading(false);
		setSubmitting(false);
	};

	/** Loads target policies and probe evidence independently for one generation. */
	const loadRecoveryEvidence = async (
		selected: StoragePolicy,
		generation: number,
	) => {
		setPolicies([]);
		setTargetPolicyId("");
		setProbe(null);
		setPurgePreview(null);
		setLoading(true);

		const [policiesResult, probeResult] = await Promise.allSettled([
			adminPolicyService.listAll(),
			adminPolicyService.probeRecovery(selected.id),
		]);
		if (!isCurrentWorkflow(generation, selected.id)) return;

		if (policiesResult.status === "fulfilled") {
			setPolicies(policiesResult.value);
			setTargetPolicyId(
				String(
					policiesResult.value.find((item) => item.id !== selected.id)?.id ??
						"",
				),
			);
		} else {
			handleApiError(policiesResult.reason);
		}

		if (probeResult.status === "fulfilled") {
			setProbe(probeResult.value);
		} else {
			handleApiError(probeResult.reason);
		}
		setLoading(false);
	};

	/** Opens the workflow and obtains fresh writer-backed probe evidence. */
	const openForPolicy = async (selected: StoragePolicy) => {
		const generation = beginWorkflowGeneration(selected.id);
		setPolicy(selected);
		setReason("");
		setConfirmation("");
		setOpen(true);
		await loadRecoveryEvidence(selected, generation);
	};

	/** Repeats the read-only probe after credentials, network, or storage state changes. */
	const retryProbe = async () => {
		if (!policy || loading || submitting) return;
		const generation = beginWorkflowGeneration(policy.id);
		await loadRecoveryEvidence(policy, generation);
	};

	/** Creates a recoverable-data migration bound to the latest probe hash. */
	const startRecovery = async () => {
		if (!policy || !probe?.can_start_recovery || submitting) return;
		const generation = workflowRef.current.generation;
		const policyId = policy.id;
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
				source_policy_id: policyId,
				target_policy_id: targetId,
				mode: "recover_available",
				recovery_plan_hash: probe.plan_hash,
			});
			if (!isCurrentWorkflow(generation, policyId)) return;
			closeWorkflow();
			toast.success(t("policy_recovery_task_created", { id: task.id }));
			navigate("/admin/tasks?kind=storage_policy_migration", {
				viewTransition: false,
			});
		} catch (error) {
			if (isCurrentWorkflow(generation, policyId)) handleApiError(error);
		} finally {
			if (isCurrentWorkflow(generation, policyId)) setSubmitting(false);
		}
	};

	/** Loads a fresh destructive impact snapshot before revealing confirmation controls. */
	const previewForcedPurge = async () => {
		if (!policy || loading || submitting) return;
		const generation = workflowRef.current.generation;
		const policyId = policy.id;
		setLoading(true);
		try {
			const preview = await adminPolicyService.previewForcedPurge(policyId);
			if (!isCurrentWorkflow(generation, policyId)) return;
			setPurgePreview(preview);
			setConfirmation("");
		} catch (error) {
			if (isCurrentWorkflow(generation, policyId)) handleApiError(error);
		} finally {
			if (isCurrentWorkflow(generation, policyId)) setLoading(false);
		}
	};

	/** Creates the destructive task only when phrase, reason, and digest all match. */
	const startForcedPurge = async () => {
		if (!policy || !purgePreview?.can_start || submitting) return;
		const generation = workflowRef.current.generation;
		const policyId = policy.id;
		setSubmitting(true);
		try {
			const task = await adminPolicyService.createForcedPurge(policyId, {
				impact_digest: purgePreview.impact_digest,
				confirmation,
				reason,
			});
			if (!isCurrentWorkflow(generation, policyId)) return;
			closeWorkflow();
			toast.success(t("policy_forced_purge_task_created", { id: task.id }));
			navigate("/admin/tasks?kind=storage_policy_forced_purge", {
				viewTransition: false,
			});
		} catch (error) {
			if (isCurrentWorkflow(generation, policyId)) handleApiError(error);
		} finally {
			if (isCurrentWorkflow(generation, policyId)) setSubmitting(false);
		}
	};

	/** Handles dialog visibility while invalidating requests when it closes. */
	const setWorkflowOpen = (nextOpen: boolean) => {
		if (nextOpen) {
			setOpen(true);
		} else {
			closeWorkflow();
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
		setOpen: setWorkflowOpen,
		setReason,
		setTargetPolicyId,
		startForcedPurge,
		startRecovery,
		submitting,
		targetPolicyId,
	};
}
