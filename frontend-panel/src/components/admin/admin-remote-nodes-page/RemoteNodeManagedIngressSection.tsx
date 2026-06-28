import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
	buildCreateManagedIngressProfilePayload,
	buildUpdateManagedIngressProfilePayload,
	emptyManagedIngressProfileForm,
	getManagedIngressProfileForm,
	isManagedIngressDriverType,
	type ManagedIngressDriverType,
	type ManagedIngressProfileFormData,
} from "@/components/admin/managedIngressProfileDialogShared";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import type {
	ManagedIngressDriverDescriptor,
	RemoteCreateIngressProfileRequest,
	RemoteIngressProfileInfo,
	RemoteUpdateIngressProfileRequest,
} from "@/types/api";
import { RemoteNodeManagedIngressForm } from "./RemoteNodeManagedIngressForm";
import { RemoteNodeManagedIngressProfilesList } from "./RemoteNodeManagedIngressProfilesList";

type SupportedManagedIngressDriverDescriptor =
	ManagedIngressDriverDescriptor & {
		driver_type: ManagedIngressDriverType;
	};

interface RemoteNodeManagedIngressSectionProps {
	driverDescriptors: ManagedIngressDriverDescriptor[];
	errorMessage: string | null;
	loading: boolean;
	onCreateProfile: (
		payload: RemoteCreateIngressProfileRequest,
	) => Promise<void>;
	onDeleteProfile: (profile: RemoteIngressProfileInfo) => Promise<void>;
	onUpdateProfile: (
		profileKey: string,
		payload: RemoteUpdateIngressProfileRequest,
	) => Promise<void>;
	profiles: RemoteIngressProfileInfo[];
}

export function RemoteNodeManagedIngressSection({
	driverDescriptors,
	errorMessage,
	loading,
	onCreateProfile,
	onDeleteProfile,
	onUpdateProfile,
	profiles,
}: RemoteNodeManagedIngressSectionProps) {
	const { t } = useTranslation("admin");
	const [draftMode, setDraftMode] = useState<"create" | "edit" | null>(null);
	const [editingProfileKey, setEditingProfileKey] = useState<string | null>(
		null,
	);
	const [form, setForm] = useState<ManagedIngressProfileFormData>(
		emptyManagedIngressProfileForm,
	);
	const [submitting, setSubmitting] = useState(false);
	const [pendingDeleteProfileKey, setPendingDeleteProfileKey] = useState<
		string | null
	>(null);
	const editingProfile =
		draftMode === "edit"
			? (profiles.find(
					(profile) => profile.profile_key === editingProfileKey,
				) ?? null)
			: null;
	const activeDraftMode =
		draftMode === "edit" && editingProfile == null ? null : draftMode;
	const supportedDriverDescriptors = driverDescriptors.flatMap(
		(descriptor): SupportedManagedIngressDriverDescriptor[] =>
			isManagedIngressDriverType(descriptor.driver_type)
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
	const activePendingDeleteProfileKey = profiles.some(
		(profile) => profile.profile_key === pendingDeleteProfileKey,
	)
		? pendingDeleteProfileKey
		: null;

	const startCreate = () => {
		if (!firstSupportedDriverType) {
			return;
		}
		setDraftMode("create");
		setEditingProfileKey(null);
		setForm({
			...emptyManagedIngressProfileForm,
			driver_type: firstSupportedDriverType,
			is_default: profiles.length === 0,
		});
	};

	const startEdit = (profile: RemoteIngressProfileInfo) => {
		setDraftMode("edit");
		setEditingProfileKey(profile.profile_key);
		setForm(getManagedIngressProfileForm(profile));
	};

	const resetDraft = () => {
		setDraftMode(null);
		setEditingProfileKey(null);
		setForm(emptyManagedIngressProfileForm);
	};

	const setField = <K extends keyof ManagedIngressProfileFormData>(
		key: K,
		value: ManagedIngressProfileFormData[K],
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
		(activeDraftMode === "create" || editingProfile?.driver_type !== "s3");
	const accessKeyError =
		requiresS3Credentials && !form.access_key.trim()
			? t("remote_node_ingress_profile_access_key_required")
			: null;
	const secretKeyError =
		requiresS3Credentials && !form.secret_key.trim()
			? t("remote_node_ingress_profile_secret_key_required")
			: null;
	const defaultToggleLocked =
		activeDraftMode === "edit" && editingProfile?.is_default;
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
				await onCreateProfile(buildCreateManagedIngressProfilePayload(form));
			} else if (editingProfile != null) {
				await onUpdateProfile(
					editingProfile.profile_key,
					buildUpdateManagedIngressProfilePayload(form, editingProfile),
				);
			}
			resetDraft();
		} finally {
			setSubmitting(false);
		}
	};

	const handleDeleteProfile = async (profile: RemoteIngressProfileInfo) => {
		setPendingDeleteProfileKey(null);
		await onDeleteProfile(profile);
		if (editingProfileKey === profile.profile_key) {
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
				<RemoteNodeManagedIngressForm
					accessKeyError={accessKeyError}
					bucketError={bucketError}
					defaultToggleLocked={Boolean(defaultToggleLocked)}
					driverDescriptors={supportedDriverDescriptors}
					driverTypeError={driverTypeError}
					draftMode={activeDraftMode}
					editingProfile={editingProfile}
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

			<RemoteNodeManagedIngressProfilesList
				errorMessage={errorMessage}
				loading={loading}
				pendingDeleteProfileKey={activePendingDeleteProfileKey}
				onCancelDelete={() => setPendingDeleteProfileKey(null)}
				onConfirmDeleteProfile={(profile) => void handleDeleteProfile(profile)}
				onRequestDeleteProfile={(profile) =>
					setPendingDeleteProfileKey(profile.profile_key)
				}
				onEditProfile={startEdit}
				profiles={profiles}
			/>
		</section>
	);
}
