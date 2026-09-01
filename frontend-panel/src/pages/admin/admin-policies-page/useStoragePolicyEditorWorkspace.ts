import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
	getEndpointValidationMessage,
	normalizePolicyForm,
} from "@/components/admin/storage-policy-editor/connectionNormalization";
import {
	emptyForm,
	getPolicyForm,
	type PolicyFormData,
} from "@/components/admin/storage-policy-editor/formTypes";
import {
	applyPolicyConnectorTransition,
	applyPolicyFormFieldChange,
} from "@/components/admin/storage-policy-editor/policyFormTransition";
import { config } from "@/config/app";
import { handleApiError } from "@/hooks/useApiError";
import { translateStorageConnectorMessage } from "@/lib/adminStorageConnectorLocalizations";
import { getStorageConnectorDescriptor } from "@/lib/adminStorageDriverDescriptors";
import { adminPolicyService } from "@/services/adminService";
import type {
	StorageConnectorFieldValue,
	StoragePolicy,
	StoragePolicyCapacityInfo,
} from "@/types/api";
import { useStoragePolicyActionController } from "./useStoragePolicyActionController";
import { useStoragePolicyCredentialController } from "./useStoragePolicyCredentialController";
import { useStoragePolicyDescriptorController } from "./useStoragePolicyDescriptorController";
import { useStoragePolicyEditorController } from "./useStoragePolicyEditorController";
import { useStoragePolicyPromotionController } from "./useStoragePolicyPromotionController";

export type StoragePolicyEditorVariant = "admin" | "setup";

interface UseStoragePolicyEditorWorkspaceArgs {
	variant: StoragePolicyEditorVariant;
	policyId: number | null;
	onExit: () => void;
	onPolicyCreated?: (policy: StoragePolicy) => Promise<void> | void;
	onEnterCredentialSetup?: (policyId: number) => void;
}

function getStorageAuthorizationCallbackUrl() {
	const apiBaseUrl = new URL(config.apiBaseUrl, window.location.origin);
	return new URL(
		"admin/policies/storage-authorization/callback",
		apiBaseUrl.href.endsWith("/") ? apiBaseUrl.href : `${apiBaseUrl.href}/`,
	).toString();
}

