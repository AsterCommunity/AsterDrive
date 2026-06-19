import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { writeTextToClipboard } from "@/lib/clipboard";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { formatDateTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { StoragePolicyCredentialInfo } from "@/types/api";
import { OneDriveApplicationFields } from "./OneDriveApplicationFields";
import { onedriveCredentialStatusReasonKey } from "./onedriveCredentialReason";
import { MICROSOFT_GRAPH_PROVIDER } from "./onedriveFieldUtils";
import type { SharedFieldProps, Translate } from "./StoragePolicyFieldTypes";

export function OneDriveCredentialPanel({
	authorizationPending,
	canStartAuthorization = true,
	canValidateCredential = true,
	credentials,
	form,
	loading,
	redirectUri,
	showApplicationFields = true,
	t,
	validationPending,
	onFieldChange,
	onStartAuthorization,
	onValidateCredential,
}: {
	authorizationPending: boolean;
	canStartAuthorization?: boolean;
	canValidateCredential?: boolean;
	credentials: StoragePolicyCredentialInfo[];
	form: SharedFieldProps["form"];
	loading: boolean;
	redirectUri: string;
	showApplicationFields?: boolean;
	t: Translate;
	validationPending: boolean;
	onFieldChange: SharedFieldProps["onFieldChange"];
	onStartAuthorization: () => void;
	onValidateCredential: () => void;
}) {
	const credential =
		credentials.find((item) => item.provider === MICROSOFT_GRAPH_PROVIDER) ??
		null;
	const status = credential?.status ?? "invalid";
	const statusClassName = loading
		? "border-muted-foreground/20 bg-muted text-muted-foreground"
		: status === "authorized"
			? "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
			: status === "reauth_required"
				? "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300"
				: "border-destructive/30 bg-destructive/10 text-destructive";
	const authorizedAt = formatOptionalDateTime(credential?.authorized_at);
	const refreshedAt = formatOptionalDateTime(credential?.last_refreshed_at);
	const validatedAt = formatOptionalDateTime(credential?.last_validated_at);
	const statusReason = onedriveCredentialStatusReason(
		credential?.status_reason,
		t,
	);
	const requiresReauth = credential?.status === "reauth_required";
	const copyRedirectUri = async () => {
		try {
			await writeTextToClipboard(redirectUri);
			toast.success(t("core:copied_to_clipboard"));
		} catch (error) {
			toast.error(error instanceof Error ? error.message : String(error));
		}
	};

	return (
		<div className="space-y-4 rounded-lg border border-sky-500/25 bg-sky-500/5 p-3">
			<div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
				<div className="min-w-0 space-y-1">
					<div className="flex flex-wrap items-center gap-2 text-sm font-medium">
						<Icon
							name="Key"
							className="size-4 shrink-0 text-sky-700 dark:text-sky-300"
						/>
						<span>{t("onedrive_credential_title")}</span>
						<Badge
							variant="outline"
							className={cn("shadow-sm", statusClassName)}
						>
							{loading
								? t("onedrive_credential_loading")
								: credential
									? t(`onedrive_credential_status_${credential.status}`)
									: t("onedrive_credential_status_missing")}
						</Badge>
					</div>
					<p className="text-xs leading-5 text-muted-foreground">
						{credential
							? t("onedrive_credential_desc_authorized")
							: t("onedrive_credential_desc_missing")}
					</p>
					{credential?.account_label || credential?.subject ? (
						<p className="text-xs text-muted-foreground">
							{credential.account_label ?? credential.subject}
						</p>
					) : null}
					{requiresReauth ? (
						<div className="mt-2 flex gap-2 rounded-md border border-amber-500/30 bg-amber-500/10 p-2 text-xs leading-5 text-amber-800 dark:text-amber-200">
							<Icon name="Warning" className="mt-0.5 size-4 shrink-0" />
							<div className="min-w-0 space-y-1">
								<p className="font-medium">
									{t("onedrive_credential_reauth_required_title")}
								</p>
								<p>
									{statusReason ??
										t("onedrive_credential_reason_reauth_required")}
								</p>
								<p>{t("onedrive_credential_reauth_required_desc")}</p>
							</div>
						</div>
					) : statusReason ? (
						<p className="text-xs text-amber-700 dark:text-amber-300">
							{statusReason}
						</p>
					) : null}
					{authorizedAt || refreshedAt || validatedAt ? (
						<p className="text-xs text-muted-foreground">
							{[
								authorizedAt
									? t("onedrive_credential_authorized_at", {
											time: authorizedAt,
										})
									: null,
								refreshedAt
									? t("onedrive_credential_refreshed_at", {
											time: refreshedAt,
										})
									: null,
								validatedAt
									? t("onedrive_credential_validated_at", {
											time: validatedAt,
										})
									: null,
							]
								.filter(Boolean)
								.join(" · ")}
						</p>
					) : null}
				</div>
				{canStartAuthorization || canValidateCredential ? (
					<div className="flex shrink-0 flex-wrap items-center gap-2">
						{canStartAuthorization ? (
							<Button
								type="button"
								variant="outline"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								disabled={authorizationPending}
								onClick={onStartAuthorization}
							>
								{authorizationPending ? (
									<Icon name="Spinner" className="mr-1 size-3.5 animate-spin" />
								) : (
									<Icon name="ArrowSquareOut" className="mr-1 size-3.5" />
								)}
								{credential
									? t("onedrive_reauthorize_action")
									: t("onedrive_authorize_action")}
							</Button>
						) : null}
						{canValidateCredential ? (
							<Button
								type="button"
								variant="outline"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								disabled={!credential || validationPending}
								onClick={onValidateCredential}
							>
								{validationPending ? (
									<Icon name="Spinner" className="mr-1 size-3.5 animate-spin" />
								) : (
									<Icon name="Check" className="mr-1 size-3.5" />
								)}
								{t("onedrive_validate_action")}
							</Button>
						) : null}
					</div>
				) : null}
			</div>
			<div className="space-y-2">
				<Label htmlFor="onedrive_redirect_uri">
					{t("onedrive_redirect_uri")}
				</Label>
				<div className="flex gap-2">
					<Input
						id="onedrive_redirect_uri"
						readOnly
						value={redirectUri}
						className="font-mono text-xs"
					/>
					<Button
						type="button"
						variant="outline"
						size="icon"
						onClick={() => void copyRedirectUri()}
						aria-label={t("onedrive_copy_redirect_uri")}
						title={t("onedrive_copy_redirect_uri")}
					>
						<Icon name="Copy" className="size-4" />
					</Button>
				</div>
				<p className="text-xs leading-5 text-muted-foreground">
					{t("onedrive_redirect_uri_desc")}
				</p>
			</div>
			{showApplicationFields ? (
				<OneDriveApplicationFields
					form={form}
					t={t}
					useSavedCredentialPlaceholder={credential != null}
					onFieldChange={onFieldChange}
				/>
			) : null}
		</div>
	);
}

function formatOptionalDateTime(value: string | null | undefined) {
	return value ? formatDateTime(value) : null;
}

function onedriveCredentialStatusReason(
	reason: string | null | undefined,
	t: Translate,
) {
	const key = onedriveCredentialStatusReasonKey(reason);
	return key ? t(key) : null;
}
