import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
	getEndpointValidationMessage,
	getPolicyConnectionTestKey,
	getS3CompatibleDriverPromotionTarget,
	hasConnectionFieldChanges,
} from "@/components/admin/storage-policy-dialog/connectionNormalization";
import {
	supportsObjectStorageConnection,
	supportsRemoteNodeBinding,
	supportsStoragePolicyAction,
} from "@/components/admin/storage-policy-dialog/descriptorPredicates";
import {
	getPolicyForm,
	type PolicyFormData,
} from "@/components/admin/storage-policy-dialog/formTypes";
import { MICROSOFT_GRAPH_PROVIDER } from "@/components/admin/storage-policy-dialog/onedriveFieldUtils";
import {
	buildPolicyTestPayload,
	buildTencentCosCorsPayload,
} from "@/components/admin/storage-policy-dialog/payloadBuilders";
import {
	selectStoragePolicyActionValueSource,
	selectStoragePolicyConnectionTestMode,
} from "@/components/admin/storage-policy-dialog/policyActionSelection";
import { policyFormHasUnsavedChanges } from "@/components/admin/storage-policy-dialog/policyFormComparison";
import { handleApiError } from "@/hooks/useApiError";
import { usePendingAction } from "@/hooks/usePendingAction";
import { invalidateAdminPolicyLookup } from "@/lib/adminPolicyLookup";
import { getStorageDriverDescriptor } from "@/lib/adminStorageDriverDescriptors";
import { adminPolicyService } from "@/services/adminService";
import type {
	DriverType,
	StorageConnectorDescriptor,
	StoragePolicy,
	StoragePolicyCapacityInfo,
	StoragePolicyCredentialInfo,
} from "@/types/api";

interface StoragePolicyActionControllerInput {
	currentEditingIdRef: MutableRefObject<number | null>;
	currentStorageDriverDescriptor: StorageConnectorDescriptor | null | undefined;
	editingId: number | null;
	editingPolicy: StoragePolicy | null;
	form: PolicyFormData;
	loadPolicyCapacity: (policyId: number) => void;
	setEditingId: Dispatch<SetStateAction<number | null>>;
	setEditingPolicy: Dispatch<SetStateAction<StoragePolicy | null>>;
	setForm: Dispatch<SetStateAction<PolicyFormData>>;
	setPolicies: Dispatch<SetStateAction<StoragePolicy[]>>;
	setPolicyCapacity: Dispatch<SetStateAction<StoragePolicyCapacityInfo | null>>;
	setStorageCredentials: Dispatch<
		SetStateAction<StoragePolicyCredentialInfo[]>
	>;
	storageCredentialValidationRequestSerial: MutableRefObject<number>;
	storageDriverDescriptors: StorageConnectorDescriptor[];
	syncNormalizedPolicyForm: () => PolicyFormData;
	onDriverSuggestionApply: (driverType: DriverType) => void;
}

