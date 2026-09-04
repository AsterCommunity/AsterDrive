import type { Dispatch, SetStateAction } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { findConnectorFieldByDataSource } from "@/components/admin/storage-policy-dialog/descriptorPredicates";
import {
	connectorNumberValue,
	connectorStringValue,
	type PolicyFormData,
	updatedConnectorConfigValues,
} from "@/components/admin/storage-policy-dialog/formTypes";
import { handleApiError } from "@/hooks/useApiError";
import {
	loadAdminRemoteNodeLookup,
	readAdminRemoteNodeLookup,
} from "@/lib/adminRemoteNodeLookup";
import {
	installAdminStorageConnectorLocalizations,
	loadAdminStorageConnectorLocalizations,
	translateStorageConnectorMessage,
} from "@/lib/adminStorageConnectorLocalizations";
import {
	getStorageConnectorDescriptor,
	loadAdminStorageDriverDescriptors,
	readAdminStorageDriverDescriptors,
} from "@/lib/adminStorageDriverDescriptors";
import { adminRemoteNodeService } from "@/services/adminService";
import type {
	RemoteCreateStorageTargetRequest,
	RemoteNodeInfo,
	RemoteStorageTargetInfo,
	StorageConnectorCatalogContext,
	StorageConnectorDescriptor,
} from "@/types/api";

interface StoragePolicyDescriptorControllerInput {
	dialogOpen: boolean;
	form: PolicyFormData;
	setForm: Dispatch<SetStateAction<PolicyFormData>>;
	setupMode: boolean;
}

