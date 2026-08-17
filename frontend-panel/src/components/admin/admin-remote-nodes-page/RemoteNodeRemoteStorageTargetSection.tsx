import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
	buildCreateRemoteStorageTargetPayload,
	buildUpdateRemoteStorageTargetPayload,
	createRemoteStorageTargetForm,
	emptyRemoteStorageTargetForm,
	getRemoteStorageTargetForm,
	type RemoteStorageTargetFieldValue,
	type RemoteStorageTargetFormData,
} from "@/components/admin/remoteStorageTargetDialogShared";
import { AnimatedCollapsible } from "@/components/common/AnimatedCollapsible";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type {
	RemoteCreateStorageTargetRequest,
	RemoteStorageTargetConnectorDescriptor,
	RemoteStorageTargetInfo,
	RemoteUpdateStorageTargetRequest,
} from "@/types/api";
import { RemoteNodeRemoteStorageTargetForm } from "./RemoteNodeRemoteStorageTargetForm";
import { RemoteNodeRemoteStorageTargetsList } from "./RemoteNodeRemoteStorageTargetsList";

interface Props {
	allowCreate?: boolean;
	createLabelKey?: string;
	descriptionKey?: string;
	connectorDescriptors?: RemoteStorageTargetConnectorDescriptor[];
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
}: Props) {
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
		(readOnly && !allowCreate) || (draftMode === "edit" && !editingTarget)
			? null
			: draftMode;
	const descriptor =
		connectorDescriptors.find(
			(item) => item.connector_id === form.connector_id,
		) ?? null;
	const canCreate =
		Boolean(onCreateTarget) &&
		(!readOnly || allowCreate) &&
		connectorDescriptors.length > 0;

	const reset = () => {
		setDraftMode(null);
		setEditingTargetKey(null);
		setForm(emptyRemoteStorageTargetForm);
	};
	const startCreate = () => {
		if (!canCreate) return;
		setDraftMode("create");
		setEditingTargetKey(null);
		setReadOnlyOpen(true);
		setForm(
			createRemoteStorageTargetForm(
				connectorDescriptors[0],
				targets.length === 0,
			),
		);
	};
	const startEdit = (target: RemoteStorageTargetInfo) => {
		const targetDescriptor = connectorDescriptors.find(
			(item) => item.connector_id === target.connector_id,
		);
		if (!targetDescriptor) return;
		setDraftMode("edit");
		setEditingTargetKey(target.target_key);
		setForm(getRemoteStorageTargetForm(target, targetDescriptor));
	};
	const setField = (
		key: "name" | "connector_id" | "is_default" | "value",
		value:
			| string
			| boolean
			| { name: string; value: RemoteStorageTargetFieldValue },
	) => {
		setForm((current) => {
			if (key === "value" && typeof value === "object")
				return {
					...current,
					values: { ...current.values, [value.name]: value.value },
				};
			if (key === "connector_id" && typeof value === "string") {
				const next = connectorDescriptors.find(
					(item) => item.connector_id === value,
				);
				return next
					? {
							...createRemoteStorageTargetForm(next, current.is_default),
							name: current.name,
						}
					: current;
			}
			return { ...current, [key]: value } as RemoteStorageTargetFormData;
		});
	};

	const fieldErrors = new Map<string, string>();
	if (!form.name.trim())
		fieldErrors.set("name", t("remote_node_ingress_profile_name_required"));
	if (!descriptor)
		fieldErrors.set(
			"connector_id",
			t("remote_node_storage_target_connector_unsupported"),
		);
	for (const field of descriptor?.fields ?? []) {
		const value = form.values[field.name] ?? "";
		const blank = typeof value === "string" && !value.trim();
		const preservesSecret =
			activeDraftMode === "edit" &&
			editingTarget?.connector_id === form.connector_id &&
			editingTarget.credential_configured &&
			field.scope === "static_credential";
		if (field.required && blank && !(field.secret && preservesSecret))
			fieldErrors.set(
				field.name,
				t(
					field.required_message_key ??
						"remote_node_ingress_profile_field_required",
					{ field: t(field.label_key) },
				),
			);
		if (
			typeof value === "string" &&
			field.validation?.max_length != null &&
			value.length > field.validation.max_length
		)
			fieldErrors.set(
				field.name,
				t("remote_node_ingress_profile_field_too_long"),
			);
		if (
			typeof value === "number" &&
			field.validation?.min_integer != null &&
			value < field.validation.min_integer
		)
			fieldErrors.set(
				field.name,
				t("remote_node_ingress_profile_field_invalid_number"),
			);
		if (
			typeof value === "number" &&
			field.validation?.max_integer != null &&
			value > field.validation.max_integer
		)
			fieldErrors.set(
				field.name,
				t("remote_node_ingress_profile_field_invalid_number"),
			);
	}
	const submitDisabled =
		submitting || Boolean(errorMessage) || fieldErrors.size > 0;
	const submit = async () => {
		if (!activeDraftMode || !descriptor || submitDisabled) return;
		setSubmitting(true);
		try {
			if (activeDraftMode === "create" && onCreateTarget)
				await onCreateTarget(
					buildCreateRemoteStorageTargetPayload(form, descriptor),
				);
			else if (editingTarget && onUpdateTarget)
				await onUpdateTarget(
					editingTarget.target_key,
					buildUpdateRemoteStorageTargetPayload(
						form,
						descriptor,
						editingTarget,
					),
				);
			reset();
		} catch {
		} finally {
			setSubmitting(false);
		}
	};
	const remove = async (target: RemoteStorageTargetInfo) => {
		if (!onDeleteTarget) return;
		setPendingDeleteTargetKey(null);
		await onDeleteTarget(target);
		if (editingTargetKey === target.target_key) reset();
	};
	const list = (
		<RemoteNodeRemoteStorageTargetsList
			connectorDescriptors={connectorDescriptors}
			errorMessage={errorMessage}
			loading={loading}
			pendingDeleteTargetKey={pendingDeleteTargetKey}
			onCancelDelete={() => setPendingDeleteTargetKey(null)}
			onConfirmDeleteTarget={(target) => void remove(target)}
			onRequestDeleteTarget={(target) =>
				setPendingDeleteTargetKey(target.target_key)
			}
			onEditTarget={startEdit}
			targets={targets}
			readOnly={readOnly}
		/>
	);
	const Root = surface === "card" ? "section" : "div";
	return (
		<Root
			className={
				surface === "card"
					? "rounded-2xl border border-border/70 bg-background/70 p-5"
					: "space-y-4 border-t border-border/70 pt-4"
			}
		>
			<div className="flex flex-wrap items-start justify-between gap-3">
				<div>
					<h3 className="text-base font-semibold">{t(titleKey)}</h3>
					<p className="mt-1 text-sm text-muted-foreground">
						{t(descriptionKey)}
					</p>
				</div>
				<div className="flex gap-2">
					{activeDraftMode == null && canCreate ? (
						<Button
							type="button"
							size="sm"
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							onClick={startCreate}
							disabled={loading || Boolean(errorMessage)}
						>
							<Icon name="Plus" className="mr-1 size-4" aria-hidden />
							{t(createLabelKey)}
						</Button>
					) : null}
					{readOnly ? (
						<Button
							type="button"
							variant="outline"
							size="sm"
							aria-expanded={readOnlyOpen}
							onClick={() => setReadOnlyOpen((open) => !open)}
						>
							{t(
								readOnlyOpen
									? "policy_remote_storage_targets_hide"
									: "policy_remote_storage_targets_show",
							)}
						</Button>
					) : null}
				</div>
			</div>
			{errorMessage ? (
				<div className="mt-4 rounded-xl border border-destructive/30 p-4 text-sm text-destructive">
					{errorMessage}
				</div>
			) : null}
			{activeDraftMode ? (
				<RemoteNodeRemoteStorageTargetForm
					defaultToggleLocked={Boolean(
						activeDraftMode === "edit" && editingTarget?.is_default,
					)}
					connectorDescriptors={connectorDescriptors}
					draftMode={activeDraftMode}
					editingProfile={editingTarget}
					fieldErrors={fieldErrors}
					form={form}
					onCancel={reset}
					onFieldChange={setField}
					onSubmit={() => void submit()}
					submitDisabled={submitDisabled}
					submitting={submitting}
				/>
			) : null}
			{readOnly ? (
				<AnimatedCollapsible
					open={readOnlyOpen}
					contentClassName="max-h-[min(52vh,28rem)] overflow-y-auto pr-1"
				>
					{list}
				</AnimatedCollapsible>
			) : (
				<div className={listViewportClassName}>{list}</div>
			)}
		</Root>
	);
}
