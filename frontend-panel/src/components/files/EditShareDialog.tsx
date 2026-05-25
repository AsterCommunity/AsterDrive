import { useEffect, useReducer } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
	normalizeMaxDownloads,
	toDateTimeLocalValue,
	toIsoDateTime,
} from "@/components/files/shareDialogShared";
import {
	type EditSharePasswordAction,
	editShareDialogReducer,
	initialEditShareDialogFormState,
} from "@/components/files/shareDialogState";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { handleApiError } from "@/hooks/useApiError";
import { useRetainedDialogValue } from "@/hooks/useRetainedDialogValue";
import { shareService } from "@/services/shareService";
import type { MyShareInfo } from "@/types/api";

interface EditShareDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	share: MyShareInfo | null;
	onSaved?: () => void | Promise<void>;
}

export function EditShareDialog({
	open,
	onOpenChange,
	share: inputShare,
	onSaved,
}: EditShareDialogProps) {
	const { t } = useTranslation(["core", "share"]);
	const { retainedValue: share, handleOpenChangeComplete } =
		useRetainedDialogValue(inputShare, open);
	const [formState, dispatchForm] = useReducer(
		editShareDialogReducer,
		initialEditShareDialogFormState,
	);
	const { expiresAt, loading, maxDownloads, password, passwordAction } =
		formState;
	const passwordActionOptions = [
		{ label: t("share:my_shares_edit_password_keep"), value: "keep" },
		{ label: t("share:my_shares_edit_password_clear"), value: "clear" },
		{ label: t("share:my_shares_edit_password_set"), value: "set" },
	] satisfies ReadonlyArray<{ label: string; value: EditSharePasswordAction }>;

	useEffect(() => {
		if (!open || !share) return;

		dispatchForm({
			type: "resetForShare",
			expiresAt: toDateTimeLocalValue(share.expires_at),
			maxDownloads: String(share.max_downloads),
		});
	}, [open, share]);

	if (!share) return null;

	const handleSave = async (event: React.FormEvent) => {
		event.preventDefault();

		if (passwordAction === "set" && password.trim().length === 0) {
			toast.error(t("share:share_edit_password_required"));
			return;
		}

		dispatchForm({ type: "saveStarted" });
		try {
			await shareService.update(share.id, {
				password:
					passwordAction === "keep"
						? undefined
						: passwordAction === "clear"
							? ""
							: password.trim(),
				expires_at: toIsoDateTime(expiresAt),
				max_downloads: normalizeMaxDownloads(maxDownloads),
			});
			toast.success(t("share:my_shares_edit_success"));
			onOpenChange(false);
			await onSaved?.();
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchForm({ type: "saveFinished" });
		}
	};

	return (
		<Dialog
			open={open}
			onOpenChange={onOpenChange}
			onOpenChangeComplete={handleOpenChangeComplete}
		>
			<DialogContent className="max-w-md">
				<DialogHeader>
					<DialogTitle className="flex items-center gap-2">
						<Icon name="PencilSimple" className="size-4" />
						{t("share:my_shares_edit_title", { name: share.resource_name })}
					</DialogTitle>
					<DialogDescription>
						{t("share:my_shares_edit_desc")}
					</DialogDescription>
				</DialogHeader>

				<form onSubmit={handleSave} className="space-y-4">
					<div className="space-y-2">
						<Label>{t("share:my_shares_edit_password_mode")}</Label>
						<Select
							items={passwordActionOptions}
							value={passwordAction}
							onValueChange={(value) =>
								dispatchForm({
									type: "setPasswordAction",
									value: (value as EditSharePasswordAction | null) ?? "keep",
								})
							}
						>
							<SelectTrigger>
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{passwordActionOptions.map((option) => (
									<SelectItem key={option.value} value={option.value}>
										{option.label}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</div>

					{passwordAction === "set" && (
						<div className="space-y-2">
							<Label htmlFor="edit-share-password">
								{t("share:share_password_optional")}
							</Label>
							<Input
								id="edit-share-password"
								type="password"
								autoComplete="new-password"
								placeholder={t("share:share_password_placeholder")}
								value={password}
								onChange={(event) =>
									dispatchForm({
										type: "setPassword",
										value: event.target.value,
									})
								}
							/>
						</div>
					)}

					<div className="space-y-2">
						<Label htmlFor="edit-share-expires-at">
							{t("share:share_expiration")}
						</Label>
						<Input
							id="edit-share-expires-at"
							type="datetime-local"
							value={expiresAt}
							onChange={(event) =>
								dispatchForm({
									type: "setExpiresAt",
									value: event.target.value,
								})
							}
						/>
						<p className="text-xs text-muted-foreground">
							{t("share:my_shares_edit_expiry_hint")}
						</p>
					</div>

					<div className="space-y-2">
						<Label htmlFor="edit-share-max-downloads">
							{t("share:share_download_limit")}
						</Label>
						<Input
							id="edit-share-max-downloads"
							type="number"
							min={0}
							placeholder={t("share:share_download_limit_placeholder")}
							value={maxDownloads}
							onChange={(event) =>
								dispatchForm({
									type: "setMaxDownloads",
									value: event.target.value,
								})
							}
						/>
					</div>

					<div className="flex items-center justify-end gap-2 pt-2">
						<Button
							type="button"
							variant="outline"
							onClick={() => onOpenChange(false)}
						>
							{t("core:cancel")}
						</Button>
						<Button type="submit" disabled={loading}>
							{loading ? t("core:save") : t("core:save")}
						</Button>
					</div>
				</form>
			</DialogContent>
		</Dialog>
	);
}
