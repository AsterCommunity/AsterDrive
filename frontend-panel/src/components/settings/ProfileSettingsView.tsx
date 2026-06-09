import {
	type ChangeEvent,
	type FormEvent,
	useEffect,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
	SettingsRow,
	SettingsSection,
} from "@/components/common/SettingsScaffold";
import { UserAvatarImage } from "@/components/common/UserAvatarImage";
import { AvatarCropDialog } from "@/components/settings/AvatarCropDialog";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { handleApiError } from "@/hooks/useApiError";
import { usePendingAction } from "@/hooks/usePendingAction";
import { getNormalizedDisplayName, getUserDisplayName } from "@/lib/user";
import { authService } from "@/services/authService";
import { useAuthStore } from "@/stores/authStore";
import type { AvatarInfo } from "@/types/api";

function getAvatarSourceLabelKey(source: AvatarInfo["source"]) {
	switch (source) {
		case "gravatar":
			return "settings_avatar_source_gravatar";
		case "upload":
			return "settings_avatar_source_upload";
		default:
			return "settings_avatar_source_none";
	}
}

function getAvatarSourceDescriptionKey(source: AvatarInfo["source"]) {
	switch (source) {
		case "gravatar":
			return "settings_avatar_gravatar_desc";
		case "upload":
			return "settings_avatar_upload_desc";
		default:
			return "settings_avatar_none_desc";
	}
}

