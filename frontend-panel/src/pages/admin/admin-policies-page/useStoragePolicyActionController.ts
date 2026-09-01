import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { presentStorageConnectorActionOutput } from "@/components/admin/storage-policy-editor/actionResultPresentation";
import {
	getEndpointValidationMessage,
	getPolicyConnectionTestKey,
} from "@/components/admin/storage-policy-editor/connectionNormalization";
import { findStorageConnectorAction } from "@/components/admin/storage-policy-editor/descriptorPredicates";
import type { PolicyFormData } from "@/components/admin/storage-policy-editor/formTypes";
import {
	buildPolicyTestPayload,
	buildStorageConnectorActionPayload,
} from "@/components/admin/storage-policy-editor/payloadBuilders";
import {
	selectStorageConnectorCustomActionExecutionMode,
	selectStoragePolicyActionValueSource,
	selectStoragePolicyConnectionTestMode,
} from "@/components/admin/storage-policy-editor/policyActionSelection";
import { toastMissingRequiredConnectorFields } from "@/components/admin/storage-policy-editor/requiredFieldsToast";
import type { StorageConnectorActionValues } from "@/components/admin/storage-policy-editor/StorageConnectorActionsPanel";
import { handleApiError } from "@/hooks/useApiError";
import { translateStorageConnectorMessage } from "@/lib/adminStorageConnectorLocalizations";
import { getStorageConnectorDescriptor } from "@/lib/adminStorageDriverDescriptors";
import { adminPolicyService } from "@/services/adminService";
import type {
	StorageConnectorActionId,
	StorageConnectorDescriptor,
	StorageConnectorFieldValue,
	StoragePolicy,
	StoragePolicyActionResult,
} from "@/types/api";

interface StoragePolicyActionControllerInput {
	currentStorageDriverDescriptor: StorageConnectorDescriptor | null | undefined;
	editingId: number | null;
	editingPolicy: StoragePolicy | null;
	storageDriverDescriptors: StorageConnectorDescriptor[];
	syncNormalizedPolicyForm: () => PolicyFormData;
}

