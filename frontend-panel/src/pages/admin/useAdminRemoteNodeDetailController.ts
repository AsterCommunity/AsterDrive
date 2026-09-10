import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { hasCompletedRemoteNodeEnrollment } from "@/components/admin/admin-remote-nodes-page/shared";
import {
	buildUpdateRemoteNodePayload,
	getRemoteNodeBaseUrlValidationMessage,
	getRemoteNodeForm,
	type RemoteNodeFormData,
} from "@/components/admin/remoteNodePageShared";
import { getApiErrorMessage, handleApiError } from "@/hooks/useApiError";
import { invalidateAdminRemoteNodeLookup } from "@/lib/adminRemoteNodeLookup";
import { installStorageConnectorLocalizations } from "@/lib/adminStorageConnectorLocalizations";
import { writeTextToClipboard } from "@/lib/clipboard";
import { logger } from "@/lib/logger";
import { adminRemoteNodeService } from "@/services/adminService";
import type {
	RemoteCreateStorageTargetRequest,
	RemoteEnrollmentCommandInfo,
	RemoteNodeInfo,
	RemoteStorageTargetInfo,
	RemoteUpdateStorageTargetRequest,
	StorageConnectorDescriptor,
} from "@/types/api";

export function useAdminRemoteNodeDetailController(
	remoteNodeId: number,
	pageTab: "overview" | "storage-targets" = "overview",
) {
	const { i18n, t } = useTranslation("admin");
	const navigate = useNavigate();
	const copyToClipboard = async (value: string) => {
		try {
			await writeTextToClipboard(value);
			toast.success(t("core:copied_to_clipboard"));
		} catch {
			toast.error(t("errors:unexpected_error"));
		}
	};
	const [node, setNode] = useState<RemoteNodeInfo | null>(null);
	const [form, setForm] = useState<RemoteNodeFormData | null>(null);
	const [loading, setLoading] = useState(true);
	const [submitting, setSubmitting] = useState(false);
	const [enrollmentCommand, setEnrollmentCommand] =
		useState<RemoteEnrollmentCommandInfo | null>(null);
	const [enrollmentCommandLoading, setEnrollmentCommandLoading] =
		useState(false);
	const [enrollmentCommandError, setEnrollmentCommandError] = useState<
		string | null
	>(null);
	const [remoteStorageTargets, setRemoteStorageTargets] = useState<
		RemoteStorageTargetInfo[]
	>([]);
	const [remoteStorageTargetsLoading, setRemoteStorageTargetsLoading] =
		useState(false);
	const [remoteStorageTargetsError, setRemoteStorageTargetsError] = useState<
		string | null
	>(null);
	const [
		remoteStorageTargetConnectorDescriptors,
		setRemoteStorageTargetConnectorDescriptors,
	] = useState<StorageConnectorDescriptor[]>([]);
	const [
		remoteStorageTargetConnectorDescriptorsLoading,
		setRemoteStorageTargetConnectorDescriptorsLoading,
	] = useState(false);
	const [
		remoteStorageTargetConnectorDescriptorsError,
		setRemoteStorageTargetConnectorDescriptorsError,
	] = useState<string | null>(null);
	const targetsRequestId = useRef(0);
	const descriptorsRequestId = useRef(0);
	const language = i18n.resolvedLanguage ?? i18n.language ?? "en";
	const remoteTargetBaseUrlRequiredMessage = t(
		"remote_node_ingress_profiles_base_url_required",
	);

	const loadTargets = useCallback(async () => {
		if (
			pageTab !== "storage-targets" ||
			!node ||
			!hasCompletedRemoteNodeEnrollment(node)
		)
			return;
		const requestId = ++targetsRequestId.current;
		setRemoteStorageTargetsLoading(true);
		setRemoteStorageTargetsError(null);
		try {
			const targets =
				await adminRemoteNodeService.listStorageTargets(remoteNodeId);
			if (requestId === targetsRequestId.current) {
				setRemoteStorageTargets(targets);
			}
		} catch (error) {
			if (requestId === targetsRequestId.current) {
				setRemoteStorageTargets([]);
				setRemoteStorageTargetsError(getApiErrorMessage(error));
				handleApiError(error);
			}
		} finally {
			if (requestId === targetsRequestId.current) {
				setRemoteStorageTargetsLoading(false);
			}
		}
	}, [node, pageTab, remoteNodeId]);

	const loadDescriptors = useCallback(async () => {
		if (
			pageTab !== "storage-targets" ||
			!node ||
			!hasCompletedRemoteNodeEnrollment(node)
		)
			return;
		const requestId = ++descriptorsRequestId.current;
		setRemoteStorageTargetConnectorDescriptorsLoading(true);
		setRemoteStorageTargetConnectorDescriptorsError(null);
		try {
			const catalog = await adminRemoteNodeService.listStorageTargetConnectors(
				remoteNodeId,
				language,
			);
			if (
				requestId === descriptorsRequestId.current &&
				(i18n.resolvedLanguage ?? i18n.language ?? "en") === language
			) {
				installStorageConnectorLocalizations(
					catalog.localizations,
					language,
					i18n,
				);
				setRemoteStorageTargetConnectorDescriptors(catalog.descriptors);
			}
		} catch (error) {
			if (requestId === descriptorsRequestId.current) {
				setRemoteStorageTargetConnectorDescriptors([]);
				setRemoteStorageTargetConnectorDescriptorsError(
					getApiErrorMessage(error),
				);
				handleApiError(error);
			}
		} finally {
			if (requestId === descriptorsRequestId.current) {
				setRemoteStorageTargetConnectorDescriptorsLoading(false);
			}
		}
	}, [i18n, language, node, pageTab, remoteNodeId]);

	useEffect(() => {
		let cancelled = false;
		setLoading(true);
		setNode(null);
		setForm(null);
		adminRemoteNodeService
			.get(remoteNodeId)
			.then((loadedNode) => {
				if (!cancelled) {
					setNode(loadedNode);
					setForm(getRemoteNodeForm(loadedNode));
				}
			})
			.catch((error) => {
				if (!cancelled) handleApiError(error);
			})
			.finally(() => {
				if (!cancelled) setLoading(false);
			});
		return () => {
			cancelled = true;
			targetsRequestId.current += 1;
			descriptorsRequestId.current += 1;
		};
	}, [remoteNodeId]);

	useEffect(() => {
		if (!node) return;
		if (
			pageTab !== "storage-targets" ||
			!hasCompletedRemoteNodeEnrollment(node)
		) {
			setRemoteStorageTargets([]);
			setRemoteStorageTargetConnectorDescriptors([]);
			setRemoteStorageTargetsError(null);
			setRemoteStorageTargetConnectorDescriptorsError(null);
			return;
		}
		if (
			(node.transport_mode ?? "direct") === "direct" &&
			!node.base_url.trim()
		) {
			setRemoteStorageTargetsError(remoteTargetBaseUrlRequiredMessage);
			return;
		}
		void loadTargets();
		void loadDescriptors();
	}, [
		loadDescriptors,
		loadTargets,
		node,
		pageTab,
		remoteTargetBaseUrlRequiredMessage,
	]);

	const setField = <K extends keyof RemoteNodeFormData>(
		key: K,
		value: RemoteNodeFormData[K],
	) => setForm((current) => (current ? { ...current, [key]: value } : current));

	const baseUrlValidationMessage = getRemoteNodeBaseUrlValidationMessage(
		form?.base_url ?? "",
		t,
	);
	const runConnectionTest = async () => {
		if (!node || !form || baseUrlValidationMessage) return false;
		if (
			!hasCompletedRemoteNodeEnrollment(node) ||
			form.base_url !== node.base_url ||
			form.transport_mode !== (node.transport_mode ?? "direct") ||
			(form.transport_mode === "direct" && !form.base_url.trim())
		) {
			return false;
		}
		try {
			const updated = await adminRemoteNodeService.testConnection(remoteNodeId);
			setNode(updated);
			setForm(getRemoteNodeForm(updated));
			invalidateAdminRemoteNodeLookup();
			toast.success(t("connection_success"));
			return true;
		} catch (error) {
			handleApiError(error);
			try {
				const latest = await adminRemoteNodeService.get(remoteNodeId);
				setNode(latest);
				setForm(getRemoteNodeForm(latest));
			} catch (refreshError) {
				logger.warn(
					"Failed to refresh remote node state after connection test",
					refreshError,
				);
			}
			return false;
		}
	};

	const generateEnrollmentCommand = async () => {
		if (
			!node ||
			hasCompletedRemoteNodeEnrollment(node) ||
			enrollmentCommandLoading
		) {
			return;
		}
		setEnrollmentCommandLoading(true);
		setEnrollmentCommandError(null);
		try {
			const command =
				await adminRemoteNodeService.createEnrollmentCommand(remoteNodeId);
			setEnrollmentCommand(command);
		} catch (error) {
			setEnrollmentCommandError(getApiErrorMessage(error));
			handleApiError(error);
		} finally {
			setEnrollmentCommandLoading(false);
		}
	};

	const submit = async () => {
		if (!node || !form || submitting || baseUrlValidationMessage) return;
		setSubmitting(true);
		try {
			const updated = await adminRemoteNodeService.update(
				remoteNodeId,
				buildUpdateRemoteNodePayload(form),
			);
			setNode(updated);
			setForm(getRemoteNodeForm(updated));
			invalidateAdminRemoteNodeLookup();
			toast.success(t("remote_node_updated"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setSubmitting(false);
		}
	};

	const createRemoteStorageTarget = async (
		payload: RemoteCreateStorageTargetRequest,
	) => {
		try {
			await adminRemoteNodeService.createStorageTarget(remoteNodeId, payload);
			toast.success(t("remote_node_ingress_profile_created"));
			await loadTargets();
		} catch (error) {
			handleApiError(error);
			throw error;
		}
	};

	const updateRemoteStorageTarget = async (
		targetKey: string,
		payload: RemoteUpdateStorageTargetRequest,
	) => {
		try {
			await adminRemoteNodeService.updateStorageTarget(
				remoteNodeId,
				targetKey,
				payload,
			);
			toast.success(t("remote_node_ingress_profile_updated"));
			await loadTargets();
		} catch (error) {
			handleApiError(error);
			throw error;
		}
	};

	const deleteRemoteStorageTarget = async (target: RemoteStorageTargetInfo) => {
		try {
			await adminRemoteNodeService.deleteStorageTarget(
				remoteNodeId,
				target.target_key,
			);
			toast.success(t("remote_node_ingress_profile_deleted"));
			await loadTargets();
		} catch (error) {
			handleApiError(error);
			throw error;
		}
	};

	return {
		baseUrlValidationMessage,
		copyToClipboard,
		createRemoteStorageTarget,
		deleteRemoteStorageTarget,
		enrollmentCommand,
		enrollmentCommandError,
		enrollmentCommandLoading,
		generateEnrollmentCommand,
		navigateBack: () =>
			navigate("/admin/remote-nodes", { viewTransition: false }),
		node,
		form,
		loading,
		remoteStorageTargetConnectorDescriptors,
		remoteStorageTargetConnectorDescriptorsError,
		remoteStorageTargetConnectorDescriptorsLoading,
		remoteStorageTargets,
		remoteStorageTargetsError,
		remoteStorageTargetsLoading,
		runConnectionTest,
		setField,
		submit,
		submitting,
		t,
		updateRemoteStorageTarget,
	};
}