export function useStoragePolicyDescriptorController({
	dialogOpen,
	form,
	setForm,
	setupMode,
}: StoragePolicyDescriptorControllerInput) {
	const { i18n, t } = useTranslation("admin");
	const primaryCatalogContext: StorageConnectorCatalogContext = setupMode
		? "setup"
		: "manage";
	const creationCatalogContext: StorageConnectorCatalogContext = setupMode
		? "setup"
		: "create";
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
	const storageConnectorLocalizationsRequestSerial = useRef(0);
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
	const remoteStorageTargetConnectorDescriptorsRequestSerial = useRef(0);
	const [storageDriverDescriptors, setStorageDriverDescriptors] = useState<
		StorageConnectorDescriptor[]
	>(() => readAdminStorageDriverDescriptors(primaryCatalogContext) ?? []);
	const [
		creatableStorageDriverDescriptors,
		setCreatableStorageDriverDescriptors,
	] = useState<StorageConnectorDescriptor[]>(
		() => readAdminStorageDriverDescriptors(creationCatalogContext) ?? [],
	);
	const [storageDriverDescriptorsLoading, setStorageDriverDescriptorsLoading] =
		useState(
			() => readAdminStorageDriverDescriptors(primaryCatalogContext) == null,
		);
	const [
		creatableStorageDriverDescriptorsLoading,
		setCreatableStorageDriverDescriptorsLoading,
	] = useState(
		() => readAdminStorageDriverDescriptors(creationCatalogContext) == null,
	);
	const [storageDriverDescriptorsError, setStorageDriverDescriptorsError] =
		useState<string | null>(null);
	const [
		creatableStorageDriverDescriptorsError,
		setCreatableStorageDriverDescriptorsError,
	] = useState<string | null>(null);

	const currentStorageDriverDescriptor = getStorageConnectorDescriptor(
		storageDriverDescriptors,
		form.connector_id,
	);
	const remoteNodeField = findConnectorFieldByDataSource(
		currentStorageDriverDescriptor,
		"remote_nodes",
	);
	const remoteStorageTargetField = findConnectorFieldByDataSource(
		currentStorageDriverDescriptor,
		"remote_storage_targets",
	);
	const remoteNodeFieldName = remoteNodeField?.name ?? null;
	const remoteStorageTargetFieldName = remoteStorageTargetField?.name ?? null;
	const language = i18n.resolvedLanguage ?? i18n.language ?? "en";

	const loadConnectorLocalizations = useCallback(
		async ({ force = false }: { force?: boolean } = {}) => {
			const requestSerial =
				++storageConnectorLocalizationsRequestSerial.current;
			const contexts = Array.from(
				new Set([primaryCatalogContext, creationCatalogContext]),
			);
			const catalogs = await Promise.all(
				contexts.map((context) =>
					loadAdminStorageConnectorLocalizations({
						context,
						force,
						locale: language,
					}),
				),
			);
			if (
				requestSerial !== storageConnectorLocalizationsRequestSerial.current ||
				(i18n.resolvedLanguage ?? i18n.language ?? "en") !== language
			) {
				return;
			}
			for (const catalog of catalogs) {
				installAdminStorageConnectorLocalizations(catalog, language, i18n);
			}
		},
		[creationCatalogContext, i18n, language, primaryCatalogContext],
	);

	const loadRemoteStorageTargetsForPolicy = useCallback(
		async (
			remoteNodeId: number,
			{
				selectTargetKey,
				showErrorToast = true,
				syncPolicySelection = true,
			}: {
				selectTargetKey?: string;
				showErrorToast?: boolean;
				syncPolicySelection?: boolean;
			} = {},
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
				if (!syncPolicySelection) {
					return;
				}
				setForm((prev) => {
					if (
						remoteNodeFieldName == null ||
						remoteStorageTargetFieldName == null ||
						connectorNumberValue(prev, remoteNodeFieldName) !== remoteNodeId
					) {
						return prev;
					}
					if (
						selectTargetKey &&
						targets.some((target) => target.target_key === selectTargetKey)
					) {
						return {
							...prev,
							connector_config_values: updatedConnectorConfigValues(
								prev,
								remoteStorageTargetFieldName,
								selectTargetKey,
							),
						};
					}
					const currentTargetKey = connectorStringValue(
						prev,
						remoteStorageTargetFieldName,
					);
					if (
						currentTargetKey &&
						targets.some((target) => target.target_key === currentTargetKey)
					) {
						return prev;
					}
					const fallbackTarget =
						targets.find((target) => target.is_default) ?? targets[0];
					return {
						...prev,
						connector_config_values: updatedConnectorConfigValues(
							prev,
							remoteStorageTargetFieldName,
							fallbackTarget?.target_key ?? "",
						),
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
		[remoteNodeFieldName, remoteStorageTargetFieldName, setForm, t],
	);

	const loadRemoteStorageTargetConnectorDescriptorsForPolicy = useCallback(
		async (
			remoteNodeId: number,
			{ showErrorToast = true }: { showErrorToast?: boolean } = {},
		) => {
			const requestSerial =
				++remoteStorageTargetConnectorDescriptorsRequestSerial.current;
			setRemoteStorageTargetConnectorDescriptorsLoading(true);
			setRemoteStorageTargetConnectorDescriptorsError(null);

			try {
				const descriptors =
					await adminRemoteNodeService.listStorageTargetConnectors(
						remoteNodeId,
					);
				if (
					requestSerial !==
					remoteStorageTargetConnectorDescriptorsRequestSerial.current
				) {
					return;
				}
				setRemoteStorageTargetConnectorDescriptors(descriptors);
				setRemoteStorageTargetConnectorDescriptorsError(null);
			} catch (error) {
				if (
					requestSerial !==
					remoteStorageTargetConnectorDescriptorsRequestSerial.current
				) {
					return;
				}
				setRemoteStorageTargetConnectorDescriptors([]);
				setRemoteStorageTargetConnectorDescriptorsError(
					t("remote_storage_target_connectors_load_failed"),
				);
				if (showErrorToast) {
					handleApiError(error);
				}
			} finally {
				if (
					requestSerial ===
					remoteStorageTargetConnectorDescriptorsRequestSerial.current
				) {
					setRemoteStorageTargetConnectorDescriptorsLoading(false);
				}
			}
		},
		[t],
	);

	const resetRemoteStorageTargets = useCallback(() => {
		remoteStorageTargetsRequestSerial.current += 1;
		remoteStorageTargetConnectorDescriptorsRequestSerial.current += 1;
		setRemoteStorageTargets([]);
		setRemoteStorageTargetsLoading(false);
		setRemoteStorageTargetsError(null);
		setRemoteStorageTargetConnectorDescriptors([]);
		setRemoteStorageTargetConnectorDescriptorsLoading(false);
		setRemoteStorageTargetConnectorDescriptorsError(null);
	}, []);

	const selectedRemoteNodeId = remoteNodeFieldName
		? connectorNumberValue(form, remoteNodeFieldName)
		: null;
	useEffect(() => {
		const canLoadTargets =
			dialogOpen &&
			remoteStorageTargetFieldName != null &&
			selectedRemoteNodeId != null &&
			Number.isSafeInteger(selectedRemoteNodeId) &&
			selectedRemoteNodeId > 0;
		if (!canLoadTargets) {
			resetRemoteStorageTargets();
			return;
		}

		void loadRemoteStorageTargetsForPolicy(selectedRemoteNodeId);
		void loadRemoteStorageTargetConnectorDescriptorsForPolicy(
			selectedRemoteNodeId,
		);
	}, [
		dialogOpen,
		loadRemoteStorageTargetConnectorDescriptorsForPolicy,
		loadRemoteStorageTargetsForPolicy,
		resetRemoteStorageTargets,
		remoteStorageTargetFieldName,
		selectedRemoteNodeId,
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
		void loadAdminStorageDriverDescriptors({ context: primaryCatalogContext })
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
	}, [primaryCatalogContext, t]);

	useEffect(() => {
		let active = true;

		setCreatableStorageDriverDescriptorsLoading(true);
		setCreatableStorageDriverDescriptorsError(null);
		void loadAdminStorageDriverDescriptors({ context: creationCatalogContext })
			.then((descriptors) => {
				if (active) {
					setCreatableStorageDriverDescriptors(descriptors);
					setCreatableStorageDriverDescriptorsError(null);
				}
			})
			.catch((error) => {
				if (active) {
					setCreatableStorageDriverDescriptorsError(
						t("policy_driver_options_load_failed"),
					);
					handleApiError(error);
				}
			})
			.finally(() => {
				if (active) {
					setCreatableStorageDriverDescriptorsLoading(false);
				}
			});

		return () => {
			active = false;
		};
	}, [creationCatalogContext, t]);

	useEffect(() => {
		void loadConnectorLocalizations().catch(handleApiError);
		return () => {
			storageConnectorLocalizationsRequestSerial.current += 1;
		};
	}, [loadConnectorLocalizations]);

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
		const descriptorPromise = loadAdminStorageDriverDescriptors({
			context: primaryCatalogContext,
			force: true,
		});
		const creatableDescriptorPromise =
			creationCatalogContext === primaryCatalogContext
				? descriptorPromise
				: loadAdminStorageDriverDescriptors({
						context: creationCatalogContext,
						force: true,
					});
		const [remoteNodeLookup, descriptors, creatableDescriptors] =
			await Promise.all([
				loadAdminRemoteNodeLookup({ force: true }),
				descriptorPromise,
				creatableDescriptorPromise,
				loadConnectorLocalizations({ force: true }),
			]);
		setRemoteNodes(remoteNodeLookup);
		setStorageDriverDescriptors(descriptors);
		setCreatableStorageDriverDescriptors(creatableDescriptors);
	}, [
		creationCatalogContext,
		loadConnectorLocalizations,
		primaryCatalogContext,
	]);

	const createRemoteStorageTargetForPolicy = useCallback(
		async (payload: RemoteCreateStorageTargetRequest) => {
			const remoteNodeId = remoteNodeFieldName
				? connectorNumberValue(form, remoteNodeFieldName)
				: null;
			if (
				remoteNodeId == null ||
				!Number.isSafeInteger(remoteNodeId) ||
				remoteNodeId <= 0
			) {
				const fieldLabel = remoteNodeField
					? translateStorageConnectorMessage(
							t,
							currentStorageDriverDescriptor?.connector_id,
							remoteNodeField.label_key,
						)
					: t("remote_node");
				const message = remoteNodeField?.required_message_key
					? translateStorageConnectorMessage(
							t,
							currentStorageDriverDescriptor?.connector_id,
							remoteNodeField.required_message_key,
							{ field: fieldLabel },
						)
					: t("policy_connector_field_required", { field: fieldLabel });
				const error = new Error(message);
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
		[
			currentStorageDriverDescriptor?.connector_id,
			form,
			loadRemoteStorageTargetsForPolicy,
			remoteNodeField,
			remoteNodeFieldName,
			t,
		],
	);

	return {
		creatableStorageDriverDescriptors,
		creatableStorageDriverDescriptorsError,
		creatableStorageDriverDescriptorsLoading,
		createRemoteStorageTargetForPolicy,
		loadRemoteStorageTargetsForPolicy,
		currentStorageDriverDescriptor,
		refreshLookups,
		refreshRemoteNodeLookup,
		remoteNodes,
		remoteStorageTargetConnectorDescriptors,
		remoteStorageTargetConnectorDescriptorsError,
		remoteStorageTargetConnectorDescriptorsLoading,
		remoteStorageTargets,
		remoteStorageTargetsError,
		remoteStorageTargetsLoading,
		resetRemoteStorageTargets,
		storageDriverDescriptors,
		storageDriverDescriptorsError,
		storageDriverDescriptorsLoading,
	};
}
