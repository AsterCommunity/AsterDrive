import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
	getEndpointValidationMessage,
	getPolicyConnectionTestKey,
} from "@/components/admin/storage-policy-dialog/connectionNormalization";
import { findStorageConnectorAction } from "@/components/admin/storage-policy-dialog/descriptorPredicates";
import type { PolicyFormData } from "@/components/admin/storage-policy-dialog/formTypes";
import {
	buildPolicyTestPayload,
	buildStorageConnectorActionPayload,
} from "@/components/admin/storage-policy-dialog/payloadBuilders";
import {
	selectStorageConnectorCustomActionExecutionMode,
	selectStoragePolicyActionValueSource,
	selectStoragePolicyConnectionTestMode,
} from "@/components/admin/storage-policy-dialog/policyActionSelection";
import { policyFormHasUnsavedChanges } from "@/components/admin/storage-policy-dialog/policyFormComparison";
import type { StorageConnectorActionValues } from "@/components/admin/storage-policy-dialog/StorageConnectorActionsPanel";
import { handleApiError } from "@/hooks/useApiError";
import { usePendingAction } from "@/hooks/usePendingAction";
import { getStorageConnectorDescriptor } from "@/lib/adminStorageDriverDescriptors";
import { adminPolicyService } from "@/services/adminService";
import type {
	StorageConnectorActionId,
	StorageConnectorCredentialInfo,
	StorageConnectorDescriptor,
	StorageConnectorFieldValue,
	StoragePolicy,
} from "@/types/api";

interface StoragePolicyActionControllerInput {
	currentEditingIdRef: MutableRefObject<number | null>;
	currentStorageDriverDescriptor: StorageConnectorDescriptor | null | undefined;
	editingId: number | null;
	editingPolicy: StoragePolicy | null;
	form: PolicyFormData;
	loadPolicyCapacity: (policyId: number) => void;
	setStorageCredentials: Dispatch<
		SetStateAction<StorageConnectorCredentialInfo[]>
	>;
	storageCredentialValidationRequestSerial: MutableRefObject<number>;
	storageDriverDescriptors: StorageConnectorDescriptor[];
	syncNormalizedPolicyForm: () => PolicyFormData;
}

export function useStoragePolicyActionController({
	currentEditingIdRef,
	currentStorageDriverDescriptor,
	editingId,
	editingPolicy,
	form,
	loadPolicyCapacity,
	setStorageCredentials,
	storageCredentialValidationRequestSerial,
	storageDriverDescriptors,
	syncNormalizedPolicyForm,
}: StoragePolicyActionControllerInput) {
	const { t } = useTranslation("admin");
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
	const {
		pending: storageAuthorizationSubmitting,
		runWithPending: runWithStorageAuthorization,
	} = usePendingAction();
	const {
		pending: storageCredentialValidationSubmitting,
		runWithPending: runWithStorageCredentialValidation,
	} = usePendingAction();

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
			t,
			descriptor,
		);
		if (currentEndpointValidationMessage) {
			if (showFailureError) {
				toast.error(currentEndpointValidationMessage);
			}
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
			toast.error(
				t(
					missingField.required_message_key ??
						"policy_connector_action_field_required",
					{ field: t(missingField.label_key) },
				),
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
			if (executionMode === "draft") {
				const currentEndpointValidationMessage = getEndpointValidationMessage(
					currentForm,
					t,
					descriptor,
				);
				if (currentEndpointValidationMessage) {
					toast.error(currentEndpointValidationMessage);
					return;
				}
				await adminPolicyService.executeDraftPolicyAction(
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
				await adminPolicyService.executeSavedPolicyAction(savedPolicyId, {
					action_id: actionId,
					values,
				});
			}
			setConnectorActionConfirmId(null);
			toast.success(
				t("policy_connector_action_success", {
					action: t(action.label_key),
				}),
			);
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

	const startStorageAuthorization = () => {
		if (
			editingId === null ||
			!editingPolicy ||
			!findStorageConnectorAction(
				currentStorageDriverDescriptor,
				"start_authorization",
				"authorization",
			)
		) {
			return;
		}
		if (
			policyFormHasUnsavedChanges(
				form,
				editingPolicy,
				currentStorageDriverDescriptor,
			)
		) {
			toast.error(t("onedrive_save_before_authorize"));
			return;
		}
		void runWithStorageAuthorization(async () => {
			try {
				const result =
					await adminPolicyService.startStorageAuthorization(editingId);
				toast.success(t("onedrive_authorization_started"));
				const opened = window.open(result.authorization_url, "_blank");
				if (opened) {
					opened.opener = null;
				} else {
					window.location.assign(result.authorization_url);
				}
			} catch (error) {
				handleApiError(error);
			}
		});
	};

	const validateStorageCredential = () => {
		if (
			editingId === null ||
			!findStorageConnectorAction(
				currentStorageDriverDescriptor,
				"validate_credential",
				"credential_validation",
			)
		) {
			return;
		}
		if (
			policyFormHasUnsavedChanges(
				form,
				editingPolicy,
				currentStorageDriverDescriptor,
			)
		) {
			toast.error(t("onedrive_save_before_validate"));
			return;
		}

		const policyId = editingId;
		const validationRequestSerial =
			++storageCredentialValidationRequestSerial.current;

		void runWithStorageCredentialValidation(async () => {
			try {
				const isCurrentValidationRequest = () =>
					validationRequestSerial ===
						storageCredentialValidationRequestSerial.current &&
					policyId === currentEditingIdRef.current;
				if (!isCurrentValidationRequest()) {
					return;
				}

				const result =
					await adminPolicyService.validateStorageCredential(policyId);
				if (isCurrentValidationRequest()) {
					setStorageCredentials((prev) => {
						const nextCredential = result.credential;
						const hasExisting = prev.some(
							(credential) => credential.provider === nextCredential.provider,
						);
						return hasExisting
							? prev.map((credential) =>
									credential.provider === nextCredential.provider
										? nextCredential
										: credential,
								)
							: [nextCredential, ...prev];
					});
					loadPolicyCapacity(policyId);
					toast.success(t("onedrive_validation_success"), {
						description: result.root_item_name
							? t("onedrive_validation_success_root", {
									name: result.root_item_name,
								})
							: undefined,
					});
				}
			} catch (error) {
				if (
					validationRequestSerial ===
						storageCredentialValidationRequestSerial.current &&
					policyId === currentEditingIdRef.current
				) {
					handleApiError(error);
				}
			}
		});
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
		startStorageAuthorization,
		storageAuthorizationSubmitting,
		storageCredentialValidationSubmitting,
		validateStorageCredential,
		validatedConnectionKey,
	};
}
