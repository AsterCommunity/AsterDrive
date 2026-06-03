import { useCallback, useEffect, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { handleApiError } from "@/hooks/useApiError";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { adminPolicyService } from "@/services/adminService";
import type { GoogleDrivePolicyAuthStatus } from "@/types/api";
import type { SharedFieldProps } from "./StoragePolicyFieldTypes";

export function GoogleDriveConnectionFields({
	clientIdError,
	clientSecretError,
	form,
	isCreateMode,
	onFieldChange,
	showCreateValidation = false,
	t,
}: SharedFieldProps & {
	clientIdError: string | null;
	clientSecretError: string | null;
	isCreateMode: boolean;
	showCreateValidation?: boolean;
}) {
	const appDataFolderEnabled = form.google_drive_use_app_data_folder;

	return (
		<>
			<div className="grid gap-4 md:grid-cols-2">
				<div className="space-y-2">
					<Label htmlFor="google_drive_client_id">
						{t("google_drive_client_id")}
					</Label>
					<Input
						id="google_drive_client_id"
						name="storage-policy-google-drive-client-id"
						value={form.access_key}
						onChange={(e) => onFieldChange("access_key", e.target.value)}
						autoComplete="off"
						aria-invalid={
							showCreateValidation && clientIdError ? true : undefined
						}
						className={ADMIN_CONTROL_HEIGHT_CLASS}
					/>
					{showCreateValidation && clientIdError ? (
						<p className="text-xs text-destructive">{clientIdError}</p>
					) : null}
				</div>
				<div className="space-y-2">
					<Label htmlFor="google_drive_client_secret">
						{t("google_drive_client_secret")}
					</Label>
					<Input
						id="google_drive_client_secret"
						name="storage-policy-google-drive-client-secret"
						type="password"
						value={form.secret_key}
						onChange={(e) => onFieldChange("secret_key", e.target.value)}
						autoComplete="new-password"
						aria-invalid={
							showCreateValidation && clientSecretError ? true : undefined
						}
						className={ADMIN_CONTROL_HEIGHT_CLASS}
						placeholder={
							isCreateMode
								? undefined
								: t("policy_editor_credentials_keep_placeholder")
						}
					/>
					{showCreateValidation && clientSecretError ? (
						<p className="text-xs text-destructive">{clientSecretError}</p>
					) : null}
				</div>
			</div>

			<div className="grid gap-4 md:grid-cols-2">
				<div className="space-y-2">
					<Label htmlFor="google_drive_root_folder_id">
						{t("google_drive_root_folder_id")}
					</Label>
					<Input
						id="google_drive_root_folder_id"
						value={form.google_drive_root_folder_id}
						onChange={(e) =>
							onFieldChange("google_drive_root_folder_id", e.target.value)
						}
						className={ADMIN_CONTROL_HEIGHT_CLASS}
						disabled={appDataFolderEnabled}
						placeholder="root"
					/>
				</div>
				<div className="space-y-2">
					<Label htmlFor="google_drive_shared_drive_id">
						{t("google_drive_shared_drive_id")}
					</Label>
					<Input
						id="google_drive_shared_drive_id"
						value={form.google_drive_shared_drive_id}
						onChange={(e) =>
							onFieldChange("google_drive_shared_drive_id", e.target.value)
						}
						className={ADMIN_CONTROL_HEIGHT_CLASS}
						disabled={appDataFolderEnabled}
					/>
				</div>
			</div>

			<div className="space-y-2 pt-1">
				<div className="flex items-center gap-2">
					<Switch
						id="google_drive_use_app_data_folder"
						checked={form.google_drive_use_app_data_folder}
						onCheckedChange={(value) => {
							onFieldChange("google_drive_use_app_data_folder", value);
							if (value) {
								onFieldChange("google_drive_root_folder_id", "");
								onFieldChange("google_drive_shared_drive_id", "");
							}
						}}
					/>
					<Label htmlFor="google_drive_use_app_data_folder">
						{t("google_drive_use_app_data_folder")}
					</Label>
				</div>
				<p className="text-xs text-muted-foreground">
					{t("google_drive_app_data_folder_desc")}
				</p>
			</div>
		</>
	);
}

export function GoogleDriveAuthorizationPanel({
	policyId,
	t,
}: {
	policyId: number | null;
	t: SharedFieldProps["t"];
}) {
	const [status, setStatus] = useState<GoogleDrivePolicyAuthStatus | null>(
		null,
	);
	const [loading, setLoading] = useState(false);
	const [starting, setStarting] = useState(false);

	const loadStatus = useCallback(async () => {
		if (policyId == null) {
			setStatus(null);
			return;
		}

		setLoading(true);
		try {
			setStatus(await adminPolicyService.getGoogleDriveAuthStatus(policyId));
		} catch (error) {
			handleApiError(error);
			setStatus(null);
		} finally {
			setLoading(false);
		}
	}, [policyId]);

	useEffect(() => {
		void loadStatus();
	}, [loadStatus]);

	const startAuth = async () => {
		if (policyId == null || starting) {
			return;
		}

		setStarting(true);
		try {
			const returnPath = `${window.location.pathname}${window.location.search}`;
			const response = await adminPolicyService.startGoogleDriveAuth(policyId, {
				return_path: returnPath,
			});
			window.location.assign(response.authorization_url);
		} catch (error) {
			handleApiError(error);
			setStarting(false);
		}
	};

	const accountLabel =
		status?.account_email ||
		status?.account_name ||
		t("google_drive_no_account");
	const authorized = status?.authorized === true;
	const statusTone = authorized
		? "border-emerald-500/50 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
		: "border-amber-500/50 bg-amber-500/10 text-amber-700 dark:text-amber-300";

	return (
		<div className="rounded-2xl border border-border/70 bg-muted/20 p-4">
			<div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
				<div className="min-w-0 space-y-2">
					<div className="flex flex-wrap items-center gap-2">
						<p className="text-sm font-medium">
							{t("google_drive_authorization")}
						</p>
						<Badge variant="outline" className={statusTone}>
							{loading
								? t("google_drive_auth_checking")
								: authorized
									? t("google_drive_auth_authorized")
									: t("google_drive_auth_required")}
						</Badge>
					</div>
					<div className="grid gap-1 text-xs text-muted-foreground">
						<span className="truncate">
							{t("google_drive_account")}: {accountLabel}
						</span>
						<span className="truncate">
							{t("google_drive_root")}: {status?.root || "root"}
						</span>
						{status?.last_error ? (
							<span className="text-destructive">
								{t("google_drive_last_error")}: {status.last_error}
							</span>
						) : null}
					</div>
				</div>
				<div className="flex shrink-0 gap-2">
					<Button
						type="button"
						variant="outline"
						className={ADMIN_CONTROL_HEIGHT_CLASS}
						disabled={loading || starting}
						onClick={() => void loadStatus()}
					>
						<Icon
							name={loading ? "Spinner" : "ArrowsClockwise"}
							className={`mr-1 size-3.5 ${loading ? "animate-spin" : ""}`}
						/>
						{t("core:refresh")}
					</Button>
					<Button
						type="button"
						className={ADMIN_CONTROL_HEIGHT_CLASS}
						disabled={starting}
						onClick={() => void startAuth()}
					>
						<Icon
							name={starting ? "Spinner" : "Key"}
							className={`mr-1 size-3.5 ${starting ? "animate-spin" : ""}`}
						/>
						{authorized
							? t("google_drive_reauthorize")
							: t("google_drive_authorize")}
					</Button>
				</div>
			</div>
			{authorized ? (
				<p className="mt-3 text-xs text-muted-foreground">
					{t("google_drive_test_connection_hint")}
				</p>
			) : (
				<p className="mt-3 text-xs text-muted-foreground">
					{t("google_drive_authorization_hint")}
				</p>
			)}
		</div>
	);
}