export function useStoragePolicyEditorWorkspace({
	variant,
	policyId,
	onExit,
	onPolicyCreated,
	onEnterCredentialSetup,
}: UseStoragePolicyEditorWorkspaceArgs) {
	const { t } = useTranslation("admin");
	const setupMode = variant === "setup";
	const editingId = policyId;
	const [editingPolicy, setEditingPolicy] = useState<StoragePolicy | null>(
		null,
	);
	const [policyNotFound, setPolicyNotFound] = useState(false);
	const [policyLoading, setPolicyLoading] = useState(policyId !== null);
	const [policyCapacity, setPolicyCapacity] =
		useState<StoragePolicyCapacityInfo | null>(null);
	const [policyCapacityLoading, setPolicyCapacityLoading] = useState(false);
	const policyCapacityRequestSerial = useRef(0);
	const [form, setForm] = useState<PolicyFormData>(() =>
		setupMode ? { ...emptyForm, is_default: true } : emptyForm,
	);
	const [submitting, setSubmitting] = useState(false);
	const [saveAnywayConfirmOpen, setSaveAnywayConfirmOpen] = useState(false);
	const [createStep, setCreateStep] = useState(0);
	const [createStepTouched, setCreateStepTouched] = useState(false);

	const descriptorController = useStoragePolicyDescriptorController({
		form,
		setForm,
		setupMode,
	});

	// 创建模式下表单先于驱动描述符就绪；描述符到达后给未选择驱动的表单
	// 补上第一个可创建驱动（避免覆盖用户手动选择）。
	useEffect(() => {
		if (editingId !== null) {
			return;
		}
		const creatableDescriptors =
			descriptorController.creatableStorageDriverDescriptors;
		if (
			creatableDescriptors.length === 0 ||
			creatableDescriptors.some(
				(descriptor) => descriptor.connector_id === form.connector_id,
			)
		) {
			return;
		}

		const firstDescriptor = creatableDescriptors[0];
		setForm((current) => {
			const transitioned = applyPolicyConnectorTransition(
				current,
				firstDescriptor.connector_id,
				firstDescriptor,
			);
			return setupMode ? { ...transitioned, is_default: true } : transitioned;
		});
	}, [
		descriptorController.creatableStorageDriverDescriptors,
		editingId,
		form.connector_id,
		setupMode,
	]);

	const loadPolicyCapacity = useCallback((nextPolicyId: number) => {
		const capacityRequestSerial = ++policyCapacityRequestSerial.current;
		setPolicyCapacityLoading(true);
		void adminPolicyService
			.getCapacity(nextPolicyId)
			.then((capacity) => {
				if (capacityRequestSerial === policyCapacityRequestSerial.current) {
					setPolicyCapacity(capacity);
				}
			})
			.catch((error) => {
				if (capacityRequestSerial === policyCapacityRequestSerial.current) {
					handleApiError(error);
					setPolicyCapacity(null);
				}
			})
			.finally(() => {
				if (capacityRequestSerial === policyCapacityRequestSerial.current) {
					setPolicyCapacityLoading(false);
				}
			});
	}, []);

	// 编辑器激活即预热远端节点 lookup（创建模式的驱动字段与编辑模式共用）。
	useEffect(() => {
		void descriptorController.refreshRemoteNodeLookup();
	}, [descriptorController.refreshRemoteNodeLookup]);

	// 编辑模式按路由 policyId 加载策略并填充表单。
	useEffect(() => {
		if (editingId === null) {
			return;
		}

		let cancelled = false;
		setPolicyLoading(true);
		adminPolicyService
			.get(editingId)
			.then((policy) => {
				if (cancelled) return;
				setEditingPolicy(policy);
				setForm(getPolicyForm(policy));
				loadPolicyCapacity(policy.id);
			})
			.catch((error) => {
				if (!cancelled) {
					setPolicyNotFound(true);
					handleApiError(error);
				}
			})
			.finally(() => {
				if (!cancelled) {
					setPolicyLoading(false);
				}
			});

		return () => {
			cancelled = true;
		};
	}, [editingId, loadPolicyCapacity]);

	const currentStorageDriverDescriptor =
		descriptorController.currentStorageDriverDescriptor;
	const endpointValidationMessage = getEndpointValidationMessage(
		form,
		(key) =>
			translateStorageConnectorMessage(
				t,
				currentStorageDriverDescriptor?.connector_id,
				key,
			),
		currentStorageDriverDescriptor,
	);
	const storageAuthorizationRedirectUri = getStorageAuthorizationCallbackUrl();

	function syncNormalizedPolicyForm() {
		const descriptor = getStorageConnectorDescriptor(
			descriptorController.storageDriverDescriptors,
			form.connector_id,
		);
		const normalizedForm = normalizePolicyForm(
			setupMode ? { ...form, is_default: true } : form,
			descriptor,
		);
		if (normalizedForm !== form) {
			setForm(normalizedForm);
		}
		return normalizedForm;
	}

	const actionController = useStoragePolicyActionController({
		currentStorageDriverDescriptor,
		editingId,
		editingPolicy,
		storageDriverDescriptors: descriptorController.storageDriverDescriptors,
		syncNormalizedPolicyForm,
	});
	const credentialController = useStoragePolicyCredentialController({
		currentStorageDriverDescriptor,
		editingPolicy,
		form,
		loadPolicyCapacity,
	});
	const setConnectorActionValue = useCallback(
		(
			actionId: string,
			fieldName: string,
			value: StorageConnectorFieldValue | undefined,
		) => {
			actionController.setConnectorActionValue(actionId, fieldName, value);
			const action = currentStorageDriverDescriptor?.actions.find(
				(candidate) => candidate.action_id === actionId,
			);
			const controlsRemoteTargets = action?.fields?.some(
				(field) =>
					field.select?.data_source === "remote_storage_targets" &&
					field.select.depends_on === fieldName,
			);
			if (!controlsRemoteTargets) {
				return;
			}
			if (
				typeof value === "number" &&
				Number.isSafeInteger(value) &&
				value > 0
			) {
				void descriptorController.loadRemoteStorageTargetsForPolicy(value, {
					showErrorToast: false,
					syncPolicySelection: false,
				});
			} else {
				descriptorController.resetRemoteStorageTargets();
			}
		},
		[actionController, currentStorageDriverDescriptor, descriptorController],
	);
	const editorController = useStoragePolicyEditorController({
		allowSaveWithoutConnectionTest: !setupMode,
		currentStorageDriverDescriptor,
		createStep,
		editingId,
		editingPolicy,
		endpointValidationMessage,
		form,
		onExit,
		onPolicyCreated,
		onEnterCredentialSetup,
		onUpdated: (updated) => {
			setEditingPolicy(updated);
			setForm(getPolicyForm(updated));
			loadPolicyCapacity(updated.id);
		},
		setCreateStep,
		setCreateStepTouched,
		setSaveAnywayConfirmOpen,
		setSubmitting,
		storageDriverDescriptors: descriptorController.storageDriverDescriptors,
		submitting,
		syncNormalizedPolicyForm,
	});
	const promotionController = useStoragePolicyPromotionController({
		currentDescriptor: currentStorageDriverDescriptor,
		editingId,
		editingPolicy,
		form,
		loadPolicyCapacity,
		onDraftApplied: () => {
			actionController.resetActionState();
			credentialController.reset();
			setSaveAnywayConfirmOpen(false);
			setCreateStepTouched(false);
		},
		onPromoted: () => {
			actionController.resetActionState();
			credentialController.reset();
			setSaveAnywayConfirmOpen(false);
		},
		setEditingPolicy,
		setForm,
		storageDriverDescriptors:
			editingId !== null
				? descriptorController.storageDriverDescriptors
				: descriptorController.creatableStorageDriverDescriptors,
	});

	const setField = <K extends keyof PolicyFormData>(
		key: K,
		value: PolicyFormData[K],
	) => {
		setSaveAnywayConfirmOpen(false);
		actionController.clearActionConfirms();
		setForm((prev) => {
			const next = applyPolicyFormFieldChange(prev, key, value);
			return setupMode ? { ...next, is_default: true } : next;
		});
	};

	const setConnectorId = (connectorId: string) => {
		setSaveAnywayConfirmOpen(false);
		actionController.resetActionState();
		setCreateStepTouched(false);
		setForm((prev) => {
			const nextDriverDescriptor = getStorageConnectorDescriptor(
				descriptorController.storageDriverDescriptors,
				connectorId,
			);
			const next = applyPolicyConnectorTransition(
				prev,
				connectorId,
				nextDriverDescriptor,
			);
			return setupMode ? { ...next, is_default: true } : next;
		});
	};

	return {
		actionController,
		confirmConnectorAction: (actionId: string) => {
			setSaveAnywayConfirmOpen(false);
			void actionController.executeConnectorAction(actionId);
		},
		confirmSaveAnyway: () =>
			editorController.confirmSaveAnyway(actionController),
		createStep,
		createStepTouched,
		credentialController,
		descriptorController,
		editingPolicy,
		editorController,
		endpointValidationMessage,
		form,
		isCreateMode: editingId === null,
		policyCapacity,
		policyCapacityLoading,
		policyLoading,
		policyNotFound,
		promotionController,
		requestConnectorAction: (actionId: string) => {
			setSaveAnywayConfirmOpen(false);
			actionController.requestConnectorAction(actionId);
		},
		runConnectionTest: () => actionController.runConnectionTest(),
		saveAnywayConfirmOpen,
		setupMode,
		setConnectorActionValue,
		setConnectorId,
		setField,
		storageAuthorizationRedirectUri,
		submit: () => editorController.handleSubmit(actionController),
		submitting,
	};
}
