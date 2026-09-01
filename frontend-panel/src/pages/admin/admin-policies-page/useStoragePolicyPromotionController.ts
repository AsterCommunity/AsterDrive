import type { Dispatch, SetStateAction } from "react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
	getPolicyForm,
	type PolicyFormData,
} from "@/components/admin/storage-policy-editor/formTypes";
import { policyFormHasUnsavedChanges } from "@/components/admin/storage-policy-editor/policyFormComparison";
import {
	applyStorageConnectorPromotion,
	findStorageConnectorPromotionCandidates,
	type StorageConnectorPromotionCandidate,
	storageConnectorPromotionKey,
} from "@/components/admin/storage-policy-editor/policyPromotion";
import { handleApiError } from "@/hooks/useApiError";
import { invalidateAdminPolicyLookup } from "@/lib/adminPolicyLookup";
import { translateStorageConnectorMessage } from "@/lib/adminStorageConnectorLocalizations";
import { adminPolicyService } from "@/services/adminService";
import type { StorageConnectorDescriptor, StoragePolicy } from "@/types/api";

interface StoragePolicyPromotionControllerInput {
	currentDescriptor: StorageConnectorDescriptor | null | undefined;
	editingId: number | null;
	editingPolicy: StoragePolicy | null;
	form: PolicyFormData;
	loadPolicyCapacity: (policyId: number) => void;
	onDraftApplied: () => void;
	onPromoted: () => void;
	setEditingPolicy: Dispatch<SetStateAction<StoragePolicy | null>>;
	setForm: Dispatch<SetStateAction<PolicyFormData>>;
	storageDriverDescriptors: StorageConnectorDescriptor[];
}

export function useStoragePolicyPromotionController({
	currentDescriptor,
	editingId,
	editingPolicy,
	form,
	loadPolicyCapacity,
	onDraftApplied,
	onPromoted,
	setEditingPolicy,
	setForm,
	storageDriverDescriptors,
}: StoragePolicyPromotionControllerInput) {
	const { t } = useTranslation("admin");
	const [confirmKey, setConfirmKey] = useState<string | null>(null);
	const [submittingKey, setSubmittingKey] = useState<string | null>(null);
	const requestSerialRef = useRef(0);
	const generationRef = useRef(0);
	const draftCandidates = useMemo(
		() =>
			findStorageConnectorPromotionCandidates(storageDriverDescriptors, form),
		[form, storageDriverDescriptors],
	);
	const savedCandidates = useMemo(
		() =>
			editingPolicy
				? findStorageConnectorPromotionCandidates(
						storageDriverDescriptors,
						getPolicyForm(editingPolicy),
					)
				: [],
		[editingPolicy, storageDriverDescriptors],
	);
	const candidates =
		draftCandidates.length > 0 ? draftCandidates : savedCandidates;
	const blocked =
		editingId !== null &&
		policyFormHasUnsavedChanges(form, editingPolicy, currentDescriptor);

	useEffect(() => {
		if (
			confirmKey !== null &&
			!candidates.some(
				(candidate) => storageConnectorPromotionKey(candidate) === confirmKey,
			)
		) {
			setConfirmKey(null);
		}
	}, [candidates, confirmKey]);

	const reset = () => {
		generationRef.current += 1;
		requestSerialRef.current += 1;
		setConfirmKey(null);
		setSubmittingKey(null);
	};

	const applyDraft = (candidate: StorageConnectorPromotionCandidate) => {
		setForm((current) => applyStorageConnectorPromotion(current, candidate));
		reset();
		onDraftApplied();
	};

	const request = (candidate: StorageConnectorPromotionCandidate) => {
		if (blocked) {
			return;
		}
		setConfirmKey(storageConnectorPromotionKey(candidate));
	};

	const cancel = () => setConfirmKey(null);

	const confirm = async (candidate: StorageConnectorPromotionCandidate) => {
		if (editingId === null || blocked) {
			return;
		}
		const key = storageConnectorPromotionKey(candidate);
		const savedCandidate = savedCandidates.find(
			(item) => storageConnectorPromotionKey(item) === key,
		);
		if (!savedCandidate) {
			return;
		}
		const generation = generationRef.current;
		const requestSerial = ++requestSerialRef.current;
		const isCurrentRequest = () =>
			generationRef.current === generation &&
			requestSerialRef.current === requestSerial;
		setSubmittingKey(key);
		try {
			const updated = await adminPolicyService.promoteConnector(editingId, {
				target_connector_id: savedCandidate.targetDescriptor.connector_id,
				promotion_id: savedCandidate.promotion.promotion_id,
			});
			invalidateAdminPolicyLookup();
			if (!isCurrentRequest()) {
				return;
			}
			setEditingPolicy(updated);
			setForm(getPolicyForm(updated));
			loadPolicyCapacity(updated.id);
			onPromoted();
			const targetLabel = translateStorageConnectorMessage(
				t,
				savedCandidate.targetDescriptor.connector_id,
				savedCandidate.targetDescriptor.ui.label_key,
			);
			toast.success(
				t("policy_connector_promotion_success", {
					connector: targetLabel,
				}),
			);
			setConfirmKey(null);
		} catch (error) {
			if (isCurrentRequest()) {
				handleApiError(error);
			}
		} finally {
			if (isCurrentRequest()) {
				setSubmittingKey(null);
			}
		}
	};

	return {
		applyDraft,
		blocked,
		cancel,
		candidates,
		confirm,
		confirmKey,
		request,
		reset,
		submittingKey,
	};
}
