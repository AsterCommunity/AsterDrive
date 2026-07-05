import { type SetStateAction, useCallback, useRef, useState } from "react";
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
import { useConfirmDialog } from "@/hooks/useConfirmDialog";
import { useManagedAdminList } from "@/hooks/useManagedAdminList";
import {
	type ManagedListQuerySchema,
	managedOffsetQueryField,
	managedPageSizeQueryField,
	managedSortByQueryField,
	managedSortOrderQueryField,
	useManagedListQueryState,
} from "@/hooks/useManagedListQueryState";
import { usePageTitle } from "@/hooks/usePageTitle";
import { usePendingId } from "@/hooks/usePendingId";
import { invalidateAdminRemoteNodeLookup } from "@/lib/adminRemoteNodeLookup";
import { writeTextToClipboard } from "@/lib/clipboard";
import { logger } from "@/lib/logger";
import { parsePageSizeOption, type SortOrder } from "@/lib/pagination";
import { adminRemoteNodeService } from "@/services/adminService";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import type { AdminRemoteNodeSortBy } from "@/types/adminSort";
import type {
	RemoteCreateStorageTargetRequest,
	RemoteEnrollmentCommandInfo,
	RemoteNodeInfo,
	RemoteStorageTargetDriverDescriptor,
	RemoteStorageTargetInfo,
	RemoteUpdateStorageTargetRequest,
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

type ManagedRemoteNodeQuery = {
	offset: number;
	pageSize: (typeof REMOTE_NODE_PAGE_SIZE_OPTIONS)[number];
	sortBy: AdminRemoteNodeSortBy;
	sortOrder: SortOrder;
};

const MANAGED_REMOTE_NODE_QUERY_DEFAULTS = {
	offset: 0,
	pageSize: DEFAULT_REMOTE_NODE_PAGE_SIZE,
	sortBy: DEFAULT_REMOTE_NODE_SORT_BY,
	sortOrder: DEFAULT_REMOTE_NODE_SORT_ORDER,
} satisfies ManagedRemoteNodeQuery;

const MANAGED_REMOTE_NODE_QUERY_SCHEMA = {
	offset: managedOffsetQueryField(),
	pageSize: managedPageSizeQueryField(
		REMOTE_NODE_PAGE_SIZE_OPTIONS,
		DEFAULT_REMOTE_NODE_PAGE_SIZE,
	),
	sortBy: managedSortByQueryField(
		REMOTE_NODE_SORT_BY_OPTIONS,
		DEFAULT_REMOTE_NODE_SORT_BY,
	),
	sortOrder: managedSortOrderQueryField(DEFAULT_REMOTE_NODE_SORT_ORDER),
} satisfies ManagedListQuerySchema<ManagedRemoteNodeQuery>;

function requiresDirectStorageTargetBaseUrl(node: RemoteNodeInfo) {
	return (
		(node.transport_mode ?? "direct") === "direct" && !node.base_url.trim()
	);
}

export function useAdminRemoteNodesPageController() {
	const { t } = useTranslation("admin");
	usePageTitle(t("remote_nodes"));
	const primarySiteUrl = useFrontendConfigStore((state) => state.siteUrl);
	const [searchParams, setSearchParams] = useSearchParams();
	const { query, setQuery } = useManagedListQueryState({
		defaults: MANAGED_REMOTE_NODE_QUERY_DEFAULTS,
		schema: MANAGED_REMOTE_NODE_QUERY_SCHEMA,
		searchParams,
		setSearchParams,
	});
	const { offset, pageSize, sortBy, sortOrder } = query;
	const setOffset = useCallback(
		(value: SetStateAction<number>) => {
			setQuery((current) => ({
				offset: typeof value === "function" ? value(current.offset) : value,
			}));
		},
		[setQuery],
	);
	const {
		currentPage,
		items: remoteNodes,
		setItems: setRemoteNodes,
		total,
		totalPages,
		setTotal,
		loading,
		reload,
		nextPageDisabled,
		prevPageDisabled,
	} = useManagedAdminList<RemoteNodeInfo, ManagedRemoteNodeQuery>({
		deps: [offset, pageSize, sortBy, sortOrder],
		loadPage: (query) =>
			adminRemoteNodeService.list({
				limit: query.pageSize,
				offset: query.offset,
				sort_by: query.sortBy,
				sort_order: query.sortOrder,
			}),
		query,
		setOffset,
	});
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
	const [remoteStorageTargets, setRemoteStorageTargets] = useState<
		RemoteStorageTargetInfo[]
	>([]);
	const [remoteStorageTargetsLoading, setRemoteStorageTargetsLoading] =
		useState(false);
	const [remoteStorageTargetsError, setRemoteStorageTargetsError] = useState<
		string | null
	>(null);
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
	const {
		pendingId: deletingRemoteNodeId,
		runWithPending: runWithDeletingRemoteNode,
	} = usePendingId<number>();
	const remoteStorageTargetRequestIdRef = useRef(0);
	const remoteStorageTargetDriverDescriptorsRequestIdRef = useRef(0);
	const createButtonTitle = primarySiteUrl
		? undefined
		: t("remote_node_primary_site_url_required");
	const pageSizeOptions = REMOTE_NODE_PAGE_SIZE_OPTIONS.map((size) => ({
		label: t("page_size_option", { count: size }),
		value: String(size),
	}));
	const remoteNodeBaseUrlValidationMessage =
		getRemoteNodeBaseUrlValidationMessage(form.base_url, t);

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, REMOTE_NODE_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setQuery({ offset: 0, pageSize: next });
	};

	const handleSortChange = (
		nextSortBy: AdminRemoteNodeSortBy,
		nextOrder: SortOrder,
	) => {
		setQuery({ offset: 0, sortBy: nextSortBy, sortOrder: nextOrder });
	};

	const resetDialogState = () => {
		setCreateStep(0);
		setCreateStepTouched(false);
	};

	const resetRemoteStorageTargetState = () => {
		remoteStorageTargetRequestIdRef.current += 1;
		remoteStorageTargetDriverDescriptorsRequestIdRef.current += 1;
		setRemoteStorageTargets([]);
		setRemoteStorageTargetsLoading(false);
		setRemoteStorageTargetsError(null);
		setRemoteStorageTargetDriverDescriptors([]);
		setRemoteStorageTargetDriverDescriptorsLoading(false);
		setRemoteStorageTargetDriverDescriptorsError(null);
	};

	const loadRemoteStorageTargetDriverDescriptors = async (
		remoteNodeId: number,
		{ showErrorToast = true }: { showErrorToast?: boolean } = {},
	) => {
		const requestId =
			remoteStorageTargetDriverDescriptorsRequestIdRef.current + 1;
		remoteStorageTargetDriverDescriptorsRequestIdRef.current = requestId;
		setRemoteStorageTargetDriverDescriptorsLoading(true);
		setRemoteStorageTargetDriverDescriptorsError(null);

		try {
			const descriptors =
				await adminRemoteNodeService.listStorageTargetDrivers(remoteNodeId);
			if (
				remoteStorageTargetDriverDescriptorsRequestIdRef.current !== requestId
			) {
				return;
			}
			setRemoteStorageTargetDriverDescriptors(descriptors);
			setRemoteStorageTargetDriverDescriptorsError(null);
		} catch (error) {
			if (
				remoteStorageTargetDriverDescriptorsRequestIdRef.current !== requestId
			) {
				return;
			}
			setRemoteStorageTargetDriverDescriptors([]);
			setRemoteStorageTargetDriverDescriptorsError(getApiErrorMessage(error));
			if (showErrorToast) {
				handleApiError(error);
			}
		} finally {
			if (
				remoteStorageTargetDriverDescriptorsRequestIdRef.current === requestId
			) {
				setRemoteStorageTargetDriverDescriptorsLoading(false);
			}
		}
	};

	const loadRemoteStorageTargets = async (
		remoteNodeId: number,
		{ showErrorToast = true }: { showErrorToast?: boolean } = {},
	) => {
		const requestId = remoteStorageTargetRequestIdRef.current + 1;
		remoteStorageTargetRequestIdRef.current = requestId;
		setRemoteStorageTargetsLoading(true);
		setRemoteStorageTargetsError(null);

		try {
			const profiles =
				await adminRemoteNodeService.listStorageTargets(remoteNodeId);
			if (remoteStorageTargetRequestIdRef.current !== requestId) {
				return;
			}
			setRemoteStorageTargets(profiles);
			setRemoteStorageTargetsError(null);
		} catch (error) {
			if (remoteStorageTargetRequestIdRef.current !== requestId) {
				return;
			}
			setRemoteStorageTargets([]);
			setRemoteStorageTargetsError(getApiErrorMessage(error));
			if (showErrorToast) {
				handleApiError(error);
			}
		} finally {
			if (remoteStorageTargetRequestIdRef.current === requestId) {
				setRemoteStorageTargetsLoading(false);
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
		resetRemoteStorageTargetState();
		setDialogOpen(true);
	};

	const openEdit = (node: RemoteNodeInfo) => {
		setEditingId(node.id);
		setEditingNode(node);
		setForm(getRemoteNodeForm(node));
		resetDialogState();
		resetRemoteStorageTargetState();
		if (requiresDirectStorageTargetBaseUrl(node)) {
			setRemoteStorageTargetsError(
				t("remote_node_ingress_profiles_base_url_required"),
			);
		} else if (hasCompletedRemoteNodeEnrollment(node)) {
			void loadRemoteStorageTargets(node.id);
			void loadRemoteStorageTargetDriverDescriptors(node.id);
		} else {
			setRemoteStorageTargetsError(null);
			setRemoteStorageTargetDriverDescriptorsError(null);
		}
		setDialogOpen(true);
	};

	const handleDialogOpenChange = (open: boolean) => {
		setDialogOpen(open);
		if (!open) {
			resetDialogState();
			resetRemoteStorageTargetState();
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

	const createRemoteStorageTarget = async (
		payload: RemoteCreateStorageTargetRequest,
	) => {
		if (editingId == null) {
			return;
		}

		try {
			await adminRemoteNodeService.createStorageTarget(editingId, payload);
			toast.success(t("remote_node_ingress_profile_created"));
			await loadRemoteStorageTargets(editingId, { showErrorToast: false });
		} catch (error) {
			handleApiError(error);
			throw error;
		}
	};

	const updateRemoteStorageTarget = async (
		target_key: string,
		payload: RemoteUpdateStorageTargetRequest,
	) => {
		if (editingId == null) {
			return;
		}

		try {
			await adminRemoteNodeService.updateStorageTarget(
				editingId,
				target_key,
				payload,
			);
			toast.success(t("remote_node_ingress_profile_updated"));
			await loadRemoteStorageTargets(editingId, { showErrorToast: false });
		} catch (error) {
			handleApiError(error);
			throw error;
		}
	};

	const deleteRemoteStorageTarget = async (
		profile: RemoteStorageTargetInfo,
	) => {
		if (editingId == null) {
			return;
		}

		try {
			await adminRemoteNodeService.deleteStorageTarget(
				editingId,
				profile.target_key,
			);
			toast.success(t("remote_node_ingress_profile_deleted"));
			await loadRemoteStorageTargets(editingId, { showErrorToast: false });
		} catch (error) {
			handleApiError(error);
			throw error;
		}
	};

	return {
		copyToClipboard,
		createButtonTitle,
		createRemoteStorageTarget,
		createStep,
		createStepTouched,
		currentPage,
		deleteDialogProps,
		deleteRemoteStorageTarget,
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
		remoteStorageTargets,
		remoteStorageTargetDriverDescriptors,
		remoteStorageTargetDriverDescriptorsError,
		remoteStorageTargetDriverDescriptorsLoading,
		remoteStorageTargetsError,
		remoteStorageTargetsLoading,
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
		updateRemoteStorageTarget,
	};
}
