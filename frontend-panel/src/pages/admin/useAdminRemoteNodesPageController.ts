import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { hasCompletedRemoteNodeEnrollment } from "@/components/admin/admin-remote-nodes-page/shared";
import {
	buildCreateRemoteNodePayload,
	buildUpdateRemoteNodePayload,
	emptyRemoteNodeForm,
	getRemoteNodeBaseUrlValidationMessage,
	getRemoteNodeForm,
	type RemoteNodeFormData,
} from "@/components/admin/remoteNodeDialogShared";
import { getApiErrorMessage, handleApiError } from "@/hooks/useApiError";
import { useApiList } from "@/hooks/useApiList";
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { usePageTitle } from "@/hooks/usePageTitle";
import { usePendingId } from "@/hooks/usePendingId";
import { invalidateAdminRemoteNodeLookup } from "@/lib/adminRemoteNodeLookup";
import { writeTextToClipboard } from "@/lib/clipboard";
import { logger } from "@/lib/logger";
import {
	buildOffsetPaginationSearchParams,
	parseOffsetSearchParam,
	parsePageSizeOption,
	parsePageSizeSearchParam,
	parseSortOrderSearchParam,
	parseSortSearchParam,
	type SortOrder,
} from "@/lib/pagination";
import { adminRemoteNodeService } from "@/services/adminService";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import type { AdminRemoteNodeSortBy } from "@/types/adminSort";
import type {
	ManagedIngressDriverDescriptor,
	RemoteCreateIngressProfileRequest,
	RemoteEnrollmentCommandInfo,
	RemoteIngressProfileInfo,
	RemoteNodeInfo,
	RemoteUpdateIngressProfileRequest,
} from "@/types/api";

export const REMOTE_NODE_PAGE_SIZE_OPTIONS = [10, 20, 50] as const;
const DEFAULT_REMOTE_NODE_PAGE_SIZE = 20 as const;
const REMOTE_NODE_CREATE_LAST_STEP = 2 as const;
const REMOTE_NODE_SORT_BY_OPTIONS = [
	"id",
	"name",
	"base_url",
	"is_enabled",
	"last_checked_at",
	"created_at",
	"updated_at",
] as const satisfies readonly AdminRemoteNodeSortBy[];
const DEFAULT_REMOTE_NODE_SORT_BY =
	"created_at" as const satisfies AdminRemoteNodeSortBy;
const DEFAULT_REMOTE_NODE_SORT_ORDER = "desc" as const satisfies SortOrder;

function requiresDirectIngressProfileBaseUrl(node: RemoteNodeInfo) {
	return (
		(node.transport_mode ?? "direct") === "direct" && !node.base_url.trim()
	);
}