export function useStoragePolicyActionController({
	currentEditingIdRef,
	currentStorageDriverDescriptor,
	editingId,
	editingPolicy,
	form,
	loadPolicyCapacity,
	onDriverSuggestionApply,
	setEditingId,
	setEditingPolicy,
	setForm,
	setPolicies,
	setPolicyCapacity,
	setStorageCredentials,
	storageCredentialValidationRequestSerial,
	storageDriverDescriptors,
	syncNormalizedPolicyForm,
}: StoragePolicyActionControllerInput) {
	const { t } = useTranslation("admin");
	const [cosCorsConfirmOpen, setCosCorsConfirmOpen] = useState(false);
	const [s3DriverPromotionConfirmOpen, setS3DriverPromotionConfirmOpen] =
		useState(false);
	const [validatedConnectionKey, setValidatedConnectionKey] = useState<
		string | null
	>(null);
	const {
		pending: s3DriverPromotionSubmitting,
		runWithPending: runWithS3DriverPromotion,
	} = usePendingAction();
	const {
		pending: cosCorsSubmitting,
		runWithPending: runWithCosCorsConfigure,
	} = usePendingAction();
	const {
		pending: storageAuthorizationSubmitting,
		runWithPending: runWithStorageAuthorization,
	} = usePendingAction();
	const {
		pending: storageCredentialValidationSubmitting,
		runWithPending: runWithStorageCredentialValidation,
	} = usePendingAction();

	const canConfigureTencentCosCors = supportsStoragePolicyAction(
		currentStorageDriverDescriptor,
		"configure_tencent_cos_cors",
	);
	const currentAuthorizationProvider =
		currentStorageDriverDescriptor?.authorization_provider ?? null;
	const isMicrosoftGraphAuthorizationProvider =
		currentAuthorizationProvider === MICROSOFT_GRAPH_PROVIDER;
	const getS3CompatiblePromotionDriverLabel = (driverType: DriverType) => {
		const descriptor = getStorageDriverDescriptor(
			storageDriverDescriptors,
			driverType,
		);
		return descriptor?.ui ? t(descriptor.ui.label_key) : driverType;
	};
	const savedS3DriverPromotionTarget = getS3CompatibleDriverPromotionTarget(
		editingPolicy,
		getStorageDriverDescriptor(
			storageDriverDescriptors,
			editingPolicy?.driver_type ?? form.driver_type,
		),
		getS3CompatiblePromotionDriverLabel,
	);
	const draftS3DriverPromotionTarget = getS3CompatibleDriverPromotionTarget(
		editingId !== null
			? { driver_type: form.driver_type, endpoint: form.endpoint }
			: null,
		currentStorageDriverDescriptor,
		getS3CompatiblePromotionDriverLabel,
	);
	const s3DriverPromotionTarget =
		draftS3DriverPromotionTarget ?? savedS3DriverPromotionTarget;
	const s3CompatibleDriverSuggestionTarget =
		getS3CompatibleDriverPromotionTarget(
			{ driver_type: form.driver_type, endpoint: form.endpoint },
			currentStorageDriverDescriptor,
			getS3CompatiblePromotionDriverLabel,
		);
	const s3DriverPromotionBlocked =
		s3DriverPromotionTarget != null &&
		policyFormHasUnsavedChanges(
			form,
			editingPolicy,
			currentStorageDriverDescriptor,
		);
	const cosCorsUsesDraftValues =
		editingId === null ||
		hasConnectionFieldChanges(
			form,
			editingPolicy,
			currentStorageDriverDescriptor,
		);

	const clearActionConfirms = () => {
		setS3DriverPromotionConfirmOpen(false);
		setCosCorsConfirmOpen(false);
	};

	const resetActionState = () => {
		clearActionConfirms();
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
		const descriptor = getStorageDriverDescriptor(
			storageDriverDescriptors,
			currentForm.driver_type,
		);
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

			if (
				supportsObjectStorageConnection(descriptor) ||
				supportsRemoteNodeBinding(descriptor)
			) {
				setValidatedConnectionKey(
					getPolicyConnectionTestKey(currentForm, descriptor),
				);
			}
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

	const requestS3DriverPromotion = () => {
		if (!savedS3DriverPromotionTarget || s3DriverPromotionBlocked) {
			return;
		}
		setS3DriverPromotionConfirmOpen(true);
	};

	const cancelS3DriverPromotion = () => {
		setS3DriverPromotionConfirmOpen(false);
	};

	const cancelCosCorsConfigure = () => {
		setCosCorsConfirmOpen(false);
	};

	const requestOrConfirmCosCorsConfigure = () => {
		if (cosCorsConfirmOpen) {
			void configureTencentCosCors();
			return;
		}
		setCosCorsConfirmOpen(true);
	};

	const configureTencentCosCors = async () => {
		if (!canConfigureTencentCosCors) {
			return;
		}

		await runWithCosCorsConfigure(async () => {
			try {
				const currentForm = syncNormalizedPolicyForm();
				const descriptor = getStorageDriverDescriptor(
					storageDriverDescriptors,
					currentForm.driver_type,
				);
				const currentEndpointValidationMessage = getEndpointValidationMessage(
					currentForm,
					t,
					descriptor,
				);
				if (currentEndpointValidationMessage) {
					toast.error(currentEndpointValidationMessage);
					return;
				}

				const shouldUseDraft =
					selectStoragePolicyActionValueSource({
						descriptor,
						editingId,
						editingPolicy,
						form: currentForm,
					}) === "draft";
				const result =
					editingId !== null && !shouldUseDraft
						? await adminPolicyService.executeSavedPolicyAction(editingId, {
								action: "configure_tencent_cos_cors",
							})
						: await adminPolicyService.executeDraftPolicyAction(
								buildTencentCosCorsPayload(currentForm, editingId, descriptor),
							);
				const requestId = result.tencent_cos_cors?.request_id;
				setCosCorsConfirmOpen(false);
				toast.success(t("policy_cos_cors_success"), {
					description: requestId
						? t("policy_cos_cors_success_request_id", {
								requestId,
							})
						: undefined,
				});
			} catch (error) {
				handleApiError(error);
			}
		});
	};

	const confirmS3DriverPromotion = () => {
		if (
			!editingPolicy ||
			!savedS3DriverPromotionTarget ||
			s3DriverPromotionBlocked
		) {
			return;
		}

		void runWithS3DriverPromotion(async () => {
			try {
				const updated = await adminPolicyService.promoteS3CompatibleDriver(
					editingPolicy.id,
					{
						target_driver_type: savedS3DriverPromotionTarget.driverType,
						endpoint: editingPolicy.endpoint,
						bucket: editingPolicy.bucket,
					},
				);
				setS3DriverPromotionConfirmOpen(false);
				setEditingId(updated.id);
				setEditingPolicy(updated);
				setForm(getPolicyForm(updated));
				setPolicies((prev) =>
					prev.map((policy) => (policy.id === updated.id ? updated : policy)),
				);
				setPolicyCapacity((prev) =>
					prev == null ? prev : { ...prev, driver_type: updated.driver_type },
				);
				invalidateAdminPolicyLookup();
				toast.success(
					t("policy_s3_driver_promotion_success", {
						driver: savedS3DriverPromotionTarget.driverLabel,
					}),
				);
			} catch (error) {
				handleApiError(error);
			}
		});
	};

	const applyS3CompatibleDriverSuggestion = () => {
		if (!s3CompatibleDriverSuggestionTarget) {
			return;
		}
		onDriverSuggestionApply(s3CompatibleDriverSuggestionTarget.driverType);
	};

	const startStorageAuthorization = () => {
		if (
			editingId === null ||
			!editingPolicy ||
			!isMicrosoftGraphAuthorizationProvider
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
				const result = await adminPolicyService.startStorageAuthorization(
					editingId,
					{
						provider: MICROSOFT_GRAPH_PROVIDER,
					},
				);
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
		if (editingId === null || !isMicrosoftGraphAuthorizationProvider) {
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

				const result = await adminPolicyService.validateStorageCredential(
					policyId,
					MICROSOFT_GRAPH_PROVIDER,
				);
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
		applyS3CompatibleDriverSuggestion,
		canConfigureTencentCosCors,
		cancelCosCorsConfigure,
		cancelS3DriverPromotion,
		clearActionConfirms,
		confirmS3DriverPromotion,
		cosCorsConfirmOpen,
		cosCorsSubmitting,
		cosCorsUsesDraftValues,
		requestOrConfirmCosCorsConfigure,
		requestS3DriverPromotion,
		resetActionState,
		runConnectionTest,
		s3CompatibleDriverSuggestionTargetLabel:
			s3CompatibleDriverSuggestionTarget?.driverLabel ?? null,
		s3DriverPromotionBlocked,
		s3DriverPromotionConfirmOpen,
		s3DriverPromotionSubmitting,
		s3DriverPromotionTargetLabel: s3DriverPromotionTarget?.driverLabel ?? null,
		setValidatedConnectionKey,
		startStorageAuthorization,
		storageAuthorizationSubmitting,
		storageCredentialValidationSubmitting,
		validateStorageCredential,
		validatedConnectionKey,
	};
}
