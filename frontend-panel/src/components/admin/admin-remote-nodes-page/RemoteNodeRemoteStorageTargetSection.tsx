import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
	buildCreateRemoteStorageTargetPayload,
	buildUpdateRemoteStorageTargetPayload,
	emptyRemoteStorageTargetForm,
	getRemoteStorageTargetForm,
	isRemoteStorageTargetDriverType,
	type RemoteStorageTargetDriverType,
	type RemoteStorageTargetFormData,
} from "@/components/admin/remoteStorageTargetDialogShared";
import { AnimatedCollapsible } from "@/components/common/AnimatedCollapsible";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type {
	RemoteCreateStorageTargetRequest,
	RemoteStorageTargetDriverDescriptor,
	RemoteStorageTargetInfo,
	RemoteUpdateStorageTargetRequest,
} from "@/types/api";
import { RemoteNodeRemoteStorageTargetForm } from "./RemoteNodeRemoteStorageTargetForm";
import { RemoteNodeRemoteStorageTargetsList } from "./RemoteNodeRemoteStorageTargetsList";

type SupportedRemoteStorageTargetDriverDescriptor =
	RemoteStorageTargetDriverDescriptor & {
		driver_type: RemoteStorageTargetDriverType;
	};

interface RemoteNodeRemoteStorageTargetSectionProps {
	allowCreate?: boolean;
	createLabelKey?: string;
	descriptionKey?: string;
	driverDescriptors?: RemoteStorageTargetDriverDescriptor[];
	errorMessage: string | null;
	listViewportClassName?: string;
	loading: boolean;
	onCreateTarget?: (payload: RemoteCreateStorageTargetRequest) => Promise<void>;
	onDeleteTarget?: (target: RemoteStorageTargetInfo) => Promise<void>;
	onUpdateTarget?: (
		targetKey: string,
		payload: RemoteUpdateStorageTargetRequest,
	) => Promise<void>;
	readOnly?: boolean;
	surface?: "card" | "plain";
	targets: RemoteStorageTargetInfo[];
	titleKey?: string;
}

