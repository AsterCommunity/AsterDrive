import type { Dispatch, SetStateAction } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { supportsRemoteNodeBinding } from "@/components/admin/storage-policy-dialog/descriptorPredicates";
import type { PolicyFormData } from "@/components/admin/storage-policy-dialog/formTypes";
import { handleApiError } from "@/hooks/useApiError";
import {
	loadAdminRemoteNodeLookup,
	readAdminRemoteNodeLookup,
} from "@/lib/adminRemoteNodeLookup";
import {
	getStorageDriverDescriptor,
	loadAdminStorageDriverDescriptors,
	readAdminStorageDriverDescriptors,
} from "@/lib/adminStorageDriverDescriptors";
import { adminRemoteNodeService } from "@/services/adminService";
import type {
	RemoteCreateStorageTargetRequest,
	RemoteNodeInfo,
	RemoteStorageTargetDriverDescriptor,
	RemoteStorageTargetInfo,
	StorageConnectorDescriptor,
} from "@/types/api";

interface StoragePolicyDescriptorControllerInput {
	dialogOpen: boolean;
	form: PolicyFormData;
	setForm: Dispatch<SetStateAction<PolicyFormData>>;
}

export function useStoragePolicyDescriptorController({
	dialogOpen,
	form,
	setForm,
}: StoragePolicyDescriptorControllerInput) {
	const { t } = useTranslation("admin");
	const [remoteNodes, setRemoteNodes] = useState<RemoteNodeInfo[]>(
		() => readAdminRemoteNodeLookup() ?? [],
	);
	const [remoteStorageTargets, setRemoteStorageTargets] = useState<
		RemoteStorageTargetInfo[]
	>([]);
	const [remoteStorageTargetsLoading, setRemoteStorageTargetsLoading] =
		useState(false);
	const [remoteStorageTargetsError, setRemoteStorageTargetsError] = useState<
		string | null
	>(null);
	const remoteStorageTargetsRequestSerial = useRef(0);
	const [
		remoteStorageTargetDriverDescriptors,
		setRemoteStorageTargetDriverDescriptors,
	] = useState<RemoteStorageTargetDriverDescriptor[]>([]);
	const [
		remoteStorageTargetDriverDescriptorsLoading,
		setRemoteStorageTargetDriverDescriptorsLoading,
	] = useState(false);
	const [
		remoteStorageTargetDriverDescriptorsError,
		setRemoteStorageTargetDriverDescriptorsError,
	] = useState<string | null>(null);
	const remoteStorageTargetDriverDescriptorsRequestSerial = useRef(0);
	const [storageDriverDescriptors, setStorageDriverDescriptors] = useState<
		StorageConnectorDescriptor[]
	>(() => readAdminStorageDriverDescriptors() ?? []);
	const [storageDriverDescriptorsLoading, setStorageDriverDescriptorsLoading] =
		useState(() => readAdminStorageDriverDescriptors() == null);
	const [storageDriverDescriptorsError, setStorageDriverDescriptorsError] =
		useState<string | null>(null);

	const currentStorageDriverDescriptor = getStorageDriverDescriptor(
		storageDriverDescriptors,
		form.driver_type,
	);

	const loadRemoteStorageTargetsForPolicy = useCallback(
		async (
			remoteNodeId: number,
			{
				selectTargetKey,
				showErrorToast = true,
			}: { selectTargetKey?: string; showErrorToast?: boolean } = {},
		) => {
			const requestSerial = ++remoteStorageTargetsRequestSerial.current;
			setRemoteStorageTargetsLoading(true);
			setRemoteStorageTargetsError(null);

			try {
				const targets =
					await adminRemoteNodeService.listStorageTargets(remoteNodeId);
				if (requestSerial !== remoteStorageTargetsRequestSerial.current) {
					return;
				}
				setRemoteStorageTargets(targets);
				setRemoteStorageTargetsError(null);
				setForm((prev) => {
					if (prev.remote_node_id !== String(remoteNodeId)) {
						return prev;
					}
					if (
						selectTargetKey &&
						targets.some((target) => target.target_key === selectTargetKey)
					) {
						return {
							...prev,
							remote_storage_target_key: selectTargetKey,
						};
					}
					if (
						prev.remote_storage_target_key &&
						targets.some(
							(target) => target.target_key === prev.remote_storage_target_key,
						)
					) {
						return prev;
					}
					const fallbackTarget =
						targets.find((target) => target.is_default) ?? targets[0];
					return {
						...prev,
						remote_storage_target_key: fallbackTarget?.target_key ?? "",
					};
				});
			} catch (error) {
				if (requestSerial !== remoteStorageTargetsRequestSerial.current) {
					return;
				}
				setRemoteStorageTargets([]);
				setRemoteStorageTargetsError(t("remote_storage_targets_load_failed"));
				if (showErrorToast) {
					handleApiError(error);
				}
			} finally {
				if (requestSerial === remoteStorageTargetsRequestSerial.current) {
					setRemoteStorageTargetsLoading(false);
				}
			}
		},
		[setForm, t],
	);

	const loadRemoteStorageTargetDriverDescriptorsForPolicy = useCallback(
		async (
			remoteNodeId: number,
			{ showErrorToast = true }: { showErrorToast?: boolean } = {},
		) => {
			const requestSerial =
				++remoteStorageTargetDriverDescriptorsRequestSerial.current;
			setRemoteStorageTargetDriverDescriptorsLoading(true);
			setRemoteStorageTargetDriverDescriptorsError(null);

			try {
				const descriptors =
					await adminRemoteNodeService.listStorageTargetDrivers(remoteNodeId);
				if (
					requestSerial !==
					remoteStorageTargetDriverDescriptorsRequestSerial.current
				) {
					return;
				}
				setRemoteStorageTargetDriverDescriptors(descriptors);
				setRemoteStorageTargetDriverDescriptorsError(null);
			} catch (error) {
				if (
					requestSerial !==
					remoteStorageTargetDriverDescriptorsRequestSerial.current
				) {
					return;
				}
				setRemoteStorageTargetDriverDescriptors([]);
				setRemoteStorageTargetDriverDescriptorsError(
					t("remote_storage_target_drivers_load_failed"),
				);
				if (showErrorToast) {
					handleApiError(error);
				}
			} finally {
				if (
					requestSerial ===
					remoteStorageTargetDriverDescriptorsRequestSerial.current
				) {
					setRemoteStorageTargetDriverDescriptorsLoading(false);
				}
			}
		},
		[t],
	);

	const resetRemoteStorageTargets = useCallback(() => {
		remoteStorageTargetsRequestSerial.current += 1;
		remoteStorageTargetDriverDescriptorsRequestSerial.current += 1;
		setRemoteStorageTargets([]);
		setRemoteStorageTargetsLoading(false);
		setRemoteStorageTargetsError(null);
		setRemoteStorageTargetDriverDescriptors([]);
		setRemoteStorageTargetDriverDescriptorsLoading(false);
		setRemoteStorageTargetDriverDescriptorsError(null);
	}, []);

	useEffect(() => {
		const remoteNodeId = Number(form.remote_node_id);
		const canLoadTargets =
			dialogOpen &&
			supportsRemoteNodeBinding(currentStorageDriverDescriptor) &&
			Number.isSafeInteger(remoteNodeId) &&
			remoteNodeId > 0;
		if (!canLoadTargets) {
			resetRemoteStorageTargets();
			return;
		}

		void loadRemoteStorageTargetsForPolicy(remoteNodeId);
		void loadRemoteStorageTargetDriverDescriptorsForPolicy(remoteNodeId);
	}, [
		currentStorageDriverDescriptor,
		dialogOpen,
		form.remote_node_id,
		loadRemoteStorageTargetDriverDescriptorsForPolicy,
		loadRemoteStorageTargetsForPolicy,
		resetRemoteStorageTargets,
	]);

	useEffect(() => {
		let active = true;

		void loadAdminRemoteNodeLookup()
			.then((nodes) => {
				if (active) {
					setRemoteNodes(nodes);
				}
			})
			.catch((error) => {
				if (active) {
					handleApiError(error);
				}
			});

		return () => {
			active = false;
		};
	}, []);

	useEffect(() => {
		let active = true;

		setStorageDriverDescriptorsLoading(true);
		setStorageDriverDescriptorsError(null);
		void loadAdminStorageDriverDescriptors()
			.then((descriptors) => {
				if (active) {
					setStorageDriverDescriptors(descriptors);
					setStorageDriverDescriptorsError(null);
				}
			})
			.catch((error) => {
				if (active) {
					setStorageDriverDescriptorsError(
						t("policy_driver_options_load_failed"),
					);
					handleApiError(error);
				}
			})
			.finally(() => {
				if (active) {
					setStorageDriverDescriptorsLoading(false);
				}
			});

		return () => {
			active = false;
		};
	}, [t]);

	const refreshRemoteNodeLookup = useCallback(
		async (options?: { force?: boolean }) => {
			try {
				setRemoteNodes(await loadAdminRemoteNodeLookup(options));
			} catch (error) {
				handleApiError(error);
			}
		},
		[],
	);

	const refreshLookups = useCallback(async () => {
		const [remoteNodeLookup, descriptors] = await Promise.all([
			loadAdminRemoteNodeLookup({ force: true }),
			loadAdminStorageDriverDescriptors({ force: true }),
		]);
		setRemoteNodes(remoteNodeLookup);
		setStorageDriverDescriptors(descriptors);
	}, []);

	const createRemoteStorageTargetForPolicy = useCallback(
		async (payload: RemoteCreateStorageTargetRequest) => {
			const remoteNodeId = Number(form.remote_node_id);
			if (!Number.isSafeInteger(remoteNodeId) || remoteNodeId <= 0) {
				const error = new Error(t("policy_wizard_remote_node_required"));
				toast.error(error.message);
				throw error;
			}

			try {
				const created = await adminRemoteNodeService.createStorageTarget(
					remoteNodeId,
					payload,
				);
				toast.success(t("remote_node_ingress_profile_created"));
				await loadRemoteStorageTargetsForPolicy(remoteNodeId, {
					selectTargetKey: created.target_key,
					showErrorToast: false,
				});
			} catch (error) {
				handleApiError(error);
				throw error;
			}
		},
		[form.remote_node_id, loadRemoteStorageTargetsForPolicy, t],
	);

	return {
		createRemoteStorageTargetForPolicy,
		currentStorageDriverDescriptor,
		refreshLookups,
		refreshRemoteNodeLookup,
		remoteNodes,
		remoteStorageTargetDriverDescriptors,
		remoteStorageTargetDriverDescriptorsError,
		remoteStorageTargetDriverDescriptorsLoading,
		remoteStorageTargets,
		remoteStorageTargetsError,
		remoteStorageTargetsLoading,
		resetRemoteStorageTargets,
		storageDriverDescriptors,
		storageDriverDescriptorsError,
		storageDriverDescriptorsLoading,
	};
}
