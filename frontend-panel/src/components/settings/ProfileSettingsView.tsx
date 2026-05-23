import {
	type ChangeEvent,
	type FormEvent,
	useEffect,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { SettingsSection } from "@/components/common/SettingsScaffold";
import { UserAvatarImage } from "@/components/common/UserAvatarImage";
import { AvatarCropDialog } from "@/components/settings/AvatarCropDialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { handleApiError } from "@/hooks/useApiError";
import { getNormalizedDisplayName, getUserDisplayName } from "@/lib/user";
import { authService } from "@/services/authService";
import { useAuthStore } from "@/stores/authStore";

export function ProfileSettingsView() {
	const { t } = useTranslation(["core", "files", "settings", "auth"]);
	const user = useAuthStore((s) => s.user);
	const refreshUser = useAuthStore((s) => s.refreshUser);
	const fileInputRef = useRef<HTMLInputElement | null>(null);
	const [avatarBusy, setAvatarBusy] = useState(false);
	const [avatarCropOpen, setAvatarCropOpen] = useState(false);
	const [avatarFile, setAvatarFile] = useState<File | null>(null);
	const [profileBusy, setProfileBusy] = useState(false);
	const [displayNameValue, setDisplayNameValue] = useState("");

	const currentDisplayName =
		getNormalizedDisplayName(user?.profile.display_name) ?? "";
	const previewDisplayName =
		getNormalizedDisplayName(displayNameValue) ?? getUserDisplayName(user);
	const displayNameChanged = displayNameValue.trim() !== currentDisplayName;

	useEffect(() => {
		setDisplayNameValue(user?.profile.display_name ?? "");
	}, [user?.profile.display_name]);

	const handleAvatarSelect = (event: ChangeEvent<HTMLInputElement>) => {
		const file = event.target.files?.[0];
		event.target.value = "";
		if (!file) return;
		setAvatarFile(file);
		setAvatarCropOpen(true);
	};

	const handleAvatarUpload = async (file: File) => {
		try {
			setAvatarBusy(true);
			await authService.uploadAvatar(file);
			await refreshUser();
			toast.success(t("settings:settings_avatar_updated"));
			return true;
		} catch (error) {
			handleApiError(error);
			return false;
		} finally {
			setAvatarBusy(false);
		}
	};

	const handleAvatarCropOpenChange = (nextOpen: boolean) => {
		setAvatarCropOpen(nextOpen);
		if (!nextOpen) {
			setAvatarFile(null);
		}
	};

	const updateAvatarSource = async (source: "none" | "gravatar") => {
		try {
			setAvatarBusy(true);
			await authService.setAvatarSource(source);
			await refreshUser();
			toast.success(t("settings:settings_avatar_source_updated"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setAvatarBusy(false);
		}
	};

	const handleProfileSubmit = async (event: FormEvent<HTMLFormElement>) => {
		event.preventDefault();
		if (!user || !displayNameChanged) return;
		try {
			setProfileBusy(true);
			await authService.updateProfile({ display_name: displayNameValue });
			await refreshUser();
			toast.success(t("settings:settings_profile_updated"));
		} catch (error) {
			handleApiError(error);
		} finally {
			setProfileBusy(false);
		}
	};

	return (
		<SettingsSection
			title={t("settings:settings_profile")}
			description={t("settings:settings_profile_desc")}
			contentClassName="pt-4"
		>
			<form
				className="grid gap-5 rounded-xl border bg-muted/20 p-4 lg:grid-cols-[260px_minmax(0,1fr)]"
				onSubmit={(event) => void handleProfileSubmit(event)}
			>
				<div className="rounded-xl border bg-background p-4">
					<div className="flex h-full flex-col gap-4">
						<div className="flex items-center gap-3">
							<UserAvatarImage
								avatar={user?.profile.avatar ?? null}
								name={previewDisplayName}
								size="lg"
								className="size-24 ring-1 ring-border/35"
							/>
							<div className="min-w-0 flex-1 space-y-1.5">
								<p className="truncate text-sm font-semibold">
									{previewDisplayName}
								</p>
								<p className="truncate text-xs text-muted-foreground">
									@{user?.username ?? ""}
								</p>
								{user?.email ? (
									<p className="truncate text-xs text-muted-foreground">
										{user.email}
									</p>
								) : null}
							</div>
						</div>
						<div className="mt-auto space-y-2 border-t pt-4">
							<input
								ref={fileInputRef}
								type="file"
								aria-label={t("settings:settings_avatar_upload_and_crop")}
								accept="image/*"
								className="hidden"
								onChange={handleAvatarSelect}
							/>
							<Button
								type="button"
								className="w-full"
								size="sm"
								disabled={avatarBusy}
								onClick={() => fileInputRef.current?.click()}
							>
								{t("settings:settings_avatar_upload_and_crop")}
							</Button>
							<Button
								type="button"
								variant="outline"
								className="w-full"
								size="sm"
								disabled={avatarBusy}
								onClick={() => void updateAvatarSource("gravatar")}
							>
								{t("settings:settings_use_gravatar")}
							</Button>
							<Button
								type="button"
								variant="ghost"
								className="w-full justify-start px-0 text-muted-foreground"
								size="sm"
								disabled={avatarBusy}
								onClick={() => void updateAvatarSource("none")}
							>
								{t("settings:settings_remove_avatar")}
							</Button>
						</div>
					</div>
				</div>

				<div className="space-y-4">
					<div className="space-y-1.5">
						<p className="text-sm font-medium">
							{t("settings:settings_display_name")}
						</p>
						<Input
							value={displayNameValue}
							maxLength={64}
							disabled={profileBusy}
							aria-label={t("settings:settings_display_name")}
							placeholder={t("settings:settings_display_name_placeholder")}
							onChange={(event) => setDisplayNameValue(event.target.value)}
						/>
						<p className="text-xs text-muted-foreground">
							{t("settings:settings_display_name_hint", {
								username: user?.username ?? "",
							})}
						</p>
					</div>

					<div className="grid gap-4 md:grid-cols-2">
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

					<div className="flex justify-end border-t pt-4">
						<Button
							type="submit"
							className="min-w-24"
							disabled={profileBusy || !displayNameChanged}
						>
							{t("save")}
						</Button>
					</div>
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
