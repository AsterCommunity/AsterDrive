import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
	findStorageConnectorAction,
	supportsStorageCredentialLifecycle,
} from "@/components/admin/storage-policy-editor/descriptorPredicates";
import type { PolicyFormData } from "@/components/admin/storage-policy-editor/formTypes";
import { policyFormHasUnsavedChanges } from "@/components/admin/storage-policy-editor/policyFormComparison";
import { handleApiError } from "@/hooks/useApiError";
import { translateStorageConnectorMessage } from "@/lib/adminStorageConnectorLocalizations";
import { adminPolicyService } from "@/services/adminService";
import type {
	StorageConnectorCredentialInfo,
	StorageConnectorDescriptor,
	StoragePolicy,
} from "@/types/api";

interface StoragePolicyCredentialControllerInput {
	currentStorageDriverDescriptor: StorageConnectorDescriptor | null | undefined;
	editingPolicy: StoragePolicy | null;
	form: PolicyFormData;
	loadPolicyCapacity: (policyId: number) => void;
}

interface CredentialSession {
	generation: number;
	key: string;
	policyId: number;
}

function upsertCredential(
	credentials: StorageConnectorCredentialInfo[],
	nextCredential: StorageConnectorCredentialInfo,
) {
	const hasExisting = credentials.some(
		(credential) => credential.provider === nextCredential.provider,
	);
	return hasExisting
		? credentials.map((credential) =>
				credential.provider === nextCredential.provider
					? nextCredential
					: credential,
			)
		: [nextCredential, ...credentials];
}

/**
 * Owns the credential resource and credential actions for one saved-policy
 * dialog session.
 *
 * A session is identified by policy ID, and the saved
 * connector ID rather than the `StoragePolicy` object identity. Switching policies/connectors invalidates the generation so late
 * list, authorization, validation, and `finally` completions cannot mutate a
 * newer session. Validation failures reload the writer-backed credential
 * status because the backend may persist an expired or reauthorization state
 * before returning the provider error.
 */
