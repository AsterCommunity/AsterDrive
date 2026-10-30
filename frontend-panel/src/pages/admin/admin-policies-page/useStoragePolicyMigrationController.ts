import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { useDialogActivity } from "@/components/ui/dialog-lifecycle";
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

	const isMounted = useDialogActivity();
	const isCurrent = useDialogActivity(
		`${sourcePolicyId}:${targetPolicyId}`,
		open,
	);
	const openDialog = async () => {
		try {
			const allPolicies = await adminPolicyService.listAll();
			if (!isMounted()) return;
			const firstPolicy = allPolicies[0];
			const secondPolicy = allPolicies.find(
				(policy) => policy.id !== firstPolicy?.id,
			);
			setPolicies(allPolicies);
			setSourcePolicyId(firstPolicy ? String(firstPolicy.id) : "");
			setTargetPolicyId(secondPolicy ? String(secondPolicy.id) : "");
			setDryRun(null);
			setDryRunLoading(false);
			setSubmitting(false);
			setOpen(true);
		} catch (error) {
			handleApiError(error);
		}
	};

	const handleSourcePolicyChange = (policyId: string) => {
		setSourcePolicyId(policyId);
		setDryRunLoading(false);
		setDryRun(null);
		if (policyId === targetPolicyId) {
			setTargetPolicyId("");
		}
	};

	const handleTargetPolicyChange = (policyId: string) => {
		setDryRunLoading(false);
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
			});
			if (isCurrent()) setOpen(false);
			toast.success(t("policy_migration_created", { id: task.id }));
			if (isCurrent())
				navigate("/admin/tasks?kind=storage_policy_migration", {
					viewTransition: false,
				});
		} catch (error) {
			if (isCurrent()) handleApiError(error);
		} finally {
			if (isCurrent()) setSubmitting(false);
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
			const result = await adminPolicyService.dryRunMigration({
				source_policy_id: sourceId,
				target_policy_id: targetId,
			});
			if (isCurrent()) setDryRun(result);
		} catch (error) {
			if (!isCurrent()) return;
			setDryRun(null);
			handleApiError(error);
		} finally {
			if (isCurrent()) setDryRunLoading(false);
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