export function ProfileSettingsView() {
	const { t } = useTranslation(["core", "files", "settings", "auth"]);
	const user = useAuthStore((s) => s.user);
	const refreshUser = useAuthStore((s) => s.refreshUser);
	const fileInputRef = useRef<HTMLInputElement | null>(null);
	const [avatarCropOpen, setAvatarCropOpen] = useState(false);
	const [avatarFile, setAvatarFile] = useState<File | null>(null);
	const {
		pending: avatarSourcePending,
		runWithPending: runAvatarSourceAction,
	} = usePendingAction();
	const {
		pending: avatarUploadPending,
		runWithPending: runAvatarUploadAction,
	} = usePendingAction();
	const { pending: profilePending, runWithPending: runProfileAction } =
		usePendingAction();
	const userDisplayNameValue = user?.profile.display_name ?? "";
	const [displayNameState, setDisplayNameState] = useState({
		source: userDisplayNameValue,
		value: userDisplayNameValue,
	});

	useEffect(() => {
		setDisplayNameState({
			source: userDisplayNameValue,
			value: userDisplayNameValue,
		});
	}, [userDisplayNameValue]);

	const displayNameValue = displayNameState.value;
	const currentDisplayName =
		getNormalizedDisplayName(user?.profile.display_name) ?? "";
	const previewDisplayName =
		getNormalizedDisplayName(displayNameValue) ?? getUserDisplayName(user);
	const displayNameChanged = displayNameValue.trim() !== currentDisplayName;
	const avatarSource = user?.profile.avatar.source ?? "none";
	const avatarBusy = avatarSourcePending || avatarUploadPending;

	const handleAvatarSelect = (event: ChangeEvent<HTMLInputElement>) => {
		const file = event.target.files?.[0];
		event.target.value = "";
		if (!file) return;
		setAvatarFile(file);
		setAvatarCropOpen(true);
	};

	const handleAvatarUpload = async (file: File) => {
		const result = await runAvatarUploadAction(async () => {
			try {
				await authService.uploadAvatar(file);
				await refreshUser();
				toast.success(t("settings:settings_avatar_updated"));
				return true;
			} catch (error) {
				handleApiError(error);
				return false;
			}
		});
		return result.entered ? result.value : false;
	};

	const handleAvatarCropOpenChange = (nextOpen: boolean) => {
		setAvatarCropOpen(nextOpen);
		if (!nextOpen) {
			setAvatarFile(null);
		}
	};

	const updateAvatarSource = async (source: "none" | "gravatar") => {
		await runAvatarSourceAction(async () => {
			try {
				await authService.setAvatarSource(source);
				await refreshUser();
				toast.success(t("settings:settings_avatar_source_updated"));
			} catch (error) {
				handleApiError(error);
			}
		});
	};

	const handleProfileSubmit = async (event: FormEvent<HTMLFormElement>) => {
		event.preventDefault();
		if (!user || !displayNameChanged) return;
		await runProfileAction(async () => {
			try {
				await authService.updateProfile({ display_name: displayNameValue });
				await refreshUser();
				toast.success(t("settings:settings_profile_updated"));
			} catch (error) {
				handleApiError(error);
			}
		});
	};

	return (
		<SettingsSection
			title={t("settings:settings_profile")}
			description={t("settings:settings_profile_desc")}
			contentClassName="pt-0"
		>
			<form
				className="divide-y"
				onSubmit={(event) => void handleProfileSubmit(event)}
			>
				<div className="grid gap-4 py-5 lg:grid-cols-[minmax(0,1fr)_minmax(240px,320px)] lg:items-center">
					<div className="flex min-w-0 items-center gap-4">
						<UserAvatarImage
							avatar={user?.profile.avatar ?? null}
							name={previewDisplayName}
							size="lg"
							className="size-20 shrink-0 ring-1 ring-border/35 sm:size-24"
						/>
						<div className="min-w-0 space-y-1">
							<p className="truncate text-base font-semibold">
								{previewDisplayName}
							</p>
							<p className="truncate text-sm text-muted-foreground">
								@{user?.username ?? ""}
							</p>
							{user?.email ? (
								<p className="truncate text-sm text-muted-foreground">
									{user.email}
								</p>
							) : null}
						</div>
					</div>
					<div className="rounded-lg border bg-muted/15 px-3 py-2.5">
						<p className="text-xs font-medium text-muted-foreground">
							{t("settings:settings_avatar_source")}
						</p>
						<p className="mt-1 text-sm font-medium">
							{t(`settings:${getAvatarSourceLabelKey(avatarSource)}`)}
						</p>
					</div>
				</div>

				<SettingsRow
					label={t("settings:settings_avatar")}
					description={t(
						`settings:${getAvatarSourceDescriptionKey(avatarSource)}`,
					)}
					className="py-5"
					controlClassName="md:max-w-[460px]"
				>
					<input
						ref={fileInputRef}
						type="file"
						aria-label={t("settings:settings_avatar_upload_and_crop")}
						accept="image/*"
						className="hidden"
						onChange={handleAvatarSelect}
					/>
					<div className="grid gap-2 sm:grid-cols-2">
						<Button
							type="button"
							size="sm"
							disabled={avatarBusy}
							onClick={() => fileInputRef.current?.click()}
						>
							{avatarUploadPending ? (
								<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
							) : (
								<Icon name="Upload" className="mr-1 size-4" />
							)}
							{t("settings:settings_avatar_upload_and_crop")}
						</Button>
						<Button
							type="button"
							variant="outline"
							size="sm"
							disabled={avatarBusy || avatarSource === "gravatar"}
							onClick={() => void updateAvatarSource("gravatar")}
						>
							{avatarSourcePending ? (
								<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
							) : (
								<Icon name="Globe" className="mr-1 size-4" />
							)}
							{t("settings:settings_use_gravatar")}
						</Button>
						<div className="sm:col-span-2 sm:flex sm:justify-end">
							<Button
								type="button"
								variant="ghost"
								size="sm"
								className="w-full justify-start text-muted-foreground sm:w-auto"
								disabled={avatarBusy || avatarSource === "none"}
								onClick={() => void updateAvatarSource("none")}
							>
								<Icon name="X" className="mr-1 size-4" />
								{t("settings:settings_remove_avatar")}
							</Button>
						</div>
					</div>
				</SettingsRow>

				<SettingsRow
					label={t("settings:settings_display_name")}
					description={t("settings:settings_display_name_hint", {
						username: user?.username ?? "",
					})}
					className="py-5"
					controlClassName="md:max-w-[460px]"
				>
					<Input
						value={displayNameValue}
						maxLength={64}
						disabled={profilePending}
						aria-label={t("settings:settings_display_name")}
						placeholder={t("settings:settings_display_name_placeholder")}
						onChange={(event) =>
							setDisplayNameState((prev) => ({
								...prev,
								value: event.target.value,
							}))
						}
					/>
				</SettingsRow>

				<SettingsRow
					label={t("settings:settings_account_readonly")}
					description={t("settings:settings_account_readonly_desc")}
					className="py-5"
					controlClassName="md:max-w-[520px]"
				>
					<div className="grid gap-3 sm:grid-cols-2">
						<div className="space-y-1.5">
							<p className="text-sm font-medium">{t("core:username")}</p>
							<Input
								readOnly
								value={user?.username ?? ""}
								aria-label={t("core:username")}
								className="font-mono text-sm"
							/>
							<p className="text-xs text-muted-foreground">
								{t("settings:settings_username_readonly_hint")}
							</p>
						</div>
						<div className="space-y-1.5">
							<p className="text-sm font-medium">{t("core:email")}</p>
							<Input
								readOnly
								value={user?.email ?? ""}
								aria-label={t("core:email")}
								className="text-sm"
							/>
							<p className="text-xs text-muted-foreground">
								{t("settings:settings_email_readonly_hint")}
							</p>
						</div>
					</div>
				</SettingsRow>

				<div className="flex justify-end py-4">
					<Button
						type="submit"
						className="min-w-24"
						disabled={profilePending || !displayNameChanged}
					>
						{profilePending ? (
							<Icon name="Spinner" className="mr-1 size-4 animate-spin" />
						) : null}
						{t("save")}
					</Button>
				</div>
			</form>

			<AvatarCropDialog
				open={avatarCropOpen}
				file={avatarFile}
				busy={avatarBusy}
				onOpenChange={handleAvatarCropOpenChange}
				onConfirm={handleAvatarUpload}
			/>
		</SettingsSection>
	);
}