export function useStoragePolicyActionController({
	currentStorageDriverDescriptor,
	editingId,
	editingPolicy,
	storageDriverDescriptors,
	syncNormalizedPolicyForm,
}: StoragePolicyActionControllerInput) {
	const { t } = useTranslation("admin");
	const connectorT = (key: string, values?: Record<string, number | string>) =>
		translateStorageConnectorMessage(
			t,
			currentStorageDriverDescriptor?.connector_id,
			key,
			values,
		);
	const [connectorActionConfirmId, setConnectorActionConfirmId] = useState<
		string | null
	>(null);
	const [connectorActionSubmittingId, setConnectorActionSubmittingId] =
		useState<string | null>(null);
	const [connectorActionValues, setConnectorActionValues] =
		useState<StorageConnectorActionValues>({});
	const [validatedConnectionKey, setValidatedConnectionKey] = useState<
		string | null
	>(null);

	const clearActionConfirms = () => {
		setConnectorActionConfirmId(null);
	};

	const resetActionState = () => {
		clearActionConfirms();
		setConnectorActionSubmittingId(null);
		setConnectorActionValues({});
		setValidatedConnectionKey(null);
	};

	const runConnectionTest = async ({
		showSuccessToast = true,
		showFailureError = true,
	}: {
		showSuccessToast?: boolean;
		showFailureError?: boolean;
	} = {}) => {
		const currentForm = syncNormalizedPolicyForm();
		const descriptor = getStorageConnectorDescriptor(
			storageDriverDescriptors,
			currentForm.connector_id,
		);
		if (!descriptor) {
			setValidatedConnectionKey(null);
			return false;
		}
		const currentEndpointValidationMessage = getEndpointValidationMessage(
			currentForm,
			(key) =>
				translateStorageConnectorMessage(t, descriptor.connector_id, key),
			descriptor,
		);
		if (currentEndpointValidationMessage) {
			if (showFailureError) {
				toast.error(currentEndpointValidationMessage);
			}
			setValidatedConnectionKey(null);
			return false;
		}

		if (
			toastMissingRequiredConnectorFields(t, currentForm, descriptor, {
				allowSavedCredentials: editingId !== null,
			})
		) {
			setValidatedConnectionKey(null);
			return false;
		}

		const connectionTestMode = selectStoragePolicyConnectionTestMode({
			descriptor,
			editingId,
			editingPolicy,
			form: currentForm,
		});
		if (connectionTestMode === "unsupported") {
			setValidatedConnectionKey(null);
			return false;
		}

		try {
			if (connectionTestMode === "draft") {
				await adminPolicyService.testParams(
					buildPolicyTestPayload(currentForm, descriptor, editingId),
				);
			} else if (editingId !== null) {
				await adminPolicyService.testConnection(editingId);
			}

			setValidatedConnectionKey(
				getPolicyConnectionTestKey(currentForm, descriptor),
			);
			if (showSuccessToast) {
				toast.success(t("connection_success"));
			}
			return true;
		} catch (e) {
			setValidatedConnectionKey(null);
			if (showFailureError) {
				handleApiError(e);
			}
			return false;
		}
	};

	const cancelConnectorAction = () => {
		setConnectorActionConfirmId(null);
	};

	const setConnectorActionValue = (
		actionId: string,
		fieldName: string,
		value: StorageConnectorFieldValue | undefined,
	) => {
		setConnectorActionValues((current) => {
			const actionValues = { ...(current[actionId] ?? {}) };
			if (value === undefined) {
				delete actionValues[fieldName];
			} else {
				actionValues[fieldName] = value;
			}
			return { ...current, [actionId]: actionValues };
		});
	};

	const executeConnectorAction = async (actionId: StorageConnectorActionId) => {
		const action = findStorageConnectorAction(
			currentStorageDriverDescriptor,
			actionId,
			"custom",
		);
		if (!action || !currentStorageDriverDescriptor) {
			return;
		}
		const values = connectorActionValues[actionId] ?? {};
		const missingField = action.fields?.find((field) => {
			if (!field.required) {
				return false;
			}
			const value = values[field.name] ?? field.default_value;
			return value === undefined || value === "";
		});
		if (missingField) {
			const fieldLabel = connectorT(missingField.label_key);
			toast.error(
				missingField.required_message_key
					? connectorT(missingField.required_message_key, { field: fieldLabel })
					: t("policy_connector_action_field_required", { field: fieldLabel }),
			);
			return;
		}

		setConnectorActionSubmittingId(actionId);
		try {
			const currentForm = syncNormalizedPolicyForm();
			const descriptor = currentStorageDriverDescriptor;
			if (!descriptor || descriptor.connector_id !== currentForm.connector_id) {
				return;
			}
			const valueSource = selectStoragePolicyActionValueSource({
				descriptor,
				editingId,
				editingPolicy,
				form: currentForm,
			});
			const executionMode = selectStorageConnectorCustomActionExecutionMode(
				action,
				valueSource,
				editingId,
			);
			if (executionMode === "save_first") {
				toast.error(t("policy_connector_action_save_first"));
				return;
			}
			if (executionMode === "unsupported") {
				return;
			}
			let result: StoragePolicyActionResult;
			if (executionMode === "draft") {
				const currentEndpointValidationMessage = getEndpointValidationMessage(
					currentForm,
					(key) =>
						translateStorageConnectorMessage(t, descriptor.connector_id, key),
					descriptor,
				);
				if (currentEndpointValidationMessage) {
					toast.error(currentEndpointValidationMessage);
					return;
				}
				result = await adminPolicyService.executeDraftPolicyAction(
					buildStorageConnectorActionPayload(
						currentForm,
						editingId,
						descriptor,
						actionId,
						values,
					),
				);
			} else {
				const savedPolicyId = editingId;
				if (savedPolicyId === null) {
					return;
				}
				result = await adminPolicyService.executeSavedPolicyAction(
					savedPolicyId,
					{
						action_id: actionId,
						values,
					},
				);
			}
			setConnectorActionConfirmId(null);
			const successMessage = t("policy_connector_action_success", {
				action: connectorT(action.label_key),
			});
			const outputDetails = presentStorageConnectorActionOutput(
				action,
				result,
				(key) => connectorT(key),
			);
			if (outputDetails.length === 0) {
				toast.success(successMessage);
			} else {
				toast.success(successMessage, {
					description: outputDetails
						.map(({ label, value }) => `${label}: ${value}`)
						.join(" · "),
				});
			}
		} catch (error) {
			handleApiError(error);
		} finally {
			setConnectorActionSubmittingId(null);
		}
	};

	const requestConnectorAction = (actionId: StorageConnectorActionId) => {
		const action = findStorageConnectorAction(
			currentStorageDriverDescriptor,
			actionId,
			"custom",
		);
		if (!action) {
			return;
		}
		if (action.requires_confirmation) {
			setConnectorActionConfirmId(actionId);
			return;
		}
		void executeConnectorAction(actionId);
	};

	return {
		cancelConnectorAction,
		clearActionConfirms,
		connectorActionConfirmId,
		connectorActionSubmittingId,
		connectorActionValues,
		executeConnectorAction,
		requestConnectorAction,
		resetActionState,
		runConnectionTest,
		setValidatedConnectionKey,
		setConnectorActionValue,
		validatedConnectionKey,
	};
}