export function useAdminRemoteNodesPageController() {
	const { t } = useTranslation("admin");
	usePageTitle(t("remote_nodes"));
	const primarySiteUrl = useFrontendConfigStore((state) => state.siteUrl);
	const [searchParams, setSearchParams] = useSearchParams();
	const [offset, setOffset] = useState(() =>
		parseOffsetSearchParam(searchParams.get("offset")),
	);
	const [pageSize, setPageSize] = useState<
		(typeof REMOTE_NODE_PAGE_SIZE_OPTIONS)[number]
	>(() =>
		parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			REMOTE_NODE_PAGE_SIZE_OPTIONS,
			DEFAULT_REMOTE_NODE_PAGE_SIZE,
		),
	);
	const [sortBy, setSortBy] = useState<AdminRemoteNodeSortBy>(() =>
		parseSortSearchParam(
			searchParams.get("sortBy"),
			REMOTE_NODE_SORT_BY_OPTIONS,
			DEFAULT_REMOTE_NODE_SORT_BY,
		),
	);
	const [sortOrder, setSortOrder] = useState<SortOrder>(() =>
		parseSortOrderSearchParam(
			searchParams.get("sortOrder"),
			DEFAULT_REMOTE_NODE_SORT_ORDER,
		),
	);
	const {
		items: remoteNodes,
		setItems: setRemoteNodes,
		total,
		setTotal,
		loading,
		reload,
	} = useApiList(
		() =>
			adminRemoteNodeService.list({
				limit: pageSize,
				offset,
				sort_by: sortBy,
				sort_order: sortOrder,
			}),
		[offset, pageSize, sortBy, sortOrder],
	);
	const [dialogOpen, setDialogOpen] = useState(false);
	const [editingId, setEditingId] = useState<number | null>(null);
	const [editingNode, setEditingNode] = useState<RemoteNodeInfo | null>(null);
	const [enrollmentDialogOpen, setEnrollmentDialogOpen] = useState(false);
	const [enrollmentCommand, setEnrollmentCommand] =
		useState<RemoteEnrollmentCommandInfo | null>(null);
	const [enrollmentCommandCanTest, setEnrollmentCommandCanTest] =
		useState(false);
	const [generatingEnrollmentId, setGeneratingEnrollmentId] = useState<
		number | null
	>(null);
	const [form, setForm] = useState<RemoteNodeFormData>(emptyRemoteNodeForm);
	const [submitting, setSubmitting] = useState(false);
	const [createStep, setCreateStep] = useState(0);
	const [createStepTouched, setCreateStepTouched] = useState(false);
	const [managedIngressProfiles, setManagedIngressProfiles] = useState<
		RemoteIngressProfileInfo[]
	>([]);
	const [managedIngressProfilesLoading, setManagedIngressProfilesLoading] =
		useState(false);
	const [managedIngressProfilesError, setManagedIngressProfilesError] =
		useState<string | null>(null);
	const [managedIngressDriverDescriptors, setManagedIngressDriverDescriptors] =
		useState<ManagedIngressDriverDescriptor[]>([]);
	const [
		managedIngressDriverDescriptorsLoading,
		setManagedIngressDriverDescriptorsLoading,
	] = useState(false);
	const [
		managedIngressDriverDescriptorsError,
		setManagedIngressDriverDescriptorsError,
	] = useState<string | null>(null);
	const {
		pendingId: deletingRemoteNodeId,
		runWithPending: runWithDeletingRemoteNode,
	} = usePendingId<number>();
	const managedIngressRequestIdRef = useRef(0);
	const managedIngressDriverDescriptorsRequestIdRef = useRef(0);
	const totalPages = Math.max(1, Math.ceil(total / pageSize));
	const currentPage = Math.floor(offset / pageSize) + 1;
	const prevPageDisabled = offset === 0;
	const nextPageDisabled = offset + pageSize >= total;
	const createButtonTitle = primarySiteUrl
		? undefined
		: t("remote_node_primary_site_url_required");
	const pageSizeOptions = REMOTE_NODE_PAGE_SIZE_OPTIONS.map((size) => ({
		label: t("page_size_option", { count: size }),
		value: String(size),
	}));
	const remoteNodeBaseUrlValidationMessage =
		getRemoteNodeBaseUrlValidationMessage(form.base_url, t);

	useEffect(() => {
		setSearchParams(
			buildOffsetPaginationSearchParams({
				offset,
				pageSize,
				defaultPageSize: DEFAULT_REMOTE_NODE_PAGE_SIZE,
				extraParams: {
					sortBy: sortBy !== DEFAULT_REMOTE_NODE_SORT_BY ? sortBy : undefined,
					sortOrder:
						sortOrder !== DEFAULT_REMOTE_NODE_SORT_ORDER
							? sortOrder
							: undefined,
				},
			}),
			{ replace: true },
		);
	}, [offset, pageSize, setSearchParams, sortBy, sortOrder]);

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, REMOTE_NODE_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setPageSize(next);
		setOffset(0);
	};

	const handleSortChange = (
		nextSortBy: AdminRemoteNodeSortBy,
		nextOrder: SortOrder,
	) => {
		setSortBy(nextSortBy);
		setSortOrder(nextOrder);
		setOffset(0);
	};

	const resetDialogState = () => {
		setCreateStep(0);
		setCreateStepTouched(false);
	};

	const resetManagedIngressState = () => {
		managedIngressRequestIdRef.current += 1;
		managedIngressDriverDescriptorsRequestIdRef.current += 1;
		setManagedIngressProfiles([]);
		setManagedIngressProfilesLoading(false);
		setManagedIngressProfilesError(null);
		setManagedIngressDriverDescriptors([]);
		setManagedIngressDriverDescriptorsLoading(false);
		setManagedIngressDriverDescriptorsError(null);
	};

	const loadManagedIngressDriverDescriptors = async (
		remoteNodeId: number,
		{ showErrorToast = true }: { showErrorToast?: boolean } = {},
	) => {
		const requestId = managedIngressDriverDescriptorsRequestIdRef.current + 1;
		managedIngressDriverDescriptorsRequestIdRef.current = requestId;
		setManagedIngressDriverDescriptorsLoading(true);
		setManagedIngressDriverDescriptorsError(null);

		try {
			const descriptors =
				await adminRemoteNodeService.listIngressProfileDrivers(remoteNodeId);
			if (managedIngressDriverDescriptorsRequestIdRef.current !== requestId) {
				return;
			}
			setManagedIngressDriverDescriptors(descriptors);
			setManagedIngressDriverDescriptorsError(null);
		} catch (error) {
			if (managedIngressDriverDescriptorsRequestIdRef.current !== requestId) {
				return;
			}
			setManagedIngressDriverDescriptors([]);
			setManagedIngressDriverDescriptorsError(getApiErrorMessage(error));
			if (showErrorToast) {
				handleApiError(error);
			}
		} finally {
			if (managedIngressDriverDescriptorsRequestIdRef.current === requestId) {
				setManagedIngressDriverDescriptorsLoading(false);
			}
		}
	};

	const loadManagedIngressProfiles = async (
		remoteNodeId: number,
		{ showErrorToast = true }: { showErrorToast?: boolean } = {},
	) => {
		const requestId = managedIngressRequestIdRef.current + 1;
		managedIngressRequestIdRef.current = requestId;
		setManagedIngressProfilesLoading(true);
		setManagedIngressProfilesError(null);

		try {
			const profiles =
				await adminRemoteNodeService.listIngressProfiles(remoteNodeId);
			if (managedIngressRequestIdRef.current !== requestId) {
				return;
			}
			setManagedIngressProfiles(profiles);
			setManagedIngressProfilesError(null);
		} catch (error) {
			if (managedIngressRequestIdRef.current !== requestId) {
				return;
			}
			setManagedIngressProfiles([]);
			setManagedIngressProfilesError(getApiErrorMessage(error));
			if (showErrorToast) {
				handleApiError(error);
			}
		} finally {
			if (managedIngressRequestIdRef.current === requestId) {
				setManagedIngressProfilesLoading(false);
			}
		}
	};

	const openCreate = () => {
		if (!primarySiteUrl) {
			toast.error(t("remote_node_primary_site_url_required"));
			return;
		}

		setEditingId(null);
		setEditingNode(null);
		setForm({ ...emptyRemoteNodeForm });
		setEnrollmentCommandCanTest(false);
		resetDialogState();
		resetManagedIngressState();
		setDialogOpen(true);
	};

	const openEdit = (node: RemoteNodeInfo) => {
		setEditingId(node.id);
		setEditingNode(node);
		setForm(getRemoteNodeForm(node));
		resetDialogState();
		resetManagedIngressState();
		if (requiresDirectIngressProfileBaseUrl(node)) {
			setManagedIngressProfilesError(
				t("remote_node_ingress_profiles_base_url_required"),
			);
		} else if (hasCompletedRemoteNodeEnrollment(node)) {
			void loadManagedIngressProfiles(node.id);
			void loadManagedIngressDriverDescriptors(node.id);
		} else {
			setManagedIngressProfilesError(null);
			setManagedIngressDriverDescriptorsError(null);
		}
		setDialogOpen(true);
	};

	const handleDialogOpenChange = (open: boolean) => {
		setDialogOpen(open);
		if (!open) {
			resetDialogState();
			resetManagedIngressState();
		}
	};

	const setField = <K extends keyof RemoteNodeFormData>(
		key: K,
		value: RemoteNodeFormData[K],
	) => setForm((prev) => ({ ...prev, [key]: value }));

	const copyToClipboard = async (value: string) => {
		try {
			await writeTextToClipboard(value);
			toast.success(t("core:copied_to_clipboard"));
		} catch {
			toast.error(t("errors:unexpected_error"));
		}
	};

	const syncRemoteNodeState = async (remoteNodeId: number) => {
		try {
			const latest = await adminRemoteNodeService.get(remoteNodeId);
			setEditingNode((current) =>
				current?.id === remoteNodeId ? latest : current,
			);
			setRemoteNodes((prev) =>
				prev.map((node) => (node.id === remoteNodeId ? latest : node)),
			);
			invalidateAdminRemoteNodeLookup();
		} catch (error) {
			logger.warn(
				"Failed to refresh remote node state after connection test",
				error,
			);
		}
	};

	const runConnectionTest = async ({
		showFailureError = true,
		showSuccessToast = true,
	}: {
		showFailureError?: boolean;
		showSuccessToast?: boolean;
	} = {}) => {
		if (editingId === null) {
			return false;
		}

		try {
			const updated = await adminRemoteNodeService.testConnection(editingId);
			setEditingNode(updated);
			setRemoteNodes((prev) =>
				prev.map((node) => (node.id === editingId ? updated : node)),
			);
			invalidateAdminRemoteNodeLookup();

			if (showSuccessToast) {
				toast.success(t("connection_success"));
			}
			return true;
		} catch (error) {
			if (showFailureError) {
				handleApiError(error);
			}
			await syncRemoteNodeState(editingId);
			return false;
		}
	};

	const persistRemoteNode = async () => {
		try {
			if (editingId !== null) {
				const updated = await adminRemoteNodeService.update(
					editingId,
					buildUpdateRemoteNodePayload(form),
				);
				setEditingNode(updated);
				setRemoteNodes((prev) =>
					prev.map((node) => (node.id === editingId ? updated : node)),
				);
				invalidateAdminRemoteNodeLookup();
				toast.success(t("remote_node_updated"));
				handleDialogOpenChange(false);
			} else {
				const created = await adminRemoteNodeService.create(
					buildCreateRemoteNodePayload(form),
				);
				const nextTotal = total + 1;
				const nextLastOffset = Math.max(
					0,
					Math.floor((nextTotal - 1) / pageSize) * pageSize,
				);
				if (nextLastOffset !== offset) {
					setOffset(nextLastOffset);
				} else {
					await reload();
				}
				invalidateAdminRemoteNodeLookup();
				handleDialogOpenChange(false);
				const command = await adminRemoteNodeService.createEnrollmentCommand(
					created.id,
				);
				setEnrollmentCommand(command);
				setEnrollmentCommandCanTest(Boolean(created.base_url.trim()));
				setEnrollmentDialogOpen(true);
				toast.success(t("remote_node_enrollment_prepared"));
			}
		} catch (error) {
			handleApiError(error);
		}
	};

	const submitRemoteNode = async () => {
		if (submitting) {
			return;
		}

		setSubmitting(true);
		try {
			await persistRemoteNode();
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
		setCreateStep(Math.max(0, Math.min(REMOTE_NODE_CREATE_LAST_STEP, step)));
	};

	const handleCreateNext = () => {
		if (createStep >= REMOTE_NODE_CREATE_LAST_STEP) {
			return;
		}

		setCreateStepTouched(true);

		if (createStep === 0 && !form.name.trim()) {
			return;
		}
		if (createStep === 1 && remoteNodeBaseUrlValidationMessage) {
			return;
		}

		setCreateStepTouched(false);
		setCreateStep((prev) => Math.min(REMOTE_NODE_CREATE_LAST_STEP, prev + 1));
	};

	const handleSubmit = () => {
		if (remoteNodeBaseUrlValidationMessage) {
			return;
		}

		if (editingId === null && createStep < REMOTE_NODE_CREATE_LAST_STEP) {
			handleCreateNext();
			return;
		}

		void submitRemoteNode();
	};

	const handleDelete = async (id: number) => {
		await runWithDeletingRemoteNode(id, async () => {
			try {
				await adminRemoteNodeService.delete(id);
				invalidateAdminRemoteNodeLookup();
				if (remoteNodes.length === 1 && offset > 0) {
					setOffset(Math.max(0, offset - pageSize));
				} else {
					await reload();
				}
				toast.success(t("remote_node_deleted"));
			} catch (error) {
				handleApiError(error);
			}
		});
	};
	const {
		confirmId: deleteId,
		requestConfirm,
		dialogProps: deleteDialogProps,
	} = useConfirmDialog(handleDelete);
	const deleteNodeName =
		deleteId !== null
			? (remoteNodes.find((node) => node.id === deleteId)?.name ?? "")
			: "";

	const handleRefresh = async () => {
		try {
			const nodesPage = await adminRemoteNodeService.list({
				limit: pageSize,
				offset,
				sort_by: sortBy,
				sort_order: sortOrder,
			});
			setRemoteNodes(nodesPage.items);
			setTotal(nodesPage.total);
			invalidateAdminRemoteNodeLookup();
		} catch (error) {
			handleApiError(error);
		}
	};

	const handleEnrollmentDialogOpenChange = (open: boolean) => {
		setEnrollmentDialogOpen(open);
		if (!open) {
			setEnrollmentCommand(null);
			setEnrollmentCommandCanTest(false);
		}
	};

	const handleVerifyEnrollmentConnection = async (remoteNodeId: number) => {
		try {
			const updated = await adminRemoteNodeService.testConnection(remoteNodeId);
			setRemoteNodes((prev) =>
				prev.map((node) => (node.id === remoteNodeId ? updated : node)),
			);
			invalidateAdminRemoteNodeLookup();
			if (editingId === remoteNodeId) {
				setEditingNode(updated);
			}
			toast.success(t("connection_success"));
			return true;
		} catch (error) {
			handleApiError(error);
			await syncRemoteNodeState(remoteNodeId);
			return false;
		}
	};

	const handleGenerateEnrollmentCommand = async (node: RemoteNodeInfo) => {
		if (hasCompletedRemoteNodeEnrollment(node)) {
			toast.info(t("remote_node_enrollment_completed_action_disabled"));
			return;
		}

		setGeneratingEnrollmentId(node.id);
		try {
			const command = await adminRemoteNodeService.createEnrollmentCommand(
				node.id,
			);
			setEnrollmentCommand(command);
			setEnrollmentCommandCanTest(Boolean(node.base_url.trim()));
			setEnrollmentDialogOpen(true);
		} catch (error) {
			handleApiError(error);
		} finally {
			setGeneratingEnrollmentId((current) =>
				current === node.id ? null : current,
			);
		}
	};

	const createManagedIngressProfile = async (
		payload: RemoteCreateIngressProfileRequest,
	) => {
		if (editingId == null) {
			return;
		}

		try {
			await adminRemoteNodeService.createIngressProfile(editingId, payload);
			toast.success(t("remote_node_ingress_profile_created"));
			await loadManagedIngressProfiles(editingId, { showErrorToast: false });
		} catch (error) {
			handleApiError(error);
			throw error;
		}
	};

	const updateManagedIngressProfile = async (
		profileKey: string,
		payload: RemoteUpdateIngressProfileRequest,
	) => {
		if (editingId == null) {
			return;
		}

		try {
			await adminRemoteNodeService.updateIngressProfile(
				editingId,
				profileKey,
				payload,
			);
			toast.success(t("remote_node_ingress_profile_updated"));
			await loadManagedIngressProfiles(editingId, { showErrorToast: false });
		} catch (error) {
			handleApiError(error);
			throw error;
		}
	};

	const deleteManagedIngressProfile = async (
		profile: RemoteIngressProfileInfo,
	) => {
		if (editingId == null) {
			return;
		}

		try {
			await adminRemoteNodeService.deleteIngressProfile(
				editingId,
				profile.profile_key,
			);
			toast.success(t("remote_node_ingress_profile_deleted"));
			await loadManagedIngressProfiles(editingId, { showErrorToast: false });
		} catch (error) {
			handleApiError(error);
			throw error;
		}
	};

	return {
		copyToClipboard,
		createButtonTitle,
		createManagedIngressProfile,
		createStep,
		createStepTouched,
		currentPage,
		deleteDialogProps,
		deleteManagedIngressProfile,
		deleteNodeName,
		deletingRemoteNodeId,
		dialogOpen,
		editingId,
		editingNode,
		enrollmentCommand,
		enrollmentCommandCanTest,
		enrollmentDialogOpen,
		form,
		generatingEnrollmentId,
		handleCreateBack,
		handleCreateNext,
		handleCreateStepChange,
		handleDialogOpenChange,
		handleEnrollmentDialogOpenChange,
		handleGenerateEnrollmentCommand,
		handlePageSizeChange,
		handleRefresh,
		handleSortChange,
		handleSubmit,
		handleVerifyEnrollmentConnection,
		loading,
		managedIngressProfiles,
		managedIngressDriverDescriptors,
		managedIngressDriverDescriptorsError,
		managedIngressDriverDescriptorsLoading,
		managedIngressProfilesError,
		managedIngressProfilesLoading,
		nextPageDisabled,
		openCreate,
		openEdit,
		pageSize,
		pageSizeOptions,
		prevPageDisabled,
		remoteNodes,
		requestConfirm,
		runConnectionTest,
		setField,
		setOffset,
		sortBy,
		sortOrder,
		submitting,
		t,
		total,
		totalPages,
		updateManagedIngressProfile,
	};
}
