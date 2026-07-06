import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { handleApiError } from "@/hooks/useApiError";
import { adminPolicyService } from "@/services/adminService";
import type { StoragePolicy, StoragePolicyMigrationDryRun } from "@/types/api";

export function useStoragePolicyMigrationController() {
	const { t } = useTranslation("admin");
	const navigate = useNavigate();
	const [open, setOpen] = useState(false);
	const [policies, setPolicies] = useState<StoragePolicy[]>([]);
	const [sourcePolicyId, setSourcePolicyId] = useState("");
	const [targetPolicyId, setTargetPolicyId] = useState("");
	const [dryRun, setDryRun] = useState<StoragePolicyMigrationDryRun | null>(
		null,
	);
	const [dryRunLoading, setDryRunLoading] = useState(false);
	const [submitting, setSubmitting] = useState(false);

	const openDialog = async () => {
		try {
			const allPolicies = await adminPolicyService.listAll();
			const firstPolicy = allPolicies[0];
			const secondPolicy = allPolicies.find(
				(policy) => policy.id !== firstPolicy?.id,
			);
			setPolicies(allPolicies);
			setSourcePolicyId(firstPolicy ? String(firstPolicy.id) : "");
			setTargetPolicyId(secondPolicy ? String(secondPolicy.id) : "");
			setDryRun(null);
			setOpen(true);
		} catch (error) {
			handleApiError(error);
		}
	};

	const handleSourcePolicyChange = (policyId: string) => {
		setSourcePolicyId(policyId);
		setDryRun(null);
		if (policyId === targetPolicyId) {
			setTargetPolicyId("");
		}
	};

	const handleTargetPolicyChange = (policyId: string) => {
		if (policyId === sourcePolicyId) {
			setTargetPolicyId("");
			setDryRun(null);
			toast.error(t("policy_migration_same_policy_error"));
			return;
		}
		setTargetPolicyId(policyId);
		setDryRun(null);
	};

	const createMigration = async () => {
		if (submitting) return;
		const sourceId = Number(sourcePolicyId);
		const targetId = Number(targetPolicyId);
		if (
			!Number.isSafeInteger(sourceId) ||
			!Number.isSafeInteger(targetId) ||
			sourceId <= 0 ||
			targetId <= 0
		) {
			return;
		}
		if (sourceId === targetId) {
			toast.error(t("policy_migration_same_policy_error"));
			return;
		}
		if (
			dryRun?.source_policy_id !== sourceId ||
			dryRun?.target_policy_id !== targetId ||
			!dryRun.can_start ||
			dryRunLoading
		) {
			return;
		}

		setSubmitting(true);
		try {
			const task = await adminPolicyService.createMigration({
				source_policy_id: sourceId,
				target_policy_id: targetId,
				delete_source_after_success: false,
			});
			setOpen(false);
			toast.success(t("policy_migration_created", { id: task.id }));
			navigate("/admin/tasks?kind=storage_policy_migration", {
				viewTransition: false,
			});
		} catch (error) {
			handleApiError(error);
		} finally {
			setSubmitting(false);
		}
	};

	const dryRunMigration = async () => {
		if (dryRunLoading || submitting) return;
		const sourceId = Number(sourcePolicyId);
		const targetId = Number(targetPolicyId);
		if (
			!Number.isSafeInteger(sourceId) ||
			!Number.isSafeInteger(targetId) ||
			sourceId <= 0 ||
			targetId <= 0
		) {
			return;
		}
		if (sourceId === targetId) {
			setDryRun(null);
			toast.error(t("policy_migration_same_policy_error"));
			return;
		}

		setDryRunLoading(true);
		try {
			setDryRun(
				await adminPolicyService.dryRunMigration({
					source_policy_id: sourceId,
					target_policy_id: targetId,
					delete_source_after_success: false,
				}),
			);
		} catch (error) {
			setDryRun(null);
			handleApiError(error);
		} finally {
			setDryRunLoading(false);
		}
	};

	return {
		createMigration,
		dryRun,
		dryRunLoading,
		dryRunMigration,
		handleSourcePolicyChange,
		handleTargetPolicyChange,
		open,
		openDialog,
		policies,
		setOpen,
		sourcePolicyId,
		submitting,
		targetPolicyId,
	};
}
