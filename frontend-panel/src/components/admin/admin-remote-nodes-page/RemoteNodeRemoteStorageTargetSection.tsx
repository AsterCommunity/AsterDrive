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
	driverDescriptors: RemoteStorageTargetDriverDescriptor[];
	errorMessage: string | null;
	loading: boolean;
	onCreateTarget: (payload: RemoteCreateStorageTargetRequest) => Promise<void>;
	onDeleteTarget: (target: RemoteStorageTargetInfo) => Promise<void>;
	onUpdateTarget: (
		targetKey: string,
		payload: RemoteUpdateStorageTargetRequest,
	) => Promise<void>;
	targets: RemoteStorageTargetInfo[];
}

export function RemoteNodeRemoteStorageTargetSection({
	driverDescriptors,
	errorMessage,
	loading,
	onCreateTarget,
	onDeleteTarget,
	onUpdateTarget,
	targets,
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
	const editingTarget =
		draftMode === "edit"
			? (targets.find((target) => target.target_key === editingTargetKey) ??
				null)
			: null;
	const activeDraftMode =
		draftMode === "edit" && editingTarget == null ? null : draftMode;
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
	const activePendingDeleteTargetKey = targets.some(
		(target) => target.target_key === pendingDeleteTargetKey,
	)
		? pendingDeleteTargetKey
		: null;

	const startCreate = () => {
		if (!firstSupportedDriverType) {
			return;
		}
		setDraftMode("create");
		setEditingTargetKey(null);
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
	const maxFileSizeValue = form.max_file_size.trim();
	const parsedMaxFileSize =
		maxFileSizeValue === "" ? 0 : Number(maxFileSizeValue);
	const maxFileSizeError =
		Number.isSafeInteger(parsedMaxFileSize) && parsedMaxFileSize >= 0
			? null
			: t("remote_node_ingress_profile_max_file_size_invalid");
	const localPathCandidate = form.base_path.trim().replaceAll("\\", "/");
	const localPathError =
		activeFieldNames.has("base_path") && form.driver_type === "local"
			? !form.base_path.trim()
				? t("remote_node_ingress_profile_base_path_required")
				: localPathCandidate.startsWith("/") ||
						/^[A-Za-z]:/.test(localPathCandidate) ||
						localPathCandidate.split("/").some((segment) => segment === "..")
					? t("remote_node_ingress_profile_base_path_relative")
					: null
			: null;
	const endpointError =
		activeFieldNames.has("endpoint") && !form.endpoint.trim()
			? t("remote_node_ingress_profile_endpoint_required")
			: null;
	const bucketError =
		activeFieldNames.has("bucket") && !form.bucket.trim()
			? t("remote_node_ingress_profile_bucket_required")
			: null;
	const requiresS3Credentials =
		activeFieldNames.has("access_key") &&
		(activeDraftMode === "create" || editingTarget?.driver_type !== "s3");
	const accessKeyError =
		requiresS3Credentials && !form.access_key.trim()
			? t("remote_node_ingress_profile_access_key_required")
			: null;
	const secretKeyError =
		requiresS3Credentials && !form.secret_key.trim()
			? t("remote_node_ingress_profile_secret_key_required")
			: null;
	const defaultToggleLocked =
		activeDraftMode === "edit" && editingTarget?.is_default;
	const submitDisabled =
		submitting ||
		Boolean(errorMessage) ||
		Boolean(
			nameError ||
				maxFileSizeError ||
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
			if (activeDraftMode === "create") {
				await onCreateTarget(
					buildCreateRemoteStorageTargetPayload(form, activeFieldNames),
				);
			} else if (editingTarget != null) {
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
		} finally {
			setSubmitting(false);
		}
	};

	const handleDeleteTarget = async (target: RemoteStorageTargetInfo) => {
		setPendingDeleteTargetKey(null);
		await onDeleteTarget(target);
		if (editingTargetKey === target.target_key) {
			resetDraft();
		}
	};

	return (
		<section className="rounded-2xl border border-border/70 bg-background/70 p-5">
			<div className="flex flex-wrap items-start justify-between gap-3">
				<div>
					<h3 className="text-base font-semibold text-foreground">
						{t("remote_node_ingress_profiles_title")}
					</h3>
					<p className="mt-1 text-sm text-muted-foreground">
						{t("remote_node_ingress_profiles_desc")}
					</p>
				</div>
				{activeDraftMode == null ? (
					<Button
						type="button"
						size="sm"
						className={ADMIN_CONTROL_HEIGHT_CLASS}
						onClick={startCreate}
						disabled={
							loading ||
							Boolean(errorMessage) ||
							firstSupportedDriverType == null
						}
					>
						<Icon name="Plus" className="mr-1 size-4" />
						{t("remote_node_ingress_profiles_create")}
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
					maxFileSizeError={maxFileSizeError}
					nameError={nameError}
					onCancel={resetDraft}
					onFieldChange={setField}
					onSubmit={() => void handleSubmit()}
					secretKeyError={secretKeyError}
					submitDisabled={submitDisabled}
					submitting={submitting}
				/>
			) : null}

			<RemoteNodeRemoteStorageTargetsList
				errorMessage={errorMessage}
				loading={loading}
				pendingDeleteTargetKey={activePendingDeleteTargetKey}
				onCancelDelete={() => setPendingDeleteTargetKey(null)}
				onConfirmDeleteTarget={(target) => void handleDeleteTarget(target)}
				onRequestDeleteTarget={(target) =>
					setPendingDeleteTargetKey(target.target_key)
				}
				onEditTarget={startEdit}
				targets={targets}
			/>
		</section>
	);
}
