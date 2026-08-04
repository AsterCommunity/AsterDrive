import type { Dispatch, SetStateAction } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { supportsStorageCredentialLifecycle } from "@/components/admin/storage-policy-dialog/descriptorPredicates";
import {
	connectorFormValue,
	getPolicyForm,
	type PolicyFormData,
} from "@/components/admin/storage-policy-dialog/formTypes";
import {
	buildCreatePolicyPayload,
	buildUpdatePolicyPayload,
} from "@/components/admin/storage-policy-dialog/payloadBuilders";
import { shouldRunPolicyConnectionSaveTest } from "@/components/admin/storage-policy-dialog/policyActionSelection";
import { handleApiError } from "@/hooks/useApiError";
import { invalidateAdminPolicyLookup } from "@/lib/adminPolicyLookup";
import { translateStorageConnectorMessage } from "@/lib/adminStorageConnectorLocalizations";
import { getStorageConnectorDescriptor } from "@/lib/adminStorageDriverDescriptors";
import { adminPolicyService } from "@/services/adminService";
import type { StorageConnectorDescriptor, StoragePolicy } from "@/types/api";

const CREATE_LAST_STEP = 2;

interface StoragePolicyEditorListBridge {
	offset: number;
	pageSize: number;
	reload: () => Promise<unknown>;
	setOffset: (updater: number | ((current: number) => number)) => void;
	setPolicies: Dispatch<SetStateAction<StoragePolicy[]>>;
	setTotal: Dispatch<SetStateAction<number>>;
	total: number;
}

interface SubmitPolicyActionBridge {
	runConnectionTest: (options?: {
		showSuccessToast?: boolean;
		showFailureError?: boolean;
	}) => Promise<boolean>;
	setValidatedConnectionKey: (key: string | null) => void;
	validatedConnectionKey: string | null;
}

interface StoragePolicyEditorControllerInput {
	allowSaveWithoutConnectionTest?: boolean;
	currentStorageDriverDescriptor: StorageConnectorDescriptor | null | undefined;
	editingId: number | null;
	editingPolicy: StoragePolicy | null;
	endpointValidationMessage: string | null;
	form: PolicyFormData;
	list: StoragePolicyEditorListBridge;
	loadPolicyCapacity: (policyId: number) => void;
	onCloseDialog: () => void;
	onPolicyCreated?: (policy: StoragePolicy) => Promise<void> | void;
	setCreateStep: Dispatch<SetStateAction<number>>;
	setCreateStepTouched: Dispatch<SetStateAction<boolean>>;
	setEditingId: Dispatch<SetStateAction<number | null>>;
	setEditingPolicy: Dispatch<SetStateAction<StoragePolicy | null>>;
	setForm: Dispatch<SetStateAction<PolicyFormData>>;
	setSaveAnywayConfirmOpen: Dispatch<SetStateAction<boolean>>;
	setSubmitting: Dispatch<SetStateAction<boolean>>;
	storageDriverDescriptors: StorageConnectorDescriptor[];
	syncNormalizedPolicyForm: () => PolicyFormData;
	submitting: boolean;
	createStep: number;
}

