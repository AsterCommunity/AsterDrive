import type { Dispatch, SetStateAction } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { supportsStorageCredentialLifecycle } from "@/components/admin/storage-policy-editor/descriptorPredicates";
import type { PolicyFormData } from "@/components/admin/storage-policy-editor/formTypes";
import {
	buildCreatePolicyPayload,
	buildUpdatePolicyPayload,
} from "@/components/admin/storage-policy-editor/payloadBuilders";
import { shouldRunPolicyConnectionSaveTest } from "@/components/admin/storage-policy-editor/policyActionSelection";
import { toastMissingRequiredConnectorFields } from "@/components/admin/storage-policy-editor/requiredFieldsToast";
import { handleApiError } from "@/hooks/useApiError";
import { invalidateAdminPolicyLookup } from "@/lib/adminPolicyLookup";
import { translateStorageConnectorMessage } from "@/lib/adminStorageConnectorLocalizations";
import { getStorageConnectorDescriptor } from "@/lib/adminStorageDriverDescriptors";
import { adminPolicyService } from "@/services/adminService";
import type { StorageConnectorDescriptor, StoragePolicy } from "@/types/api";

const CREATE_LAST_STEP = 2;

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
	onExit: () => void;
	onPolicyCreated?: (policy: StoragePolicy) => Promise<void> | void;
	onEnterCredentialSetup?: (policyId: number) => void;
	onUpdated: (policy: StoragePolicy) => void;
	setCreateStep: Dispatch<SetStateAction<number>>;
	setCreateStepTouched: Dispatch<SetStateAction<boolean>>;
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
	onExit,
	onPolicyCreated,
	onEnterCredentialSetup,
	onUpdated,
	setCreateStep,
	setCreateStepTouched,
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
				setValidatedConnectionKey(null);
				onUpdated(updated);
				toast.success(t("policy_updated"));
			} else {
				const created = await adminPolicyService.create(
					buildCreatePolicyPayload(currentForm, descriptor),
				);
				invalidateAdminPolicyLookup();
				await onPolicyCreated?.(created);
				if (supportsStorageCredentialLifecycle(descriptor)) {
					toast.success(
						descriptor.credential_management?.created_authorize_next_key
							? translateStorageConnectorMessage(
									t,
									descriptor.connector_id,
									descriptor.credential_management.created_authorize_next_key,
								)
							: t("policy_created"),
					);
					onEnterCredentialSetup?.(created.id);
					return;
				}
				toast.success(t("policy_created"));
				onExit();
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

		if (
			toastMissingRequiredConnectorFields(
				t,
				form,
				currentStorageDriverDescriptor,
				{
					allowSavedCredentials: editingId !== null,
				},
			)
		) {
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
			toastMissingRequiredConnectorFields(
				t,
				form,
				currentStorageDriverDescriptor,
			)
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
			toastMissingRequiredConnectorFields(
				t,
				form,
				currentStorageDriverDescriptor,
			)
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