export function RemoteNodeRemoteStorageTargetSection({
	allowCreate = false,
	createLabelKey = "remote_node_ingress_profiles_create",
	descriptionKey = "remote_node_ingress_profiles_desc",
	driverDescriptors = [],
	errorMessage,
	listViewportClassName,
	loading,
	onCreateTarget,
	onDeleteTarget,
	onUpdateTarget,
	readOnly = false,
	surface = "card",
	targets,
	titleKey = "remote_node_ingress_profiles_title",
}: RemoteNodeRemoteStorageTargetSectionProps) {
	const { t } = useTranslation("admin");
	const [draftMode, setDraftMode] = useState<"create" | "edit" | null>(null);
	const [editingTargetKey, setEditingTargetKey] = useState<string | null>(null);
	const [form, setForm] = useState<RemoteStorageTargetFormData>(
		emptyRemoteStorageTargetForm,
	);
	const [submitting, setSubmitting] = useState(false);
	const [pendingDeleteTargetKey, setPendingDeleteTargetKey] = useState<
		string | null
	>(null);
	const [readOnlyOpen, setReadOnlyOpen] = useState(false);
	const editingTarget =
		draftMode === "edit"
			? (targets.find((target) => target.target_key === editingTargetKey) ??
				null)
			: null;
	const activeDraftMode =
		(readOnly && !allowCreate) ||
		(draftMode === "edit" && editingTarget == null)
			? null
			: draftMode;
	const canCreateTargets =
		Boolean(onCreateTarget) && (!readOnly || allowCreate);
	const supportedDriverDescriptors = driverDescriptors.flatMap(
		(descriptor): SupportedRemoteStorageTargetDriverDescriptor[] =>
			isRemoteStorageTargetDriverType(descriptor.driver_type)
				? [{ ...descriptor, driver_type: descriptor.driver_type }]
				: [],
	);
	const activeDriverDescriptor =
		supportedDriverDescriptors.find(
			(descriptor) => descriptor.driver_type === form.driver_type,
		) ?? null;
	const firstSupportedDriverType =
		supportedDriverDescriptors[0]?.driver_type ?? null;
	const supportedDriverTypes = new Set(
		supportedDriverDescriptors.map((descriptor) => descriptor.driver_type),
	);
	const driverTypeError =
		activeDraftMode != null && !supportedDriverTypes.has(form.driver_type)
			? t("remote_node_ingress_profile_driver_unsupported")
			: null;
	const activeFieldNames = new Set(
		activeDriverDescriptor?.fields.map((field) => field.name) ?? [],
	);
	const activeFieldByName = new Map(
		activeDriverDescriptor?.fields.map((field) => [field.name, field]) ?? [],
	);
	const activeField = (name: string) => activeFieldByName.get(name);
	const basePathField = activeField("base_path");
	const endpointField = activeField("endpoint");
	const bucketField = activeField("bucket");
	const accessKeyField = activeField("access_key");
	const secretKeyField = activeField("secret_key");
	const activePendingDeleteTargetKey = targets.some(
		(target) => target.target_key === pendingDeleteTargetKey,
	)
		? pendingDeleteTargetKey
		: null;

	const startCreate = () => {
		if (!canCreateTargets || !firstSupportedDriverType) {
			return;
		}
		setDraftMode("create");
		setEditingTargetKey(null);
		setReadOnlyOpen(true);
		setForm({
			...emptyRemoteStorageTargetForm,
			driver_type: firstSupportedDriverType,
			is_default: targets.length === 0,
		});
	};

	const startEdit = (target: RemoteStorageTargetInfo) => {
		setDraftMode("edit");
		setEditingTargetKey(target.target_key);
		setForm(getRemoteStorageTargetForm(target));
	};

	const resetDraft = () => {
		setDraftMode(null);
		setEditingTargetKey(null);
		setForm(emptyRemoteStorageTargetForm);
	};

	const setField = <K extends keyof RemoteStorageTargetFormData>(
		key: K,
		value: RemoteStorageTargetFormData[K],
	) => setForm((current) => ({ ...current, [key]: value }));

	const nameError = form.name.trim()
		? null
		: t("remote_node_ingress_profile_name_required");
	const localPathCandidate = form.base_path.trim().replaceAll("\\", "/");
	const requiresLocalRelativePath =
		basePathField?.validation?.relative_local_path === true;
	const localPathError =
		basePathField?.required && !form.base_path.trim()
			? t("remote_node_ingress_profile_base_path_required")
			: basePathField && requiresLocalRelativePath
				? !form.base_path.trim()
					? t("remote_node_ingress_profile_base_path_required")
					: localPathCandidate.startsWith("/") ||
							/^[A-Za-z]:/.test(localPathCandidate) ||
							localPathCandidate.split("/").some((segment) => segment === "..")
						? t("remote_node_ingress_profile_base_path_relative")
						: null
				: null;
	const endpointError =
		endpointField?.required && !form.endpoint.trim()
			? t("remote_node_ingress_profile_endpoint_required")
			: null;
	const bucketError =
		bucketField?.required && !form.bucket.trim()
			? t("remote_node_ingress_profile_bucket_required")
			: null;
	const preservesExistingSecretValue =
		activeDraftMode === "edit" &&
		editingTarget?.driver_type === form.driver_type &&
		secretKeyField?.secret === true;
	const accessKeyError =
		accessKeyField?.required && !form.access_key.trim()
			? t("remote_node_ingress_profile_access_key_required")
			: null;
	const secretKeyError =
		secretKeyField?.required &&
		!preservesExistingSecretValue &&
		!form.secret_key.trim()
			? t("remote_node_ingress_profile_secret_key_required")
			: null;
	const defaultToggleLocked =
		activeDraftMode === "edit" && editingTarget?.is_default;
	const submitDisabled =
		submitting ||
		Boolean(errorMessage) ||
		Boolean(
			nameError ||
				driverTypeError ||
				localPathError ||
				endpointError ||
				bucketError ||
				accessKeyError ||
				secretKeyError,
		);

	const handleSubmit = async () => {
		if (activeDraftMode == null || submitDisabled) {
			return;
		}

		setSubmitting(true);
		try {
			if (activeDraftMode === "create" && onCreateTarget) {
				await onCreateTarget(
					buildCreateRemoteStorageTargetPayload(form, activeFieldNames),
				);
			} else if (editingTarget != null && onUpdateTarget) {
				await onUpdateTarget(
					editingTarget.target_key,
					buildUpdateRemoteStorageTargetPayload(
						form,
						activeFieldNames,
						editingTarget,
					),
				);
			}
			resetDraft();
		} catch {
			// Parent handlers surface API errors; keep the draft open on failure.
		} finally {
			setSubmitting(false);
		}
	};

	const handleDeleteTarget = async (target: RemoteStorageTargetInfo) => {
		if (!onDeleteTarget) {
			return;
		}
		setPendingDeleteTargetKey(null);
		await onDeleteTarget(target);
		if (editingTargetKey === target.target_key) {
			resetDraft();
		}
	};

	const Root = surface === "card" ? "section" : "div";
	const rootClassName =
		surface === "card"
			? "rounded-2xl border border-border/70 bg-background/70 p-5"
			: "space-y-4 border-t border-border/70 pt-4";
	const listProps = {
		errorMessage,
		loading,
		pendingDeleteTargetKey: activePendingDeleteTargetKey,
		onCancelDelete: () => setPendingDeleteTargetKey(null),
		onConfirmDeleteTarget: (target: RemoteStorageTargetInfo) =>
			void handleDeleteTarget(target),
		onRequestDeleteTarget: (target: RemoteStorageTargetInfo) =>
			setPendingDeleteTargetKey(target.target_key),
		onEditTarget: startEdit,
		targets,
	};

	return (
		<Root className={rootClassName}>
			<div className="flex flex-wrap items-start justify-between gap-3">
				<div>
					<h3 className="text-base font-semibold text-foreground">
						{t(titleKey)}
					</h3>
					<p className="mt-1 text-sm text-muted-foreground">
						{t(descriptionKey)}
					</p>
				</div>
				{readOnly ? (
					<div className="flex flex-wrap items-center gap-2">
						{allowCreate && activeDraftMode == null ? (
							<Button
								type="button"
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={startCreate}
								disabled={
									loading ||
									Boolean(errorMessage) ||
									firstSupportedDriverType == null ||
									!canCreateTargets
								}
							>
								<Icon name="Plus" aria-hidden className="mr-1 size-4" />
								{t(createLabelKey)}
							</Button>
						) : null}
						<Button
							type="button"
							variant="outline"
							size="sm"
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							aria-expanded={readOnlyOpen}
							onClick={() => setReadOnlyOpen((open) => !open)}
						>
							<Icon
								name="CaretDown"
								aria-hidden
								className={`mr-1 size-3.5 transition-transform ${
									readOnlyOpen ? "rotate-180" : ""
								}`}
							/>
							{t(
								readOnlyOpen
									? "policy_remote_storage_targets_hide"
									: "policy_remote_storage_targets_show",
							)}
						</Button>
					</div>
				) : activeDraftMode == null ? (
					<Button
						type="button"
						size="sm"
						className={ADMIN_CONTROL_HEIGHT_CLASS}
						onClick={startCreate}
						disabled={
							loading ||
							Boolean(errorMessage) ||
							firstSupportedDriverType == null ||
							!canCreateTargets
						}
					>
						<Icon name="Plus" aria-hidden className="mr-1 size-4" />
						{t(createLabelKey)}
					</Button>
				) : null}
			</div>

			{errorMessage ? (
				<div className="mt-4 rounded-2xl border border-destructive/30 bg-destructive/5 p-4 text-sm text-destructive">
					{errorMessage}
				</div>
			) : null}

			{activeDraftMode != null ? (
				<RemoteNodeRemoteStorageTargetForm
					accessKeyError={accessKeyError}
					bucketError={bucketError}
					defaultToggleLocked={Boolean(defaultToggleLocked)}
					driverDescriptors={supportedDriverDescriptors}
					driverTypeError={driverTypeError}
					draftMode={activeDraftMode}
					editingProfile={editingTarget}
					endpointError={endpointError}
					form={form}
					localPathError={localPathError}
					nameError={nameError}
					onCancel={resetDraft}
					onFieldChange={setField}
					onSubmit={() => void handleSubmit()}
					secretKeyError={secretKeyError}
					submitDisabled={submitDisabled}
					submitting={submitting}
				/>
			) : null}

			{readOnly ? (
				<AnimatedCollapsible
					open={readOnlyOpen}
					contentClassName="max-h-[min(52vh,28rem)] overflow-y-auto pr-1"
				>
					<RemoteNodeRemoteStorageTargetsList {...listProps} readOnly />
				</AnimatedCollapsible>
			) : (
				<div className={listViewportClassName}>
					<RemoteNodeRemoteStorageTargetsList {...listProps} />
				</div>
			)}
		</Root>
	);
}