export function useStoragePolicyEditorController({
	allowSaveWithoutConnectionTest = true,
	currentStorageDriverDescriptor,
	createStep,
	editingId,
	editingPolicy,
	endpointValidationMessage,
	form,
	list,
	loadPolicyCapacity,
	onCloseDialog,
	onPolicyCreated,
	setCreateStep,
	setCreateStepTouched,
	setEditingId,
	setEditingPolicy,
	setForm,
	setSaveAnywayConfirmOpen,
	setSubmitting,
	storageDriverDescriptors,
	syncNormalizedPolicyForm,
	submitting,
}: StoragePolicyEditorControllerInput) {
	const { t } = useTranslation("admin");

	const persistPolicy = async (
		setValidatedConnectionKey: (key: string | null) => void,
	) => {
		try {
			const currentForm = syncNormalizedPolicyForm();
			const descriptor = getStorageConnectorDescriptor(
				storageDriverDescriptors,
				currentForm.connector_id,
			);
			if (!descriptor) {
				return;
			}
			if (editingId) {
				const updated = await adminPolicyService.update(
					editingId,
					buildUpdatePolicyPayload(currentForm, descriptor),
				);
				invalidateAdminPolicyLookup();
				setEditingId(updated.id);
				setEditingPolicy(updated);
				setForm(getPolicyForm(updated));
				setValidatedConnectionKey(null);
				loadPolicyCapacity(updated.id);
				list.setPolicies((prev) =>
					prev.map((policy) => (policy.id === editingId ? updated : policy)),
				);
				toast.success(t("policy_updated"));
			} else {
				const created = await adminPolicyService.create(
					buildCreatePolicyPayload(currentForm, descriptor),
				);
				invalidateAdminPolicyLookup();
				await onPolicyCreated?.(created);
				if (supportsStorageCredentialLifecycle(descriptor)) {
					setEditingId(created.id);
					setEditingPolicy(created);
					setForm(getPolicyForm(created));
					setValidatedConnectionKey(null);
					setCreateStep(0);
					setCreateStepTouched(false);
					list.setPolicies((prev) => {
						const existing = prev.some((policy) => policy.id === created.id);
						return existing
							? prev.map((policy) =>
									policy.id === created.id ? created : policy,
								)
							: [created, ...prev];
					});
					list.setTotal((current) => current + 1);
					loadPolicyCapacity(created.id);
					toast.success(
						descriptor.credential_management?.created_authorize_next_key
							? translateStorageConnectorMessage(
									t,
									descriptor.connector_id,
									descriptor.credential_management.created_authorize_next_key,
								)
							: t("policy_created"),
					);
					return;
				}
				const nextTotal = list.total + 1;
				const nextLastOffset = Math.max(
					0,
					Math.floor((nextTotal - 1) / list.pageSize) * list.pageSize,
				);
				if (nextLastOffset !== list.offset) {
					list.setOffset(nextLastOffset);
				} else {
					await list.reload();
				}
				toast.success(t("policy_created"));
				onCloseDialog();
			}
		} catch (e) {
			handleApiError(e);
		}
	};

	const shouldRunConnectionSaveTest = (validatedConnectionKey: string | null) =>
		shouldRunPolicyConnectionSaveTest({
			descriptor: currentStorageDriverDescriptor,
			editingId,
			editingPolicy,
			form,
			validatedConnectionKey,
		});

	const submitPolicy = async (
		{
			runConnectionTest,
			setValidatedConnectionKey,
			validatedConnectionKey,
		}: SubmitPolicyActionBridge,
		forceSave = false,
	) => {
		if (submitting) {
			return;
		}

		setSubmitting(true);
		try {
			if (!forceSave && shouldRunConnectionSaveTest(validatedConnectionKey)) {
				const testPassed = await runConnectionTest({
					showSuccessToast: false,
					showFailureError: !allowSaveWithoutConnectionTest,
				});
				if (!testPassed) {
					setSaveAnywayConfirmOpen(allowSaveWithoutConnectionTest);
					return;
				}
			}

			setSaveAnywayConfirmOpen(false);
			await persistPolicy(setValidatedConnectionKey);
		} finally {
			setSubmitting(false);
		}
	};

	const handleCreateBack = () => {
		setCreateStepTouched(false);
		setCreateStep((prev) => Math.max(0, prev - 1));
	};

	const handleCreateStepChange = (step: number) => {
		setCreateStepTouched(false);
		setCreateStep(Math.max(0, Math.min(CREATE_LAST_STEP, step)));
	};

	const handleCreateNext = () => {
		if (createStep >= CREATE_LAST_STEP) {
			return;
		}

		if (createStep === 0) {
			setCreateStep(1);
			return;
		}

		setCreateStepTouched(true);

		if (!form.name.trim()) {
			return;
		}

		if (
			hasMissingRequiredConnectorField(form, currentStorageDriverDescriptor)
		) {
			return;
		}

		if (endpointValidationMessage) {
			return;
		}

		syncNormalizedPolicyForm();
		setCreateStepTouched(false);
		setCreateStep(CREATE_LAST_STEP);
	};

	const handleSubmit = (actionBridge: SubmitPolicyActionBridge) => {
		if (editingId === null && createStep < CREATE_LAST_STEP) {
			handleCreateNext();
			return;
		}
		if (
			editingId === null &&
			hasMissingRequiredConnectorField(form, currentStorageDriverDescriptor)
		) {
			setCreateStepTouched(true);
			setCreateStep(1);
			return;
		}
		void submitPolicy(actionBridge);
	};

	const cancelSaveAnyway = () => {
		setSaveAnywayConfirmOpen(false);
	};

	const confirmSaveAnyway = (actionBridge: SubmitPolicyActionBridge) => {
		setSaveAnywayConfirmOpen(false);
		void submitPolicy(actionBridge, true);
	};

	return {
		cancelSaveAnyway,
		confirmSaveAnyway,
		handleCreateBack,
		handleCreateNext,
		handleCreateStepChange,
		handleSubmit,
	};
}

function hasMissingRequiredConnectorField(
	form: PolicyFormData,
	descriptor: StorageConnectorDescriptor | null | undefined,
) {
	return (
		descriptor?.fields.some((field) => {
			if (!field.required || field.scope === "action_input") {
				return false;
			}
			const value =
				field.scope === "connector_config"
					? connectorFormValue(form, field.name)
					: form.credential_values[field.name];
			return value === undefined || value === null || value === "";
		}) ?? true
	);
}