export function useStoragePolicyCredentialController({
	currentStorageDriverDescriptor,
	editingPolicy,
	form,
	loadPolicyCapacity,
}: StoragePolicyCredentialControllerInput) {
	const { t } = useTranslation("admin");
	const [credentials, setCredentials] = useState<
		StorageConnectorCredentialInfo[]
	>([]);
	const [loading, setLoading] = useState(false);
	const [authorizationSubmitting, setAuthorizationSubmitting] = useState(false);
	const [validationSubmitting, setValidationSubmitting] = useState(false);
	const generationRef = useRef(0);
	const listRequestSerialRef = useRef(0);
	const authorizationRequestSerialRef = useRef(0);
	const validationRequestSerialRef = useRef(0);
	const authorizationPendingRef = useRef(false);
	const validationPendingRef = useRef(false);
	const sessionRef = useRef<CredentialSession | null>(null);

	const editingPolicyId = editingPolicy?.id ?? null;
	const savedConnectorId = editingPolicy?.connector_id ?? null;
	const displayedConnectorId =
		currentStorageDriverDescriptor?.connector_id ?? null;
	const credentialManagement =
		currentStorageDriverDescriptor?.credential_management;
	const hasCredentialLifecycle = supportsStorageCredentialLifecycle(
		currentStorageDriverDescriptor,
	);
	const activeSessionKey =
		editingPolicyId !== null &&
		savedConnectorId === displayedConnectorId &&
		hasCredentialLifecycle
			? `${editingPolicyId}:${savedConnectorId}`
			: null;
	const renderedSessionKeyRef = useRef<string | null>(activeSessionKey);
	renderedSessionKeyRef.current = activeSessionKey;

	const isCurrentSession = useCallback((session: CredentialSession) => {
		const current = sessionRef.current;
		return (
			current?.generation === session.generation &&
			current.key === session.key &&
			renderedSessionKeyRef.current === session.key
		);
	}, []);

	const invalidateSession = useCallback(() => {
		generationRef.current += 1;
		listRequestSerialRef.current += 1;
		authorizationRequestSerialRef.current += 1;
		validationRequestSerialRef.current += 1;
		authorizationPendingRef.current = false;
		validationPendingRef.current = false;
		sessionRef.current = null;
	}, []);

	const loadCredentials = useCallback(
		async (session: CredentialSession) => {
			const requestSerial = ++listRequestSerialRef.current;
			if (isCurrentSession(session)) {
				setLoading(true);
			}
			try {
				const nextCredentials = await adminPolicyService.listStorageCredentials(
					session.policyId,
				);
				if (
					requestSerial === listRequestSerialRef.current &&
					isCurrentSession(session)
				) {
					setCredentials(nextCredentials);
				}
			} catch (error) {
				if (
					requestSerial === listRequestSerialRef.current &&
					isCurrentSession(session)
				) {
					handleApiError(error);
					setCredentials([]);
				}
			} finally {
				if (
					requestSerial === listRequestSerialRef.current &&
					isCurrentSession(session)
				) {
					setLoading(false);
				}
			}
		},
		[isCurrentSession],
	);

	useEffect(() => {
		invalidateSession();
		setCredentials([]);
		setLoading(false);
		setAuthorizationSubmitting(false);
		setValidationSubmitting(false);

		if (!activeSessionKey || editingPolicyId === null) {
			return;
		}

		const session = {
			generation: generationRef.current,
			key: activeSessionKey,
			policyId: editingPolicyId,
		};
		sessionRef.current = session;
		void loadCredentials(session);

		return () => {
			if (sessionRef.current?.generation === session.generation) {
				invalidateSession();
			}
		};
	}, [activeSessionKey, editingPolicyId, invalidateSession, loadCredentials]);

	const reset = useCallback(() => {
		invalidateSession();
		setCredentials([]);
		setLoading(false);
		setAuthorizationSubmitting(false);
		setValidationSubmitting(false);
	}, [invalidateSession]);

	const connectorT = useCallback(
		(key: string, values?: Record<string, number | string>) =>
			translateStorageConnectorMessage(
				t,
				currentStorageDriverDescriptor?.connector_id,
				key,
				values,
			),
		[currentStorageDriverDescriptor?.connector_id, t],
	);

	const startAuthorization = useCallback(() => {
		const session = sessionRef.current;
		const action = findStorageConnectorAction(
			currentStorageDriverDescriptor,
			"start_authorization",
			"authorization",
		);
		if (
			!session ||
			!editingPolicy ||
			!action ||
			!isCurrentSession(session) ||
			authorizationPendingRef.current
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
			toast.error(
				credentialManagement?.save_before_authorize_key
					? connectorT(credentialManagement.save_before_authorize_key)
					: t("policy_connector_action_save_first"),
			);
			return;
		}

		const requestSerial = ++authorizationRequestSerialRef.current;
		authorizationPendingRef.current = true;
		setAuthorizationSubmitting(true);
		void adminPolicyService
			.startStorageAuthorization(session.policyId)
			.then((result) => {
				if (
					requestSerial !== authorizationRequestSerialRef.current ||
					!isCurrentSession(session)
				) {
					return;
				}
				toast.success(
					credentialManagement?.authorization_started_key
						? connectorT(credentialManagement.authorization_started_key)
						: t("policy_connector_action_success", {
								action: connectorT(action.label_key),
							}),
				);
				const opened = window.open(result.authorization_url, "_blank");
				if (opened) {
					opened.opener = null;
				} else {
					window.location.assign(result.authorization_url);
				}
			})
			.catch((error) => {
				if (
					requestSerial === authorizationRequestSerialRef.current &&
					isCurrentSession(session)
				) {
					handleApiError(error);
				}
			})
			.finally(() => {
				if (
					requestSerial === authorizationRequestSerialRef.current &&
					isCurrentSession(session)
				) {
					authorizationPendingRef.current = false;
					setAuthorizationSubmitting(false);
				}
			});
	}, [
		connectorT,
		credentialManagement,
		currentStorageDriverDescriptor,
		editingPolicy,
		form,
		isCurrentSession,
		t,
	]);

	const validate = useCallback(() => {
		const session = sessionRef.current;
		const action = findStorageConnectorAction(
			currentStorageDriverDescriptor,
			"validate_credential",
			"credential_validation",
		);
		if (
			!session ||
			!editingPolicy ||
			!action ||
			!isCurrentSession(session) ||
			validationPendingRef.current
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
			toast.error(
				credentialManagement?.save_before_validate_key
					? connectorT(credentialManagement.save_before_validate_key)
					: t("policy_connector_action_save_first"),
			);
			return;
		}

		const requestSerial = ++validationRequestSerialRef.current;
		listRequestSerialRef.current += 1;
		setLoading(false);
		validationPendingRef.current = true;
		setValidationSubmitting(true);
		void adminPolicyService
			.validateStorageCredential(session.policyId)
			.then((result) => {
				if (
					requestSerial !== validationRequestSerialRef.current ||
					!isCurrentSession(session)
				) {
					return;
				}
				setCredentials((current) =>
					upsertCredential(current, result.credential),
				);
				loadPolicyCapacity(session.policyId);
				toast.success(
					credentialManagement?.validation_success_key
						? connectorT(credentialManagement.validation_success_key)
						: t("policy_connector_action_success", {
								action: connectorT(action.label_key),
							}),
					{
						description: result.root_item_name
							? credentialManagement?.validation_success_detail_key
								? connectorT(
										credentialManagement.validation_success_detail_key,
										{ name: result.root_item_name },
									)
								: undefined
							: undefined,
					},
				);
			})
			.catch(async (error) => {
				if (
					requestSerial !== validationRequestSerialRef.current ||
					!isCurrentSession(session)
				) {
					return;
				}
				handleApiError(error);
				await loadCredentials(session);
			})
			.finally(() => {
				if (
					requestSerial === validationRequestSerialRef.current &&
					isCurrentSession(session)
				) {
					validationPendingRef.current = false;
					setValidationSubmitting(false);
				}
			});
	}, [
		connectorT,
		credentialManagement,
		currentStorageDriverDescriptor,
		editingPolicy,
		form,
		isCurrentSession,
		loadCredentials,
		loadPolicyCapacity,
		t,
	]);

	return {
		authorizationSubmitting,
		credentials,
		loading,
		reset,
		startAuthorization,
		validate,
		validationSubmitting,
	};
}
