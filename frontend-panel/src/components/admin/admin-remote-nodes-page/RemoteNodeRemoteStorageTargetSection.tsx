import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
	buildCreateRemoteStorageTargetPayload,
	buildUpdateRemoteStorageTargetPayload,
	emptyRemoteStorageTargetForm,
	getRemoteStorageTargetForm,
	isRemoteStorageTargetConnectorId,
	type RemoteStorageTargetFormData,
} from "@/components/admin/remoteStorageTargetDialogShared";
import { AnimatedCollapsible } from "@/components/common/AnimatedCollapsible";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type {
	RemoteCreateStorageTargetRequest,
	RemoteStorageTargetInfo,
	RemoteUpdateStorageTargetRequest,
	StorageConnectorDescriptor,
} from "@/types/api";
import { RemoteNodeRemoteStorageTargetForm } from "./RemoteNodeRemoteStorageTargetForm";
import { RemoteNodeRemoteStorageTargetsList } from "./RemoteNodeRemoteStorageTargetsList";

interface RemoteNodeRemoteStorageTargetSectionProps {
	allowCreate?: boolean;
	createLabelKey?: string;
	descriptionKey?: string;
	connectorDescriptors?: StorageConnectorDescriptor[];
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
	connectorDescriptors = [],
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
	const supportedConnectorDescriptors = connectorDescriptors.filter(
		(descriptor) => isRemoteStorageTargetConnectorId(descriptor.connector_id),
	);
	const activeConnectorDescriptor =
		supportedConnectorDescriptors.find(
			(descriptor) => descriptor.connector_id === form.connector_id,
		) ?? null;
	const firstSupportedConnectorId =
		supportedConnectorDescriptors[0]?.connector_id ?? null;
	const supportedConnectorIds = new Set<string>(
		supportedConnectorDescriptors.map((descriptor) => descriptor.connector_id),
	);
	const connectorIdError =
		activeDraftMode != null && !supportedConnectorIds.has(form.connector_id)
			? t("remote_node_ingress_profile_driver_unsupported")
			: null;
	const activePendingDeleteTargetKey = targets.some(
		(target) => target.target_key === pendingDeleteTargetKey,
	)
		? pendingDeleteTargetKey
		: null;

	const startCreate = () => {
		if (!canCreateTargets || !firstSupportedConnectorId) {
			return;
		}
		setDraftMode("create");
		setEditingTargetKey(null);
		setReadOnlyOpen(true);
		setForm({
			...emptyRemoteStorageTargetForm,
			connector_id: firstSupportedConnectorId,
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
	const missingRequiredField =
		activeConnectorDescriptor?.fields.some((field) => {
			if (!field.required || field.name === "is_default") return false;
			if (
				field.scope !== "connector_config" &&
				activeDraftMode === "edit" &&
				editingTarget?.connector_id === form.connector_id
			) {
				return false;
			}
			const values =
				field.scope === "connector_config"
					? form.connector_config_values
					: form.credential_values;
			const value = values[field.name];
			return value == null || String(value).trim().length === 0;
		}) ?? false;
	const defaultToggleLocked =
		activeDraftMode === "edit" && editingTarget?.is_default;
	const submitDisabled =
		submitting ||
		Boolean(errorMessage) ||
		Boolean(nameError || connectorIdError || missingRequiredField);

	const handleSubmit = async () => {
		if (
			activeDraftMode == null ||
			activeConnectorDescriptor == null ||
			submitDisabled
		) {
			return;
		}

		setSubmitting(true);
		try {
			if (activeDraftMode === "create" && onCreateTarget) {
				await onCreateTarget(
					buildCreateRemoteStorageTargetPayload(
						form,
						activeConnectorDescriptor,
					),
				);
			} else if (editingTarget != null && onUpdateTarget) {
				await onUpdateTarget(
					editingTarget.target_key,
					buildUpdateRemoteStorageTargetPayload(
						form,
						activeConnectorDescriptor,
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
		connectorDescriptors,
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
									firstSupportedConnectorId == null ||
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
							firstSupportedConnectorId == null ||
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
					defaultToggleLocked={Boolean(defaultToggleLocked)}
					connectorDescriptors={supportedConnectorDescriptors}
					connectorIdError={connectorIdError}
					draftMode={activeDraftMode}
					form={form}
					nameError={nameError}
					onCancel={resetDraft}
					onFieldChange={setField}
					onSubmit={() => void handleSubmit()}
					submitDisabled={submitDisabled}
					submitting={submitting}
					targets={targets}
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
